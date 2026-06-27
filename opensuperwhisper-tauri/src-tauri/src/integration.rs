use arboard::Clipboard;
use enigo::{Enigo, Key, KeyboardControllable};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text.to_owned()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn simulate_paste() -> Result<(), String> {
    let mut enigo = Enigo::new();
    
    #[cfg(target_os = "macos")]
    {
        enigo.key_down(Key::Meta);
        enigo.key_click(Key::Layout('v'));
        enigo.key_up(Key::Meta);
    }

    #[cfg(not(target_os = "macos"))]
    {
        enigo.key_down(Key::Control);
        enigo.key_click(Key::Layout('v'));
        enigo.key_up(Key::Control);
    }

    Ok(())
}

pub fn play_beep_sound(is_start: bool) {
    std::thread::spawn(move || {
        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(d) => d,
            None => return,
        };

        let config = match device.default_output_config() {
            Ok(c) => c,
            Err(_) => return,
        };

        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;
        
        // Start recording has a higher pitched beep, stop has a lower pitch
        let frequency = if is_start { 880.0 } else { 440.0 }; // A5 or A4
        let duration = if is_start { 0.12 } else { 0.18 }; // duration in seconds
        let total_samples = (sample_rate * duration) as usize;
        
        let mut samples = Vec::with_capacity(total_samples * channels);
        let mut sample_clock = 0.0f32;
        
        for _ in 0..total_samples {
            sample_clock += 1.0;
            let angle = sample_clock * frequency * 2.0 * std::f32::consts::PI / sample_rate;
            let raw_sample = angle.sin();
            
            // Fade out at the end to prevent clicking sound
            let fade_factor = if total_samples - samples.len() / channels < 200 {
                ((total_samples - samples.len() / channels) as f32) / 200.0
            } else {
                1.0
            };
            
            let sample = raw_sample * 0.15 * fade_factor; // Volume 15%
            
            for _ in 0..channels {
                samples.push(sample);
            }
        }

        let samples = Arc::new(samples);
        let samples_clone = samples.clone();
        let mut sample_idx = 0;

        let err_fn = |err| eprintln!("an error occurred on output stream: {}", err);
        
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_output_stream(
                    &config.into(),
                    move |data: &mut [f32], _: &_| {
                        for sample in data.iter_mut() {
                            if sample_idx < samples_clone.len() {
                                *sample = samples_clone[sample_idx];
                                sample_idx += 1;
                            } else {
                                *sample = 0.0;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            _ => return, // Ignore other output formats for simplicity
        };

        if let Ok(s) = stream {
            if s.play().is_ok() {
                // Sleep to allow audio playback to complete before thread terminates
                std::thread::sleep(std::time::Duration::from_millis((duration * 1000.0) as u64 + 100));
            }
        }
    });
}
