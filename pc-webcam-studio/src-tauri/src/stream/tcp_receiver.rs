use crate::gpu::wgpu_pipeline::{FilterUniforms, WgpuPipeline};
use crate::sinks::virtual_cam_ffi::VirtualCamWriter;
use crate::sinks::wasapi_mic::WasapiMicSink;
use crate::stream::control_client::ControlClient;
use crate::stream::hw_decoder::HardwareDecoder;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamStatus {
    pub gpu_error: Option<String>,
    pub phase: String,
    pub message: String,
    pub serial: Option<String>,
    pub frames: u64,
    pub decode_errors: u64,
    pub skipped_frames: u64,
    pub bytes: u64,
    pub fps: f32,
    pub width: u32,
    pub height: u32,
    pub filter_ms: f32,
    pub receive_latency_ms: Option<f32>,
    pub decode_ms: f32,
    pub estimated_latency_ms: Option<f32>,
    pub clock_uncertainty_ms: Option<f32>,
    pub virtual_camera_active: bool,
    pub manifest: Option<serde_json::Value>,
    pub phone: Option<serde_json::Value>,
}
impl Default for StreamStatus {
    fn default() -> Self {
        Self {
            gpu_error: None,
            phase: "idle".into(),
            message: "Connect your phone to get started".into(),
            serial: None,
            frames: 0,
            decode_errors: 0,
            skipped_frames: 0,
            bytes: 0,
            fps: 0.0,
            width: 0,
            height: 0,
            filter_ms: 0.0,
            receive_latency_ms: None,
            decode_ms: 0.0,
            estimated_latency_ms: None,
            clock_uncertainty_ms: None,
            virtual_camera_active: false,
            manifest: None,
            phone: None,
        }
    }
}
pub struct Preview {
    pub sequence: u32,
    pub bytes: Vec<u8>,
}
pub struct StreamPipeline {
    pub is_running: AtomicBool,
    pub generation: AtomicU64,
    pub wgpu: Mutex<Option<WgpuPipeline>>,
    pub uniforms: Mutex<FilterUniforms>,
    pub wasapi: tokio::sync::Mutex<WasapiMicSink>,
    pub status: Mutex<StreamStatus>,
    pub preview: Mutex<Preview>,
    preview_ready: tokio::sync::Notify,
    pub clock_origin: Instant,
    clock: Mutex<Option<(f64, f64)>>,
}
impl StreamPipeline {
    pub fn new() -> Self {
        Self {
            is_running: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            wgpu: Mutex::new(None),
            uniforms: Mutex::new(FilterUniforms::default()),
            wasapi: tokio::sync::Mutex::new(WasapiMicSink::new()),
            status: Mutex::new(StreamStatus::default()),
            clock_origin: Instant::now(),
            clock: Mutex::new(None),
            preview: Mutex::new(Preview {
                sequence: 0,
                bytes: vec![],
            }),
            preview_ready: tokio::sync::Notify::new(),
        }
    }
    pub fn observe_clock(&self, phone_us: f64, sent_us: f64) {
        let received_us = self.clock_origin.elapsed().as_secs_f64() * 1e6;
        let rtt = received_us - sent_us;
        if rtt > 100_000.0 {
            return;
        }
        let mut clock = self.clock.lock().unwrap();
        if clock.map(|(_, best)| rtt <= best * 1.25).unwrap_or(true) {
            *clock = Some((phone_us - (sent_us + received_us) / 2.0, rtt));
            self.status.lock().unwrap().clock_uncertainty_ms = Some((rtt / 2000.0) as f32);
        }
    }
    pub fn phase(&self, app: &AppHandle, phase: &str, message: &str) {
        let mut s = self.status.lock().unwrap();
        s.phase = phase.into();
        s.message = message.into();
        let _ = app.emit("stream-status", s.clone());
    }
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.is_running.store(false, Ordering::SeqCst);
        self.preview.lock().unwrap().bytes.clear();
        *self.clock.lock().unwrap() = None;
    }
}
pub async fn start_stream_receiver(
    app: Option<AppHandle>,
    pipeline: Arc<StreamPipeline>,
    generation: u64,
    control: Option<ControlClient>,
) -> anyhow::Result<()> {
    let mut stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect("127.0.0.1:27183"),
    )
    .await??;
    stream.set_nodelay(true)?;
    let mut flow_header = [0u8; 13];
    flow_header[0] = 10;
    stream.write_all(&flow_header).await?;

    let return_enabled = Arc::new(AtomicBool::new(false));
    let task = tokio::spawn(return_preview(
        pipeline.clone(),
        generation,
        return_enabled.clone(),
    ));
    let _return_guard = AbortOnDrop(task.abort_handle());
    let render_tx = start_renderer(pipeline.clone(), generation, app.clone());
    let video_decoder = start_video_decoder(pipeline.clone(), render_tx.clone());
    let key_request = Arc::new(AtomicBool::new(false));
    let mut header = [0u8; 13];
    let mut rotation = 0;
    while pipeline.generation.load(Ordering::SeqCst) == generation {
        tokio::time::timeout(Duration::from_secs(8), stream.read_exact(&mut header)).await??;
        if pipeline.generation.load(Ordering::SeqCst) != generation {
            break;
        }
        let size = u32::from_be_bytes(header[9..13].try_into().unwrap()) as usize;
        anyhow::ensure!(size <= 32 * 1024 * 1024, "Invalid media packet size");
        let mut payload = vec![0; size];
        tokio::time::timeout(Duration::from_secs(8), stream.read_exact(&mut payload)).await??;
        if matches!(header[0], 1 | 7 | 8) {
            let mut ack = [0u8; 13];
            ack[0] = 9;
            ack[1..9].copy_from_slice(&header[1..9]);
            stream.write_all(&ack).await?;
        }
        pipeline.status.lock().unwrap().bytes += size as u64;
        if matches!(header[0], 1 | 7 | 8) {
            let pts = u64::from_be_bytes(header[1..9].try_into().unwrap()) as f64;
            let clock = *pipeline.clock.lock().unwrap();
            pipeline.status.lock().unwrap().receive_latency_ms = clock.map(|(offset, _)| {
                ((pipeline.clock_origin.elapsed().as_secs_f64() * 1e6 - (pts - offset)) / 1000.)
                    .max(0.) as f32
            });
        }
        match header[0] {
            1 => {
                let pts = u64::from_be_bytes(header[1..9].try_into().unwrap()) as f64;
                let capture_us = pipeline
                    .clock
                    .lock()
                    .unwrap()
                    .map(|(offset, _)| pts - offset);
                if video_decoder.send(payload, capture_us, rotation)
                    && !key_request.swap(true, Ordering::SeqCst)
                {
                    let control = control.clone();
                    let pending = key_request.clone();
                    tokio::spawn(async move {
                        if let Some(control) = control {
                            let _ = control
                                .send_command(serde_json::json!({"cmd":"REQUEST_KEYFRAME"}))
                                .await;
                        }
                        pending.store(false, Ordering::SeqCst);
                    });
                }
            }
            7 | 8 => {
                let pts = u64::from_be_bytes(header[1..9].try_into().unwrap()) as f64;
                let capture_us = pipeline
                    .clock
                    .lock()
                    .unwrap()
                    .map(|(offset, _)| pts - offset);
                let (width, height) = if header[0] == 7 {
                    anyhow::ensure!(payload.len() >= 8, "Short uncompressed frame");
                    let w = u32::from_be_bytes(payload[0..4].try_into().unwrap());
                    let h = u32::from_be_bytes(payload[4..8].try_into().unwrap());
                    anyhow::ensure!(
                        w > 0
                            && h > 0
                            && w % 2 == 0
                            && h % 2 == 0
                            && (w as u64 * h as u64) <= 3840 * 2160,
                        "Invalid frame dimensions"
                    );
                    anyhow::ensure!(
                        payload.len() == 8 + (w * h * 3 / 2) as usize,
                        "Invalid uncompressed frame size"
                    );
                    (w, h)
                } else {
                    (0, 0)
                };
                render_tx.send(DecodedFrame {
                    rgba: payload,
                    kind: header[0],
                    width,
                    height,
                    capture_us,
                    rotation,
                });
            }
            2 => pipeline.wasapi.lock().await.write_pcm_chunk(&payload),
            3 => {
                if let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&payload) {
                    pipeline.status.lock().unwrap().manifest = Some(manifest.clone());
                    if let Some(app) = &app {
                        let _ = app.emit("device-capabilities", manifest);
                    }
                }
            }
            6 => return_enabled.store(payload.first() == Some(&1), Ordering::Relaxed),
            11 => {
                let metadata: serde_json::Value = serde_json::from_slice(&payload)?;
                rotation = valid_rotation(metadata["rotation"].as_u64())?;
            }
            _ => anyhow::bail!("Unknown media packet type {}", header[0]),
        }
    }
    Ok(())
}

struct AbortOnDrop(tokio::task::AbortHandle);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct DecodedFrame {
    rgba: Vec<u8>,
    kind: u8,
    width: u32,
    height: u32,
    capture_us: Option<f64>,
    rotation: u32,
}
fn valid_rotation(value: Option<u64>) -> anyhow::Result<u32> {
    match value {
        Some(v @ (0 | 90 | 180 | 270)) => Ok(v as u32),
        _ => anyhow::bail!("Invalid camera orientation"),
    }
}
#[derive(Clone)]
struct RenderSender {
    latest: Arc<Mutex<Option<DecodedFrame>>>,
    wake: std::sync::mpsc::SyncSender<()>,
}
impl RenderSender {
    fn send(&self, frame: DecodedFrame) {
        // Decode every reference frame, but render only the newest completed image.
        *self.latest.lock().unwrap() = Some(frame);
        let _ = self.wake.try_send(());
    }
}
fn start_renderer(
    pipeline: Arc<StreamPipeline>,
    generation: u64,
    app: Option<AppHandle>,
) -> RenderSender {
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    let latest = Arc::new(Mutex::new(None::<DecodedFrame>));
    let pending = latest.clone();
    std::thread::Builder::new()
        .name("Webcam image processing".into())
        .spawn(move || {
            prioritize_capture_thread();
            if pipeline.wgpu.lock().unwrap().is_none() {
                match WgpuPipeline::new() {
                    Ok(mut gpu) => {
                        gpu.update_uniforms(*pipeline.uniforms.lock().unwrap());
                        *pipeline.wgpu.lock().unwrap() = Some(gpu);
                    }
                    Err(e) => log::error!("GPU initialization: {e}"),
                }
            }
            let mut vcam = VirtualCamWriter::new();
            let mut last_preview = Instant::now() - Duration::from_secs(1);
            let mut fps_time = Instant::now();
            let mut fps_frames = 0u32;

            while rx.recv().is_ok() {
                let Some(frame) = pending.lock().unwrap().take() else {
                    continue;
                };
                if pipeline.generation.load(Ordering::SeqCst) != generation {
                    break;
                }
                let independent=frame.kind!=0;
                let decode_started=Instant::now();
                let Some(frame)=decode_independent(frame,&pipeline) else {continue;};
                if independent {pipeline.status.lock().unwrap().decode_ms=decode_started.elapsed().as_secs_f32()*1000.;}
                let DecodedFrame {
                    rgba,
                    kind:_,
                    width: w,
                    height: h,
                    capture_us,
                    rotation,
                } = frame;
                let started = Instant::now();
                let mut uniforms = *pipeline.uniforms.lock().unwrap();
                uniforms.rotation = (uniforms.rotation + rotation) % 360;
                uniforms.width = w as f32;
                uniforms.height = h as f32;
                let neutral = uniforms.is_neutral();
                let filtered = if neutral {
                    None
                } else {
                    let mut guard = pipeline.wgpu.lock().unwrap();
                    if let Some(gpu) = guard.as_mut() {gpu.update_uniforms(uniforms);}
                    match guard.as_mut().map(|g| g.process_frame(&rgba, w, h)) {
                        Some(Ok(frame)) => Some(frame),
                        Some(Err(e)) => {
                            log::error!("GPU filtering disabled: {e}");
                            pipeline.status.lock().unwrap().gpu_error = Some(e.to_string());
                            *guard = None;
                            None
                        }
                        None => None,
                    }
                };
                let pixels = filtered
                    .as_ref()
                    .map(|f| f.rgba.as_slice())
                    .unwrap_or(&rgba);
                let (w, h) = filtered
                    .as_ref()
                    .map(|f| (f.width, f.height))
                    .unwrap_or((w, h));
                let filter_ms = started.elapsed().as_secs_f32() * 1000.0;
                let active = vcam.write_frame(pixels, w, h);
                fps_frames += 1;
                {
                    let mut status = pipeline.status.lock().unwrap();
                    status.frames += 1;
                    if status.frames % 300 == 0 {
                        log::info!(
                            "Stream health: {} frames, {:.1} fps, {} decode errors, capture-to-output {:?} ms, clock ±{:?} ms",
                            status.frames,
                            status.fps,
                            status.decode_errors,
                            status.estimated_latency_ms, status.clock_uncertainty_ms
                        );
                    }
                    status.width = w;
                    status.height = h;
                    status.filter_ms = filter_ms;
                    status.estimated_latency_ms = capture_us.map(|capture| {
                        ((pipeline.clock_origin.elapsed().as_secs_f64() * 1e6 - capture) / 1000.0)
                            .max(0.0) as f32
                    });
                    status.virtual_camera_active = active;
                    if fps_time.elapsed() >= Duration::from_secs(1) {
                        status.fps = fps_frames as f32 / fps_time.elapsed().as_secs_f32();
                        fps_frames = 0;
                        fps_time = Instant::now();
                    }
                    status.phase = "live".into();
                    status.message = "Your camera is live".into();
                }
                if last_preview.elapsed() >= Duration::from_millis(25) {
                    last_preview = Instant::now();
                    let sample = (w.max(h) + 639) / 640;
                    let pw = w / sample;
                    let ph = h / sample;
                    let mut preview = pipeline.preview.lock().unwrap();
                    preview.sequence = preview.sequence.wrapping_add(1);
                    let sequence = preview.sequence;
                    let mut bytes = Vec::with_capacity(24 + (pw * ph * 4) as usize);
                    for val in [pw, ph, w, h, filter_ms.to_bits(), sequence] {
                        bytes.extend_from_slice(&val.to_le_bytes());
                    }
                    for y in 0..ph {
                        for x in 0..pw {
                            let i = ((y * sample * w + x * sample) * 4) as usize;
                            bytes.extend_from_slice(&pixels[i..i + 4]);
                        }
                    }
                    preview.bytes = bytes;
                    drop(preview);
                    pipeline.preview_ready.notify_one();
                    if let Some(app)=&app {let _=app.emit("preview-ready",sequence);}
                }
            }
        })
        .expect("Could not start image processing");
    RenderSender { latest, wake: tx }
}

/// Return the exact processed output as a small JPEG. Its own task and bounded
/// writes keep phone display work out of capture, decoding, and virtual output.
async fn return_preview(pipeline: Arc<StreamPipeline>, generation: u64, enabled: Arc<AtomicBool>) {
    let mut last_sent = Instant::now() - Duration::from_secs(1);
    let mut sequence = 0;
    let mut logged = false;
    let mut writer: Option<TcpStream> = None;
    loop {
        let _ = tokio::time::timeout(
            Duration::from_millis(500),
            pipeline.preview_ready.notified(),
        )
        .await;
        if pipeline.generation.load(Ordering::SeqCst) != generation {
            break;
        }
        if !enabled.load(Ordering::Relaxed) {
            continue;
        }
        // Coalesce while encoding/writing, then take the newest available frame.
        // No phase-dependent 66ms polling delay is added to the phone preview.
        if let Some(wait) = Duration::from_millis(33).checked_sub(last_sent.elapsed()) {
            tokio::time::sleep(wait).await;
        }
        if writer.is_none() {
            match tokio::time::timeout(
                Duration::from_millis(500),
                TcpStream::connect("127.0.0.1:27185"),
            )
            .await
            {
                Ok(Ok(stream)) => {
                    let _ = stream.set_nodelay(true);
                    writer = Some(stream);
                }
                _ => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }
            }
        }
        let data = {
            let preview = pipeline.preview.lock().unwrap();
            if preview.sequence == sequence || preview.bytes.len() < 24 {
                continue;
            }
            sequence = preview.sequence;
            preview.bytes.clone()
        };
        last_sent = Instant::now();
        let encoded = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
            let w = u32::from_le_bytes(data[0..4].try_into()?);
            let h = u32::from_le_bytes(data[4..8].try_into()?);
            let rgb: Vec<u8> = data[24..]
                .chunks_exact(4)
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 75).encode(
                &rgb,
                w,
                h,
                image::ExtendedColorType::Rgb8,
            )?;
            Ok(jpeg)
        })
        .await;
        let Ok(Ok(jpeg)) = encoded else {
            continue;
        };
        let mut header = [0u8; 13];
        header[0] = 4;
        header[9..13].copy_from_slice(&(jpeg.len() as u32).to_be_bytes());
        let sent = tokio::time::timeout(Duration::from_millis(150), async {
            writer.as_mut().unwrap().write_all(&header).await?;
            writer.as_mut().unwrap().write_all(&jpeg).await
        })
        .await;
        if matches!(sent, Ok(Ok(()))) && !logged {
            log::info!("Phone broadcast preview active");
            logged = true;
        }
        if !matches!(sent, Ok(Ok(()))) {
            writer = None;
            logged = false;
        }
    }
}

fn decode_independent(mut frame: DecodedFrame, pipeline: &StreamPipeline) -> Option<DecodedFrame> {
    match frame.kind {
        0 => Some(frame),
        7 => {
            let w = frame.width as usize;
            let h = frame.height as usize;
            let n = w * h;
            let uv = n / 4;
            let src = &frame.rgba[8..];
            let mut rgba = vec![0; n * 4];
            for y in 0..h {
                for x in 0..w {
                    let luma = src[y * w + x] as i32 - 16;
                    let u = src[n + (y / 2) * (w / 2) + x / 2] as i32 - 128;
                    let v = src[n + uv + (y / 2) * (w / 2) + x / 2] as i32 - 128;
                    let i = (y * w + x) * 4;
                    rgba[i] = ((298 * luma + 409 * v + 128) >> 8).clamp(0, 255) as u8;
                    rgba[i + 1] = ((298 * luma - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8;
                    rgba[i + 2] = ((298 * luma + 516 * u + 128) >> 8).clamp(0, 255) as u8;
                    rgba[i + 3] = 255;
                }
            }
            frame.rgba = rgba;
            frame.kind = 0;
            Some(frame)
        }
        8 => {
            let decoded = (|| -> anyhow::Result<image::RgbaImage> {
                use image::ImageDecoder;
                let decoder =
                    image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&frame.rgba))?;
                let (w, h) = decoder.dimensions();
                anyhow::ensure!(
                    w > 0 && h > 0 && (w as u64 * h as u64) <= 3840 * 2160,
                    "JPEG dimensions too large"
                );
                Ok(image::DynamicImage::from_decoder(decoder)?.into_rgba8())
            })();
            match decoded {
                Ok(image) => {
                    frame.width = image.width();
                    frame.height = image.height();
                    frame.rgba = image.into_raw();
                    frame.kind = 0;
                    Some(frame)
                }
                Err(e) => {
                    pipeline.status.lock().unwrap().decode_errors += 1;
                    log::warn!("Independent frame decode: {e}");
                    None
                }
            }
        }
        _ => None,
    }
}

struct EncodedFrame {
    payload: Vec<u8>,
    capture_us: Option<f64>,
    rotation: u32,
    epoch: u64,
}
struct EncodedQueue {
    frames: VecDeque<EncodedFrame>,
    await_key: bool,
    epoch: u64,
}
struct VideoDecoderSender {
    queue: Arc<Mutex<EncodedQueue>>,
    wake: std::sync::mpsc::SyncSender<()>,
    pipeline: Arc<StreamPipeline>,
}
impl VideoDecoderSender {
    fn send(&self, payload: Vec<u8>, capture_us: Option<f64>, rotation: u32) -> bool {
        let key = payload[..payload.len().min(128)]
            .windows(4)
            .any(|p| p[0] == 0 && p[1] == 0 && p[2] == 1 && matches!(p[3] & 31, 5 | 7));
        let mut queue = self.queue.lock().unwrap();
        let mut request = false;
        if queue.frames.len() >= 2 {
            self.pipeline.status.lock().unwrap().skipped_frames += queue.frames.len() as u64;
            queue.frames.clear();
            queue.epoch += 1;
            queue.await_key = true;
            request = true;
        }
        if queue.await_key && !key {
            return request;
        }
        queue.await_key = false;
        let epoch = queue.epoch;
        queue.frames.push_back(EncodedFrame {
            payload,
            capture_us,
            rotation,
            epoch,
        });
        drop(queue);
        let _ = self.wake.try_send(());
        request && !key
    }
}
fn start_video_decoder(pipeline: Arc<StreamPipeline>, render: RenderSender) -> VideoDecoderSender {
    let queue = Arc::new(Mutex::new(EncodedQueue {
        frames: VecDeque::new(),
        await_key: true,
        epoch: 0,
    }));
    let pending = queue.clone();
    let statistics = pipeline.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    std::thread::Builder::new()
        .name("Webcam video decoder".into())
        .spawn(move || {
            prioritize_capture_thread();
            let mut decoder = match HardwareDecoder::new() {
                Ok(d) => d,
                Err(e) => {
                    log::error!("Decoder initialization: {e}");
                    return;
                }
            };
            let mut epoch = 0;
            while rx.recv().is_ok() {
                loop {
                    let frame = pending.lock().unwrap().frames.pop_front();
                    let Some(frame) = frame else {
                        break;
                    };
                    if epoch != frame.epoch {
                        match HardwareDecoder::new() {
                            Ok(next) => decoder = next,
                            Err(_) => break,
                        }
                        epoch = frame.epoch;
                    }
                    let decoding = Instant::now();
                    if let Err(e) = decoder.decode_packet(&frame.payload, |rgba, width, height| {
                        pipeline.status.lock().unwrap().decode_ms =
                            decoding.elapsed().as_secs_f32() * 1000.;
                        if pending.lock().unwrap().epoch == frame.epoch {
                            render.send(DecodedFrame {
                                rgba,
                                kind: 0,
                                width,
                                height,
                                capture_us: frame.capture_us,
                                rotation: frame.rotation,
                            });
                        }
                    }) {
                        pipeline.status.lock().unwrap().decode_errors += 1;
                        log::debug!("Decode: {e}");
                    }
                }
            }
        })
        .expect("Could not start video decoding");
    VideoDecoderSender {
        queue,
        wake: tx,
        pipeline: statistics,
    }
}
fn prioritize_capture_thread() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
        };
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
    }
}

#[cfg(test)]
mod orientation_tests {
    use super::*;
    #[test]
    fn accepts_only_quarter_turns() {
        for angle in [0, 90, 180, 270] {
            assert_eq!(valid_rotation(Some(angle)).unwrap(), angle as u32);
        }
        for angle in [1, 45, 360, u64::MAX] {
            assert!(valid_rotation(Some(angle)).is_err());
        }
        assert!(valid_rotation(None).is_err());
    }
}
