use crate::models::{AppSettings, Conversation};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppData {
    pub settings: AppSettings,
    pub conversations: Vec<Conversation>,
    pub current_id: Option<uuid::Uuid>,
    /// 左侧会话栏是否展开
    #[serde(default = "default_true")]
    pub sidebar_open: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            settings: AppSettings::default(),
            conversations: Vec::new(),
            current_id: None,
            sidebar_open: true,
        }
    }
}

pub fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let new_dir = base.join("askway");
    // 从旧版 ai_client 目录迁移一次，避免改名后丢失数据
    let old_dir = base.join("ai_client");
    if !new_dir.exists() && old_dir.exists() {
        if fs::rename(&old_dir, &new_dir).is_err() {
            let _ = copy_dir_recursive(&old_dir, &new_dir);
        }
    }
    new_dir
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn data_file() -> PathBuf {
    data_dir().join("data.json")
}

pub fn load() -> AppData {
    let path = data_file();
    let mut data = if !path.exists() {
        AppData::default()
    } else {
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => AppData::default(),
        }
    };
    data.settings.ensure_all_providers();
    data
}

pub fn save(data: &AppData) -> Result<(), String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string(data).map_err(|e| e.to_string())?;
    fs::write(data_file(), text).map_err(|e| e.to_string())
}
