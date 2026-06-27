mod audio;
mod config;
mod db;
mod integration;
mod transcribe;

use audio::AudioRecorder;
use config::{ConfigManager, Settings};
use db::{DbManager, HistoryItem};
use integration::{copy_to_clipboard, play_beep_sound, simulate_paste};
use transcribe::Transcriber;

use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tauri::{Emitter, Manager};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct AppState {
    pub recorder: Arc<Mutex<AudioRecorder>>,
    pub transcriber: Arc<Mutex<Transcriber>>,
    pub db: DbManager,
    pub config: ConfigManager,
    pub active_downloads: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

#[derive(Clone, serde::Serialize)]
struct DownloadProgressPayload {
    model_name: String,
    progress: f64,
    status: String,
    error: Option<String>,
}

#[tauri::command]
fn get_microphones() -> Result<Vec<String>, String> {
    AudioRecorder::list_microphones()
}

#[tauri::command]
fn start_recording(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let settings = state.config.load();
    if settings.play_sounds {
        play_beep_sound(true);
    }
    
    let mut recorder = state.recorder.lock().unwrap();
    recorder.start(settings.selected_microphone.clone())?;
    Ok(())
}

fn resolve_model_path(app_handle: &tauri::AppHandle, path_str: &str) -> String {
    if path_str.is_empty() {
        return "".to_string();
    }
    let path = std::path::Path::new(path_str);
    if path.is_absolute() {
        path_str.to_string()
    } else {
        if let Ok(app_data_dir) = app_handle.path().app_data_dir() {
            let resolved = app_data_dir.join("models").join(path_str);
            resolved.to_string_lossy().to_string()
        } else {
            path_str.to_string()
        }
    }
}

#[tauri::command]
async fn stop_recording(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<Option<HistoryItem>, String> {
    let settings = state.config.load();
    if settings.play_sounds {
        play_beep_sound(false);
    }
    
    let recording_opt = {
        let mut recorder = state.recorder.lock().unwrap();
        recorder.stop()?
    };
    
    let recording = match recording_opt {
        Some(r) => r,
        None => return Ok(None),
    };
    
    let audio_path = recording.file_path.clone();
    let model_path = resolve_model_path(&app_handle, &settings.selected_model_path);
    let language = settings.selected_language.clone();
    let cjk_spacing = settings.cjk_spacing;
    let transcriber_clone = state.transcriber.clone();
    
    if model_path.is_empty() {
        return Err("No Whisper model is selected. Please select a model in settings first.".to_string());
    }

    let text = tokio::task::spawn_blocking(move || {
        let mut transcriber = transcriber_clone.lock().unwrap();
        transcriber.transcribe(&audio_path, &model_path, &language, cjk_spacing)
    })
    .await
    .map_err(|e| e.to_string())??;
    
    let _ = copy_to_clipboard(&text);
    
    if settings.auto_paste {
        // Sleep slightly to let clipboard register and active window regain focus
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        let _ = simulate_paste();
    }
    
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = state.db.insert_recording(&timestamp, recording.duration_seconds, &text, Some(&recording.file_path))?;
    
    Ok(Some(HistoryItem {
        id,
        timestamp,
        duration: recording.duration_seconds,
        text,
        audio_path: Some(recording.file_path),
    }))
}

#[tauri::command]
fn cancel_recording(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut recorder = state.recorder.lock().unwrap();
    recorder.cancel();
    Ok(())
}

#[tauri::command]
fn get_history(state: tauri::State<'_, AppState>) -> Result<Vec<HistoryItem>, String> {
    state.db.get_history()
}

#[tauri::command]
fn delete_history_item(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.delete_recording(id)
}

#[tauri::command]
fn clear_history(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.db.clear_history()
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.config.load())
}

#[tauri::command]
fn save_settings(state: tauri::State<'_, AppState>, settings: Settings) -> Result<(), String> {
    state.config.save(&settings)
}

#[tauri::command]
fn get_models(app_handle: tauri::AppHandle) -> Result<Vec<String>, String> {
    let app_data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let models_dir = app_data_dir.join("models");
    if !models_dir.exists() {
        let _ = std::fs::create_dir_all(&models_dir);
    }
    
    let entries = std::fs::read_dir(models_dir).map_err(|e| e.to_string())?;
    let mut models = Vec::new();
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().map(|ext| ext == "bin").unwrap_or(false) {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    models.push(name.to_string());
                }
            }
        }
    }
    models.sort();
    Ok(models)
}

#[tauri::command]
async fn transcribe_file(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    file_path: String,
) -> Result<HistoryItem, String> {
    let settings = state.config.load();
    let model_path = resolve_model_path(&app_handle, &settings.selected_model_path);
    let language = settings.selected_language.clone();
    let cjk_spacing = settings.cjk_spacing;
    let transcriber_clone = state.transcriber.clone();
    
    if model_path.is_empty() {
        return Err("No Whisper model is selected. Please select a model in settings first.".to_string());
    }

    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err("File does not exist".to_string());
    }
    
    let file_path_clone = file_path.clone();
    let text = tokio::task::spawn_blocking(move || {
        let mut transcriber = transcriber_clone.lock().unwrap();
        transcriber.transcribe(&file_path_clone, &model_path, &language, cjk_spacing)
    })
    .await
    .map_err(|e| e.to_string())??;
    
    let _ = copy_to_clipboard(&text);
    
    // Auto paste if configured
    if settings.auto_paste {
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        let _ = simulate_paste();
    }
    
    // Check duration of file using hound
    let duration = match hound::WavReader::open(&file_path) {
        Ok(reader) => {
            let spec = reader.spec();
            let total_samples = reader.duration();
            total_samples as f64 / spec.sample_rate as f64
        }
        Err(_) => 0.0,
    };

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = state.db.insert_recording(&timestamp, duration, &text, Some(&file_path))?;
    
    Ok(HistoryItem {
        id,
        timestamp,
        duration,
        text,
        audio_path: Some(file_path),
    })
}

#[tauri::command]
async fn download_model(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    model_name: String,
) -> Result<(), String> {
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        model_name
    );
    
    let app_data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let models_dir = app_data_dir.join("models");
    if !models_dir.exists() {
        let _ = std::fs::create_dir_all(&models_dir);
    }
    let dest_path = models_dir.join(&model_name);
    
    if dest_path.exists() {
        return Ok(());
    }
    
    let (tx, rx) = oneshot::channel();
    {
        let mut downloads = state.active_downloads.lock().unwrap();
        downloads.insert(model_name.clone(), tx);
    }
    
    let app_handle_clone = app_handle.clone();
    let model_name_clone = model_name.clone();
    
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let res = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                let _ = app_handle_clone.emit(
                    "download-progress",
                    DownloadProgressPayload {
                        model_name: model_name_clone,
                        progress: 0.0,
                        status: "error".to_string(),
                        error: Some(e.to_string()),
                    },
                );
                return;
            }
        };
        
        let total_size = res.content_length().unwrap_or(0);
        let mut file = match std::fs::File::create(&dest_path) {
            Ok(f) => f,
            Err(e) => {
                let _ = app_handle_clone.emit(
                    "download-progress",
                    DownloadProgressPayload {
                        model_name: model_name_clone,
                        progress: 0.0,
                        status: "error".to_string(),
                        error: Some(e.to_string()),
                    },
                );
                return;
            }
        };
        
        let mut downloaded: u64 = 0;
        let mut stream = res.bytes_stream();
        let mut rx = rx;
        
        loop {
            tokio::select! {
                _ = &mut rx => {
                    drop(file);
                    let _ = std::fs::remove_file(&dest_path);
                    let _ = app_handle_clone.emit(
                        "download-progress",
                        DownloadProgressPayload {
                            model_name: model_name_clone,
                            progress: 0.0,
                            status: "cancelled".to_string(),
                            error: None,
                        },
                    );
                    return;
                }
                chunk_opt = stream.next() => {
                    match chunk_opt {
                        Some(Ok(chunk)) => {
                            use std::io::Write;
                            if let Err(e) = file.write_all(&chunk) {
                                let _ = app_handle_clone.emit(
                                    "download-progress",
                                    DownloadProgressPayload {
                                        model_name: model_name_clone,
                                        progress: 0.0,
                                        status: "error".to_string(),
                                        error: Some(e.to_string()),
                                    },
                                );
                                return;
                            }
                            downloaded += chunk.len() as u64;
                            let progress = if total_size > 0 {
                                (downloaded as f64 / total_size as f64) * 100.0
                            } else {
                                0.0
                            };
                            
                            let _ = app_handle_clone.emit(
                                "download-progress",
                                DownloadProgressPayload {
                                    model_name: model_name_clone.clone(),
                                    progress,
                                    status: "downloading".to_string(),
                                    error: None,
                                },
                            );
                        }
                        Some(Err(e)) => {
                            drop(file);
                            let _ = std::fs::remove_file(&dest_path);
                            let _ = app_handle_clone.emit(
                                "download-progress",
                                DownloadProgressPayload {
                                    model_name: model_name_clone,
                                    progress: 0.0,
                                    status: "error".to_string(),
                                    error: Some(e.to_string()),
                                },
                            );
                            return;
                        }
                        None => {
                            let _ = app_handle_clone.emit(
                                "download-progress",
                                DownloadProgressPayload {
                                    model_name: model_name_clone,
                                    progress: 100.0,
                                    status: "completed".to_string(),
                                    error: None,
                                },
                            );
                            return;
                        }
                    }
                }
            }
        }
    });
    
    Ok(())
}

#[tauri::command]
fn cancel_model_download(state: tauri::State<'_, AppState>, model_name: String) -> Result<(), String> {
    let mut downloads = state.active_downloads.lock().unwrap();
    if let Some(tx) = downloads.remove(&model_name) {
        let _ = tx.send(());
    }
    Ok(())
}

#[tauri::command]
fn play_audio_file(file_path: String) -> Result<(), String> {
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

        let mut reader = match hound::WavReader::open(&file_path) {
            Ok(r) => r,
            Err(_) => return,
        };

        let spec = reader.spec();
        let file_sample_rate = spec.sample_rate;

        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                if bits == 16 {
                    reader
                        .samples::<i16>()
                        .map(|s| s.map(|v| v as f32 / 32768.0).unwrap_or(0.0))
                        .collect()
                } else {
                    return;
                }
            }
            hound::SampleFormat::Float => {
                reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect()
            }
        };

        let device_channels = config.channels() as usize;
        let device_sample_rate = config.sample_rate().0;

        let resampled = resample_audio(&samples, file_sample_rate, device_sample_rate);
        
        let mut final_samples = Vec::new();
        for sample in resampled {
            for _ in 0..device_channels {
                final_samples.push(sample);
            }
        }

        let samples_arc = Arc::new(final_samples);
        let samples_clone = samples_arc.clone();
        let mut sample_idx = 0;

        let err_fn = |err| eprintln!("audio playback output stream error: {}", err);
        
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
            _ => return,
        };

        if let Ok(s) = stream {
            if s.play().is_ok() {
                let duration_ms = (samples_arc.len() as f64 / (device_sample_rate as f64 * device_channels as f64) * 1000.0) as u64;
                std::thread::sleep(std::time::Duration::from_millis(duration_ms + 100));
            }
        }
    });

    Ok(())
}

fn resample_audio(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            if !app_data_dir.exists() {
                std::fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
            }
            
            let db = DbManager::new(app_data_dir.clone());
            let config = ConfigManager::new(app_data_dir.clone());
            let recordings_dir = app_data_dir.join("recordings");
            if !recordings_dir.exists() {
                std::fs::create_dir_all(&recordings_dir).map_err(|e| e.to_string())?;
            }
            
            let recorder = Arc::new(Mutex::new(AudioRecorder::new(recordings_dir)));
            let transcriber = Arc::new(Mutex::new(Transcriber::new()));
            let active_downloads = Arc::new(Mutex::new(HashMap::new()));
            
            app.manage(AppState {
                recorder,
                transcriber,
                db,
                config,
                active_downloads,
            });
            
            let show_item = MenuItem::with_id(app, "show", "Show OpenSuperWhisper", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            
            let tray_icon = app.default_window_icon().cloned();
            if let Some(icon) = tray_icon {
                let _tray = TrayIconBuilder::new()
                    .icon(icon)
                    .menu(&menu)
                    .on_menu_event(|app, event| {
                        match event.id.as_ref() {
                            "show" => {
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                            "quit" => {
                                app.exit(0);
                            }
                            _ => {}
                        }
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click { .. } = event {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    })
                    .build(app)?;
            }
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_microphones,
            start_recording,
            stop_recording,
            cancel_recording,
            get_history,
            delete_history_item,
            clear_history,
            get_settings,
            save_settings,
            get_models,
            transcribe_file,
            download_model,
            cancel_model_download,
            play_audio_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
