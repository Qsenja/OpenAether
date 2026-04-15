pub mod logger;
pub mod settings;
pub mod ollama;
pub mod docker;
pub mod agent;
pub mod memory;
pub mod shell;
pub mod preskills;

use std::sync::{Arc, Mutex};
use crate::logic::logger::Logger;
use crate::logic::settings::{SettingsManager, UserSettings};
use crate::logic::ollama::OllamaClient;
use crate::logic::memory::MemoryManager;
use crate::logic::shell::ShellManager;
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub logger: Arc<Logger>,
    pub settings_manager: Arc<SettingsManager>,
    pub ollama: Arc<OllamaClient>,
    pub memory: Arc<MemoryManager>,
    pub shell: Arc<ShellManager>,
    pub settings: Mutex<UserSettings>,
    pub cancel_token: Mutex<Option<CancellationToken>>,
}

impl AppState {
    pub fn new() -> Self {
        let settings_manager = Arc::new(SettingsManager::new());
        let settings = settings_manager.load();
        
        let logger = Arc::new(Logger::new(crate::logic::logger::LogLevel::from_i32(settings.log_level)));
        
        let ollama = Arc::new(OllamaClient::new("http://localhost:11434".to_string()));
        let memory = Arc::new(MemoryManager::new());
        let shell = Arc::new(ShellManager::new().expect("Failed to initialize ShellManager"));

        Self {
            logger,
            settings_manager,
            ollama,
            memory,
            shell,
            settings: Mutex::new(settings),
            cancel_token: Mutex::new(None),
        }
    }
}
