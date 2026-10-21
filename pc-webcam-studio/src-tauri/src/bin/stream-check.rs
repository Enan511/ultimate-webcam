use pc_webcam_studio_lib::stream::{
    control_client::ControlClient,
    tcp_receiver::{start_stream_receiver, StreamPipeline},
};
use std::{sync::Arc, time::Duration};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let seconds = arg("--seconds")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(15);
    let camera = arg("--camera").unwrap_or_else(|| "0".into());
    let profile = arg("--profile").unwrap_or_else(|| "1080".into());
    let pipeline = Arc::new(StreamPipeline::new());
    let control = ControlClient::new();
    control.connect("127.0.0.1:27184").await?;
    control
        .send_command(serde_json::json!({"cmd":"START_STREAM"}))
        .await?;
    let width = arg("--width")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(if profile == "minimum" { 320 } else { 1920 });
    control.send_command(serde_json::json!({"cmd":"CONFIGURE","cameraId":camera,"width":width,"height":if width<=640{width*3/4}else{1080},"fps":30,"bitrate":15000000,"transport":arg("--transport").unwrap_or_else(||if width<=640{"jpeg".into()}else{"h264".into()})})).await?;
    for _ in 0..12 {
        let sent = pipeline.clock_origin.elapsed().as_secs_f64() * 1e6;
        let reply = control
            .send_command(serde_json::json!({"cmd":"GET_STATE"}))
            .await?;
        if let Some(clock) = reply["clockUs"].as_f64() {
            pipeline.observe_clock(clock, sent);
        }
    }
    if args.iter().any(|a| a == "--no-auto") {
        control
            .send_command(serde_json::json!({"cmd":"SET_AUTO_ORIENTATION","enabled":false}))
            .await?;
    }
    let filtered = std::env::args().any(|a| a == "--filtered");
    if filtered {
        let mut uniforms = pipeline.uniforms.lock().unwrap();
        uniforms.rotation = 90;
        uniforms.lut_preset = 2;
    }
    let receiver = tokio::spawn(start_stream_receiver(
        None,
        pipeline.clone(),
        0,
        Some(control.clone()),
    ));
    let mut ages = Vec::new();
    let mut previous_frames = 0;
    let mut stale_ticks = 0;
    let mut longest_stall_ms = 0;
    for tick in 0..seconds * 10 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if receiver.is_finished() {
            receiver.await??;
            anyhow::bail!("Stream ended early");
        }
        let status = pipeline.status.lock().unwrap().clone();
        if status.frames == previous_frames {
            stale_ticks += 1;
        } else {
            stale_ticks = 0;
        }
        if tick > 20 {
            longest_stall_ms = longest_stall_ms.max(stale_ticks * 100);
        }
        previous_frames = status.frames;
        if tick % 50 == 0 {
            let sent = pipeline.clock_origin.elapsed().as_secs_f64() * 1e6;
            let reply = control
                .send_command(serde_json::json!({"cmd":"GET_STATE"}))
                .await?;
            if let Some(clock) = reply["clockUs"].as_f64() {
                pipeline.observe_clock(clock, sent);
            }
        }
        if tick % 100 == 0 {
            println!(
                "Progress: {}s, {} frames, {:.1} fps, {:?}",
                tick / 10,
                status.frames,
                status.fps,
                status.estimated_latency_ms
            );
        }
        if status.frames > 30 {
            if let Some(age) = status.estimated_latency_ms {
                ages.push(age);
            }
        }
    }
    let status = pipeline.status.lock().unwrap().clone();
    let bytes = pipeline.preview.lock().unwrap().bytes.clone();
    anyhow::ensure!(bytes.len() > 24, "No preview generated");
    let w = u32::from_le_bytes(bytes[0..4].try_into()?);
    let h = u32::from_le_bytes(bytes[4..8].try_into()?);
    let suffix = format!(
        "{}-camera{}-{}",
        format!("{}-{}", profile, arg("--transport").unwrap_or_default()),
        camera,
        if filtered { "filtered" } else { "neutral" }
    );
    image::save_buffer(
        format!("artifacts/processed-{suffix}.png"),
        &bytes[24..],
        w,
        h,
        image::ExtendedColorType::Rgba8,
    )?;
    ages.sort_by(f32::total_cmp);
    let median = ages.get(ages.len() / 2).copied();
    let p95 = ages.get(ages.len() * 95 / 100).copied();
    let report = serde_json::json!({"filtered":filtered,"profile":profile,"camera":camera,"longestStallMs":longest_stall_ms,"status":status,"samples":ages.len(),"medianCaptureToOutputMs":median,"p95CaptureToOutputMs":p95});
    std::fs::write(
        format!("artifacts/pipeline-{suffix}.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    pipeline.cancel();
    receiver.abort();
    control.disconnect().await;
    anyhow::ensure!(
        status.frames > seconds * 20 && status.fps > 20.0,
        "Stream throughput was too low"
    );
    anyhow::ensure!(status.gpu_error.is_none(), "GPU filtering failed");
    anyhow::ensure!(
        longest_stall_ms < 1000,
        "Video stalled for {longest_stall_ms} ms"
    );
    anyhow::ensure!(p95.unwrap_or(1000.0) < 500.0, "Latency exceeds 500ms");
    Ok(())
}
