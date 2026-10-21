use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{
    atomic::{AtomicBool, AtomicI32, Ordering},
    Arc,
};

pub struct WasapiMicSink {
    sender: Option<SyncSender<Vec<u8>>>,
    running: Arc<AtomicBool>,
    meter: Arc<AtomicI32>,
}
impl WasapiMicSink {
    pub fn new() -> Self {
        Self {
            sender: None,
            running: Arc::new(AtomicBool::new(false)),
            meter: Arc::new(AtomicI32::new(-900)),
        }
    }
    pub fn get_db_level(&self) -> f32 {
        self.meter.load(Ordering::Relaxed) as f32 / 10.0
    }
    pub fn set_enabled(&mut self, enabled: bool) -> anyhow::Result<()> {
        if enabled && self.running.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.running.store(false, Ordering::Relaxed);
        self.sender = None;
        self.meter.store(-900, Ordering::Relaxed);
        if !enabled {
            return Ok(());
        }
        let (tx, rx) = sync_channel::<Vec<u8>>(4);
        let (ready_tx, ready_rx) = sync_channel(1);
        let running = Arc::new(AtomicBool::new(true));
        self.running = running.clone();
        let meter = self.meter.clone();
        std::thread::Builder::new().name("Phone microphone".into()).spawn(move||{
            let setup=||->anyhow::Result<cpal::Stream>{
                let host=cpal::default_host();
                let device=host.output_devices()?.find(|d|d.name().map(|n|n.contains("CABLE Input")).unwrap_or(false))
                    .ok_or_else(||anyhow::anyhow!("Install VB-Audio Virtual Cable to use the phone microphone. Then select CABLE Output in your meeting app."))?;
                let range=device.supported_output_configs()?.find(|c|c.sample_format()==cpal::SampleFormat::F32 && c.min_sample_rate().0<=48000 && c.max_sample_rate().0>=48000)
                    .ok_or_else(||anyhow::anyhow!("Virtual cable does not support 48 kHz floating-point audio"))?;
                let config:cpal::StreamConfig=range.with_sample_rate(cpal::SampleRate(48000)).into();
                let channels=config.channels as usize;
                let buffer=Arc::new(std::sync::Mutex::new(VecDeque::<f32>::with_capacity(2400)));
                let consumer=buffer.clone();let producer=buffer.clone();let flag=running.clone();let db=meter.clone();
                let stream=device.build_output_stream(&config,move|out:&mut[f32],_|{
                    if let Ok(mut q)=consumer.lock(){for frame in out.chunks_mut(channels){let sample=q.pop_front().unwrap_or(0.0);for channel in frame{*channel=sample;}}}
                    else{out.fill(0.0);}
                },|e|log::error!("Audio output: {e}"),None)?;
                stream.play()?;
                // The receiver owns only PCM data; the stream stays on this audio thread.
                std::thread::spawn(move||{
                    while flag.load(Ordering::Relaxed){
                        let Ok(bytes)=rx.recv_timeout(std::time::Duration::from_millis(200)) else {continue;};
                        let mut sum=0.0f64;let n=bytes.len()/2;if n==0{continue;}
                        if let Ok(mut q)=producer.lock(){for p in bytes.chunks_exact(2){let s=i16::from_le_bytes([p[0],p[1]]) as f32/32768.0;sum+=(s as f64).powi(2);if q.len()>=2400{q.pop_front();}q.push_back(s);}}
                        db.store((200.0*(sum/n as f64).sqrt().max(0.00003).log10()) as i32,Ordering::Relaxed);
                    }
                });
                Ok(stream)
            };
            match setup(){
                Ok(_stream)=>{let _=ready_tx.send(Ok(()));while running.load(Ordering::Relaxed){std::thread::sleep(std::time::Duration::from_millis(50));}}
                Err(e)=>{running.store(false,Ordering::Relaxed);let _=ready_tx.send(Err(e.to_string()));}
            }
            meter.store(-900,Ordering::Relaxed);
        })?;
        match ready_rx.recv_timeout(std::time::Duration::from_secs(4)) {
            Ok(Ok(())) => {
                self.sender = Some(tx);
                Ok(())
            }
            Ok(Err(e)) => {
                self.running.store(false, Ordering::Relaxed);
                anyhow::bail!("{e}")
            }
            Err(e) => {
                self.running.store(false, Ordering::Relaxed);
                anyhow::bail!("Audio startup failed: {e}")
            }
        }
    }
    pub fn write_pcm_chunk(&self, bytes: &[u8]) {
        if let Some(tx) = &self.sender {
            let _ = tx.try_send(bytes.to_vec());
        }
    }
}
impl Drop for WasapiMicSink {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
