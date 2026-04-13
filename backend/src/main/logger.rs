use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use chrono::Local;
use directories::ProjectDirs;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl LogLevel {
    pub fn from_i32(val: i32) -> Self {
        match val {
            1 => LogLevel::Error,
            2 => LogLevel::Info,
            3 => LogLevel::Debug,
            4 => LogLevel::Trace,
            _ => LogLevel::Info,
        }
    }
}

pub struct Logger {
    log_file: PathBuf,
    level: RwLock<LogLevel>,
}

impl Logger {
    pub fn new(level: LogLevel) -> Self {
        let proj_dirs = ProjectDirs::from("com", "openaether", "openaether")
            .expect("Could not determine config directory");
        let log_dir = proj_dirs.data_dir().join("logs");
        fs::create_dir_all(&log_dir).expect("Could not create log directory");

        let now = Local::now();
        let log_filename = format!("session_{}.log", now.format("%Y%m%d_%H%M%S"));
        let log_file = log_dir.join(log_filename);

        println!("[LOGGER] Initialized at level {:?}", level);

        Self { 
            log_file, 
            level: RwLock::new(level) 
        }
    }

    pub fn set_level(&self, level: LogLevel) {
        if let Ok(mut l) = self.level.write() {
            *l = level;
        }
        self.log("SYSTEM", &format!("Log level changed to {:?}", level));
    }

    pub fn log(&self, tag: &str, message: &str) {
        self.log_at(LogLevel::Info, tag, message);
    }

    pub fn log_at(&self, level: LogLevel, tag: &str, message: &str) {
        {
            // Handle poisoned lock gracefully by defaulting to Trace if it happens
            let effective_level = match self.level.read() {
                Ok(l) => *l,
                Err(_) => LogLevel::Trace,
            };
            
            if level > effective_level {
                return;
            }
        }

        let now = Local::now();
        let log_entry = format!("[{}][{}] {}\n", now.format("%H:%M:%S"), tag, message);
        
        // Console output
        println!("{}", log_entry.trim());

        // File output
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file) 
        {
            let _ = file.write_all(log_entry.as_bytes());
        }
    }

    pub fn get_log_dir(&self) -> PathBuf {
        self.log_file.parent().unwrap().to_path_buf()
    }

    pub fn get_current_log_path(&self) -> PathBuf {
        self.log_file.clone()
    }
}
