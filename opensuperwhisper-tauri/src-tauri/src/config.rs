use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub selected_engine: String,          // "whisper"
    pub selected_model_path: String,      // absolute path to model
    pub selected_microphone: Option<String>,
    pub selected_language: String,        // "auto", "en", "zh", "ja", "ko", etc.
    pub cjk_spacing: bool,
    pub play_sounds: bool,
    pub auto_paste: bool,
    pub launch_at_login: bool,
    pub global_shortcut: String,         // e.g., "F9", "Control+Shift+Space"
    pub hold_to_record: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selected_engine: "whisper".to_string(),
            selected_model_path: "".to_string(),
            selected_microphone: None,
            selected_language: "auto".to_string(),
            cjk_spacing: true,
            play_sounds: true,
            auto_paste: true,
            launch_at_login: false,
            global_shortcut: "F9".to_string(),
            hold_to_record: false,
        }
    }
}

pub struct ConfigManager {
    config_dir: PathBuf,
}

impl ConfigManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            config_dir: app_data_dir,
        }
    }

    fn config_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn load(&self) -> Settings {
        let path = self.config_path();
        if !path.exists() {
            return Settings::default();
        }

        let mut file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return Settings::default(),
        };

        let mut contents = String::new();
        if file.read_to_string(&mut contents).is_err() {
            return Settings::default();
        }

        serde_json::from_str(&contents).unwrap_or_else(|_| Settings::default())
    }

    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        if !self.config_dir.exists() {
            create_dir_all(&self.config_dir).map_err(|e| e.to_string())?;
        }

        let path = self.config_path();
        let contents = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        file.write_all(contents.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }
}
