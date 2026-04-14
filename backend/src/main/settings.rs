use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserSettings {
    pub pastebin_api_key: String,
    pub ollama_model: String,
    pub searxng_url: String,
    pub log_level: i32,
    pub temperature: f64,
    pub top_p: f64,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            pastebin_api_key: String::new(),
            ollama_model: "qwen2.5:14b".to_string(),
            searxng_url: "http://localhost:8888".to_string(),
            log_level: 3,
            temperature: 0.4,
            top_p: 0.9,
        }
    }
}

pub struct SettingsManager {
    path: PathBuf,
}

impl SettingsManager {
    pub fn new() -> Self {
        let proj_dirs = ProjectDirs::from("com", "openaether", "openaether")
            .expect("Could not determine config directory");
        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir).expect("Could not create config directory");
        
        let path = config_dir.join("user_settings.json");
        Self { path }
    }

    pub fn load(&self) -> UserSettings {
        if let Ok(content) = fs::read_to_string(&self.path) {
            if let Ok(settings) = serde_json::from_str(&content) {
                return settings;
            }
        }
        UserSettings::default()
    }

    pub fn save(&self, settings: &UserSettings) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(settings)?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}
