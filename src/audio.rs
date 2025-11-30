use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SizedSample};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use log::{error, info};

pub const WHISPER_SAMPLE_RATE: u32 = 16000;

enum Cmd {
    Start,
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

pub struct AudioRecorder {
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

unsafe impl Send for AudioRecorder {}

impl AudioRecorder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            cmd_tx: None,
            worker_handle: None,
        })
    }

    pub fn start_recording(&mut self) -> Result<()> {
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Start).map_err(|e| anyhow::anyhow!("Failed to send Start command: {}", e))?;
        } else {
            self.init_stream()?;
            if let Some(tx) = &self.cmd_tx {
                 tx.send(Cmd::Start).map_err(|e| anyhow::anyhow!("Failed to send Start command: {}", e))?;
            }
        }
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Result<Vec<f32>> {
        let (resp_tx, resp_rx) = mpsc::channel();
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Stop(resp_tx)).map_err(|e| anyhow::anyhow!("Failed to send Stop command: {}", e))?;
            let samples = resp_rx.recv().map_err(|e| anyhow::anyhow!("Failed to receive samples: {}", e))?;
            return Ok(samples);
        }
        Ok(Vec::new())
    }

    fn init_stream(&mut self) -> Result<()> {
        if self.worker_handle.is_some() {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(anyhow::anyhow!("No input device found"))?;

        let (sample_tx, sample_rx) = mpsc::channel::<Vec<f32>>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();

        let worker = thread::spawn(move || {
            if let Err(e) = run_audio_thread(device, sample_tx, sample_rx, cmd_rx) {
                error!("Audio thread error: {}", e);
            }
        });

        self.cmd_tx = Some(cmd_tx);
        self.worker_handle = Some(worker);

        Ok(())
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
    }
}

fn run_audio_thread(
    device: Device,
    sample_tx: mpsc::Sender<Vec<f32>>,
    sample_rx: mpsc::Receiver<Vec<f32>>,
    cmd_rx: mpsc::Receiver<Cmd>,
) -> Result<()> {
    let config = get_preferred_config(&device)?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    info!("Audio device: {:?}, Rate: {}, Channels: {}, Format: {:?}", device.name().unwrap_or_default(), sample_rate, channels, config.sample_format());

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::I8 => build_stream::<i8>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::U8 => build_stream::<u8>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::I32 => build_stream::<i32>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::U32 => build_stream::<u32>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::F64 => build_stream::<f64>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::I64 => build_stream::<i64>(&device, &config.into(), sample_tx, channels),
        cpal::SampleFormat::U64 => build_stream::<u64>(&device, &config.into(), sample_tx, channels),
        _ => return Err(anyhow::anyhow!("Unsupported sample format: {:?}", config.sample_format())),
    }?;

    stream.play()?;

    let mut buffer = Vec::with_capacity(16000 * 600);
    let mut recording = false;

    loop {
        if let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Start => {
                    buffer.clear();
                    recording = true;
                    info!("Recording started");
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;
                    info!("Recording stopped, capturing {} samples", buffer.len());
                    
                    let mut final_samples = if sample_rate != WHISPER_SAMPLE_RATE {
                         resample_simple(&buffer, sample_rate, WHISPER_SAMPLE_RATE)
                    } else {
                        buffer.clone()
                    };
                    
                    // Optimization: Trim silence
                    trim_silence(&mut final_samples, 0.01); // Threshold 1% amplitude
                    
                    let _ = reply_tx.send(final_samples);
                }
                Cmd::Shutdown => break,
            }
        }

        match sample_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(chunk) => {
                if recording {
                    buffer.extend_from_slice(&chunk);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

fn build_stream<T>(
    device: &Device,
    config: &cpal::StreamConfig,
    tx: mpsc::Sender<Vec<f32>>,
    channels: usize,
) -> Result<cpal::Stream>
where
    T: SizedSample + Sample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &_| {
            let mut output = Vec::with_capacity(data.len() / channels);
            for frame in data.chunks(channels) {
                let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
                output.push(sum / channels as f32);
            }
            let _ = tx.send(output);
        },
        |err| error!("Stream error: {}", err),
        None,
    )?;
    Ok(stream)
}

fn get_preferred_config(device: &Device) -> Result<cpal::SupportedStreamConfig> {
    let configs = device.supported_input_configs()?;
    for config in configs {
        if config.min_sample_rate().0 <= WHISPER_SAMPLE_RATE && config.max_sample_rate().0 >= WHISPER_SAMPLE_RATE {
             return Ok(config.with_sample_rate(cpal::SampleRate(WHISPER_SAMPLE_RATE)));
        }
    }
    Ok(device.default_input_config()?)
}

fn resample_simple(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    let ratio = in_rate as f32 / out_rate as f32;
    let out_len = (input.len() as f32 / ratio) as usize;
    let mut output = Vec::with_capacity(out_len);
    
    for i in 0..out_len {
        let index = i as f32 * ratio;
        let idx_floor = index.floor() as usize;
        let idx_ceil = (idx_floor + 1).min(input.len() - 1);
        let t = index - idx_floor as f32;
        
        let sample = input[idx_floor] * (1.0 - t) + input[idx_ceil] * t;
        output.push(sample);
    }
    output
}

// Very basic silence trimming based on amplitude threshold
fn trim_silence(samples: &mut Vec<f32>, threshold: f32) {
    if samples.is_empty() { return; }

    // Find start (first sample > threshold)
    let start = samples.iter().position(|&x| x.abs() > threshold).unwrap_or(0);
    
    // Find end (last sample > threshold)
    let end = samples.iter().rposition(|&x| x.abs() > threshold).unwrap_or(samples.len() - 1);

    if start >= end {
        samples.clear();
    } else {
        // Keep a little padding (0.2s = 3200 samples)
        let padding = 3200;
        let start_pad = start.saturating_sub(padding);
        let end_pad = (end + padding).min(samples.len());
        
        *samples = samples[start_pad..end_pad].to_vec();
    }
}