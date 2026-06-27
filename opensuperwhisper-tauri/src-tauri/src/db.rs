use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs::create_dir_all;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryItem {
    pub id: i64,
    pub timestamp: String,
    pub duration: f64,
    pub text: String,
    pub audio_path: Option<String>,
}

pub struct DbManager {
    db_path: PathBuf,
}

impl DbManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        if !app_data_dir.exists() {
            let _ = create_dir_all(&app_data_dir);
        }
        let db_path = app_data_dir.join("history.db");
        let s = Self { db_path };
        let _ = s.init_db();
        s
    }

    fn conn(&self) -> Result<Connection, String> {
        Connection::open(&self.db_path).map_err(|e| e.to_string())
    }

    fn init_db(&self) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                duration REAL NOT NULL,
                text TEXT NOT NULL,
                audio_path TEXT
            )",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_recording(&self, timestamp: &str, duration: f64, text: &str, audio_path: Option<&str>) -> Result<i64, String> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO history (timestamp, duration, text, audio_path) VALUES (?, ?, ?, ?)",
            params![timestamp, duration, text, audio_path],
        )
        .map_err(|e| e.to_string())?;
        
        let id = conn.last_insert_rowid();
        Ok(id)
    }

    pub fn get_history(&self) -> Result<Vec<HistoryItem>, String> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT id, timestamp, duration, text, audio_path FROM history ORDER BY id DESC")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(HistoryItem {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    duration: row.get(2)?,
                    text: row.get(3)?,
                    audio_path: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                items.push(item);
            }
        }
        Ok(items)
    }

    pub fn delete_recording(&self, id: i64) -> Result<(), String> {
        let conn = self.conn()?;
        
        // Optionally find the audio file path and delete the actual file
        let mut stmt = conn.prepare("SELECT audio_path FROM history WHERE id = ?").map_err(|e| e.to_string())?;
        let mut rows = stmt.query([id]).map_err(|e| e.to_string())?;
        if let Ok(Some(row)) = rows.next() {
            let audio_path: Option<String> = row.get(0).ok();
            if let Some(path_str) = audio_path {
                let p = std::path::Path::new(&path_str);
                if p.exists() {
                    let _ = std::fs::remove_file(p);
                }
            }
        }

        conn.execute("DELETE FROM history WHERE id = ?", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<(), String> {
        let conn = self.conn()?;
        
        // Find all audio files and delete them
        let mut stmt = conn.prepare("SELECT audio_path FROM history").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0)).map_err(|e| e.to_string())?;
        for r in rows {
            if let Ok(Some(path_str)) = r {
                let p = std::path::Path::new(&path_str);
                if p.exists() {
                    let _ = std::fs::remove_file(p);
                }
            }
        }

        conn.execute("DELETE FROM history", []).map_err(|e| e.to_string())?;
        Ok(())
    }
}
