use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
use std::path::Path;

pub struct Transcriber {
    // Cache the last loaded model path and context to avoid reloading every time
    cached_model_path: Option<String>,
    cached_context: Option<WhisperContext>,
}

impl Transcriber {
    pub fn new() -> Self {
        Self {
            cached_model_path: None,
            cached_context: None,
        }
    }

    fn get_context(&mut self, model_path: &str) -> Result<&WhisperContext, String> {
        if self.cached_model_path.as_deref() == Some(model_path) && self.cached_context.is_some() {
            return Ok(self.cached_context.as_ref().unwrap());
        }

        let path = Path::new(model_path);
        if !path.exists() {
            return Err(format!("Model file not found at: {}", model_path));
        }

        let ctx = WhisperContext::new_with_params(
            model_path,
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("Failed to load Whisper model: {}", e))?;

        self.cached_model_path = Some(model_path.to_string());
        self.cached_context = Some(ctx);

        Ok(self.cached_context.as_ref().unwrap())
    }

    pub fn transcribe(
        &mut self,
        audio_path: &str,
        model_path: &str,
        language: &str,
        apply_autocorrect: bool,
    ) -> Result<String, String> {
        // 1. Read WAV file samples
        let mut reader = hound::WavReader::open(audio_path)
            .map_err(|e| format!("Failed to open audio file: {}", e))?;
        
        let spec = reader.spec();
        let channels = spec.channels;
        let sample_rate = spec.sample_rate;

        let raw_samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                match bits {
                    16 => {
                        reader
                            .samples::<i16>()
                            .map(|s| s.map(|v| v as f32 / 32768.0).unwrap_or(0.0))
                            .collect()
                    }
                    8 => {
                        reader
                            .samples::<i8>()
                            .map(|s| s.map(|v| v as f32 / 128.0).unwrap_or(0.0))
                            .collect()
                    }
                    32 => {
                        reader
                            .samples::<i32>()
                            .map(|s| s.map(|v| v as f32 / 2147483648.0).unwrap_or(0.0))
                            .collect()
                    }
                    _ => return Err(format!("Unsupported bits per sample: {}", bits)),
                }
            }
            hound::SampleFormat::Float => {
                reader
                    .samples::<f32>()
                    .map(|s| s.unwrap_or(0.0))
                    .collect()
            }
        };

        if raw_samples.is_empty() {
            return Err("Audio file is empty".to_string());
        }

        // Convert multi-channel input to mono
        let mut mono_samples = Vec::new();
        if channels > 1 {
            let channels_count = channels as usize;
            for chunk in raw_samples.chunks(channels_count) {
                if chunk.len() == channels_count {
                    let sum: f32 = chunk.iter().sum();
                    mono_samples.push(sum / channels_count as f32);
                }
            }
        } else {
            mono_samples = raw_samples;
        }

        // Resample mono samples to 16000 Hz
        let samples = resample(&mono_samples, sample_rate, 16000);

        if samples.is_empty() {
            return Err("Audio file is empty".to_string());
        }

        // 2. Get Whisper Context
        let ctx = self.get_context(model_path)?;

        // 3. Create Whisper State
        let mut state = ctx
            .create_state()
            .map_err(|e| format!("Failed to create Whisper state: {}", e))?;

        // 4. Set parameters
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        
        // Use 4 threads by default for transcription speed
        params.set_n_threads(4);
        
        // Disable translating to English (we want transcribing)
        params.set_translate(false);

        // Language setting
        if language != "auto" && !language.is_empty() {
            params.set_language(Some(language));
        } else {
            params.set_language(Some("auto"));
        }

        // 5. Run the model
        state
            .full(params, &samples[..])
            .map_err(|e| format!("Whisper model execution failed: {}", e))?;

        // 6. Extract text
        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(&segment.to_string());
        }

        let mut cleaned_text = text.trim().to_string();

        // 7. Apply Asian Autocorrect if enabled
        if apply_autocorrect {
            cleaned_text = autocorrect::format(&cleaned_text);
        }

        Ok(cleaned_text)
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
