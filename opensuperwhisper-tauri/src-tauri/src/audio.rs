use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Recording {
    pub file_path: String,
    pub duration_seconds: f64,
}

#[allow(dead_code)]
pub struct SendSyncStream(pub cpal::Stream);
unsafe impl Send for SendSyncStream {}
unsafe impl Sync for SendSyncStream {}

pub struct AudioRecorder {
    stream: Option<SendSyncStream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
    start_time: Option<Instant>,
    output_dir: std::path::PathBuf,
}

impl AudioRecorder {
    pub fn new(output_dir: std::path::PathBuf) -> Self {
        Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 44100,
            channels: 1,
            start_time: None,
            output_dir,
        }
    }

    pub fn list_microphones() -> Result<Vec<String>, String> {
        let host = cpal::default_host();
        let devices = host.input_devices().map_err(|e| e.to_string())?;
        let names = devices
            .filter_map(|d| d.name().ok())
            .collect::<Vec<String>>();
        Ok(names)
    }

    pub fn start(&mut self, device_name: Option<String>) -> Result<(), String> {
        if self.stream.is_some() {
            return Err("Recording is already in progress".to_string());
        }

        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            host.input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or_else(|| format!("Microphone '{}' not found", name))?
        } else {
            host.default_input_device()
                .ok_or_else(|| "No default microphone found".to_string())?
        };

        let config = device
            .default_input_config()
            .map_err(|e| e.to_string())?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        self.sample_rate = sample_rate;
        self.channels = channels;
        
        let buffer = Arc::new(Mutex::new(Vec::new()));
        self.buffer = buffer.clone();

        let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        if let Ok(mut buf) = buffer.lock() {
                            buf.extend_from_slice(data);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &_| {
                        if let Ok(mut buf) = buffer.lock() {
                            let f32_data: Vec<f32> = data
                                .iter()
                                .map(|&sample| sample as f32 / 32768.0)
                                .collect();
                            buf.extend_from_slice(&f32_data);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &_| {
                        if let Ok(mut buf) = buffer.lock() {
                            let f32_data: Vec<f32> = data
                                .iter()
                                .map(|&sample| {
                                    (sample as f32 - 32768.0) / 32768.0
                                })
                                .collect();
                            buf.extend_from_slice(&f32_data);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            _ => return Err("Unsupported audio format".to_string()),
        }
        .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(SendSyncStream(stream));
        self.start_time = Some(Instant::now());

        Ok(())
    }

    pub fn stop(&mut self) -> Result<Option<Recording>, String> {
        let stream = self.stream.take();
        if stream.is_none() {
            return Ok(None);
        }

        // Dropping the stream stops recording
        drop(stream);

        let start_time = self.start_time.take().ok_or("Invalid state")?;
        let duration_seconds = start_time.elapsed().as_secs_f64();

        // Get the recorded audio samples
        let raw_samples = {
            let mut buf = self.buffer.lock().map_err(|e| e.to_string())?;
            std::mem::take(&mut *buf)
        };

        if raw_samples.is_empty() {
            return Ok(None);
        }

        // Convert multi-channel input to mono
        let mut mono_samples = Vec::new();
        if self.channels > 1 {
            let channels = self.channels as usize;
            for chunk in raw_samples.chunks(channels) {
                if chunk.len() == channels {
                    let sum: f32 = chunk.iter().sum();
                    mono_samples.push(sum / channels as f32);
                }
            }
        } else {
            mono_samples = raw_samples;
        }

        // Resample mono samples to 16000 Hz for Whisper
        let target_sample_rate = 16000;
        let resampled_samples = resample(&mono_samples, self.sample_rate, target_sample_rate);

        // Save samples to WAV file
        if !self.output_dir.exists() {
            let _ = std::fs::create_dir_all(&self.output_dir);
        }
        
        let timestamp = chrono::Utc::now().timestamp();
        let filename = format!("{}.wav", timestamp);
        let file_path = self.output_dir.join(filename);

        write_wav_file(&file_path, &resampled_samples, target_sample_rate)?;

        Ok(Some(Recording {
            file_path: file_path.to_string_lossy().to_string(),
            duration_seconds,
        }))
    }

    pub fn cancel(&mut self) {
        self.stream = None;
        self.start_time = None;
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }
    }

    pub fn is_recording(&self) -> bool {
        self.stream.is_some()
    }
}

// Simple linear interpolation resampler
fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }
    let factor = from_rate as f64 / to_rate as f64;
    let out_len = (input.len() as f64 / factor).round() as usize;
    let mut output = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * factor;
        let idx = pos.floor() as usize;
        let frac = pos - idx as f64;
        if idx + 1 < input.len() {
            let s0 = input[idx];
            let s1 = input[idx + 1];
            let sample = s0 + (s1 - s0) * frac as f32;
            output.push(sample);
        } else if idx < input.len() {
            output.push(input[idx]);
        }
    }
    output
}

fn write_wav_file(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &sample in samples {
        // Convert f32 sample [-1.0, 1.0] to i16 [-32768, 32767]
        let scaled = (sample * 32768.0).clamp(-32768.0, 32767.0) as i16;
        writer.write_sample(scaled).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    Ok(())
}
