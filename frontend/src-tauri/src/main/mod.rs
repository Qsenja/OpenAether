pub mod logger;
pub mod settings;
pub mod ollama;
pub mod docker;
pub mod bridge;
pub mod agent;
pub mod memory;

use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use crate::logic::logger::Logger;
use crate::logic::settings::{SettingsManager, UserSettings};
use crate::logic::ollama::OllamaClient;
use crate::logic::bridge::PythonBridge;
use crate::logic::memory::MemoryManager;

pub struct AppState {
    pub logger: Arc<Logger>,
    pub settings_manager: Arc<SettingsManager>,
    pub ollama: Arc<OllamaClient>,
    pub bridge: Arc<PythonBridge>,
    pub memory: Arc<MemoryManager>,
    pub settings: Mutex<UserSettings>,
}

impl AppState {
    pub fn new(python_path: PathBuf, worker_script: PathBuf) -> Self {
        let settings_manager = Arc::new(SettingsManager::new());
        let settings = settings_manager.load();
        
        let logger = Arc::new(Logger::new(crate::logic::logger::LogLevel::from_i32(settings.log_level)));
        
        let ollama = Arc::new(OllamaClient::new("http://localhost:11434".to_string()));
        let bridge = Arc::new(PythonBridge::new(python_path, worker_script));
        let memory = Arc::new(MemoryManager::new());

        Self {
            logger,
            settings_manager,
            ollama,
            bridge,
            memory,
            settings: Mutex::new(settings),
        }
    }
}
