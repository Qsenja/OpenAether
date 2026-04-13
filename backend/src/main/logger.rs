use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use chrono::Local;
use directories::ProjectDirs;
use std::sync::{RwLock, Mutex};
use std::time::Instant;
use sysinfo::System;
use serde_json::Value;

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
    log_dir: PathBuf,
    level: RwLock<LogLevel>,
    start_time: Instant,
    event_count: Mutex<usize>,
    last_type: RwLock<Option<String>>,
}

impl Logger {
    pub fn new(level: LogLevel) -> Self {
        let proj_dirs = ProjectDirs::from("com", "openaether", "openaether")
            .expect("Could not determine config directory");
        let log_dir = proj_dirs.data_dir().join("logs");
        fs::create_dir_all(&log_dir).expect("Could not create log directory");

        let now = Local::now();
        let log_filename = format!("{}.log", now.format("%Y-%m-%d_%H-%M-%S"));
        let log_file = log_dir.join(log_filename);

        let logger = Self { 
            log_file, 
            log_dir,
            level: RwLock::new(level),
            start_time: Instant::now(),
            event_count: Mutex::new(0),
            last_type: RwLock::new(None),
        };

        logger.write_header();
        logger.auto_purge();
        
        println!("[LOGGER] Initialized at level {:?}", level);
        
        logger
    }

    fn write_header(&self) {
        let mut sys = System::new_all();
        sys.refresh_all();
        
        let header = format!(
            "============================================================\n\
             OPENAETHER DIAGNOSTIC LOG - {}\n\
             ============================================================\n\
             OS: {} {} ({})\n\
             Backend Version: {}\n\
             Executable: {:?}\n\
             Working Dir: {:?}\n\
             Log File: {:?}\n\
             ============================================================\n\n",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            System::name().unwrap_or_else(|| "Unknown".to_string()),
            System::os_version().unwrap_or_else(|| "Unknown".to_string()),
            System::cpu_arch().unwrap_or_else(|| "Unknown".to_string()),
            env!("CARGO_PKG_VERSION"),
            std::env::current_exe().unwrap_or_default(),
            std::env::current_dir().unwrap_or_default(),
            self.log_file
        );

        if let Ok(mut file) = OpenOptions::new().create(true).write(true).open(&self.log_file) {
            let _ = file.write_all(header.as_bytes());
        }
    }

    fn auto_purge(&self) {
        let max_logs = 10;
        match fs::read_dir(&self.log_dir) {
            Ok(entries) => {
                let mut logs: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().map_or(false, |ext| ext == "log"))
                    .collect();
                
                // Sort by modification time, newest first
                logs.sort_by(|a, b| {
                    let a_time = fs::metadata(a).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    let b_time = fs::metadata(b).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    b_time.cmp(&a_time)
                });

                // Exclude currently active log
                let other_logs: Vec<_> = logs.into_iter()
                    .filter(|p| p != &self.log_file)
                    .collect();

                if other_logs.len() > max_logs {
                    for old_log in other_logs.iter().skip(max_logs) {
                        let _ = fs::remove_file(old_log);
                    }
                }
            }
            Err(e) => self.tlog(&format!("Logger purge failed: {}", e)),
        }
    }

    fn check_size_limit(&self) {
        let max_mb = 5;
        if let Ok(metadata) = fs::metadata(&self.log_file) {
            if metadata.len() > max_mb * 1024 * 1024 {
                self.write_raw("\n\n[SYSTEM] Log size limit (5MB) reached. Truncating further output for safety.\n");
            }
        }
    }

    pub fn tlog(&self, msg: &str) {
        let elapsed = self.start_time.elapsed().as_secs_f32();
        println!("[TRACER][{:.3}s] {}", elapsed, msg);
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
        print!("{}", log_entry);

        // File output
        self.write_raw(&log_entry);
    }

    fn write_raw(&self, content: &str) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file) 
        {
            let _ = file.write_all(content.as_bytes());
        }
    }

    pub fn log_event(&self, event_type: &str, data: Value) {
        {
            let mut count = self.event_count.lock().unwrap();
            *count += 1;
            if *count % 20 == 0 {
                self.auto_purge();
                self.check_size_limit();
            }
        }

        self.write_readable(event_type, data);
    }

    pub fn log_message(&self, role: &str, content: &str) {
        self.log_event("message", serde_json::json!({
            "role": role,
            "content": content
        }));
    }

    pub fn log_tool(&self, name: &str, args: Value, output: &str) {
        self.log_event("tool", serde_json::json!({
            "name": name,
            "args": args,
            "output": output
        }));
    }

    pub fn log_error_report(&self, module: &str, issue: &str, details: &str) {
        self.log_event("error_report", serde_json::json!({
            "module": module,
            "issue": issue,
            "details": details
        }));
    }

    fn write_readable(&self, event_type: &str, data: Value) {
        let ts = Local::now().format("%H:%M:%S").to_string();
        let mut output = String::new();

        match event_type {
            "message" => {
                let role = data["role"].as_str().unwrap_or("unknown").to_uppercase();
                let content = data["content"].as_str();
                let fcall = data["function_call"].as_object();

                if let Some(c) = content {
                    output.push_str(&format!("\n[{}] {}:\n{}\n", ts, role, c));
                }

                if let Some(fc) = fcall {
                    let name = fc.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let args = fc.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                    output.push_str(&format!("\n[{}] {} INITIATED ACTION: {}\n", ts, role, name));
                    output.push_str(&format!("ARGS: {}\n", args));
                }

                output.push_str(&format!("{}\n", "-".repeat(40)));
                if let Ok(mut last) = self.last_type.write() {
                    *last = None;
                }
            }
            "tool" => {
                let name = data["name"].as_str().unwrap_or("unknown");
                let args = &data["args"];
                let tool_output = data["output"].as_str().unwrap_or("");

                output.push_str(&format!("\n[{}] TOOL EXECUTION: {}\n", ts, name));
                if !args.is_null() {
                    output.push_str(&format!("PARAMETERS: {}\n", serde_json::to_string_pretty(args).unwrap_or_default()));
                }

                if tool_output.len() > 50000 {
                    output.push_str(&format!("RESULT (TRUNCATED): {}... [REST OMITTED]\n", &tool_output[..50000]));
                } else {
                    output.push_str(&format!("RESULT:\n{}\n", tool_output));
                }

                output.push_str(&format!("{}\n", "=".repeat(40)));
                if let Ok(mut last) = self.last_type.write() {
                    *last = None;
                }
            }
            "server_to_client" if data["type"] == "agent_thought" => {
                let last = self.last_type.read().unwrap().clone();
                if last.as_deref() != Some("thought") {
                    output.push_str(&format!("\n[{}] AI REASONING:\n", ts));
                    if let Ok(mut last_w) = self.last_type.write() {
                        *last_w = Some("thought".to_string());
                    }
                }
                output.push_str(data["content"].as_str().unwrap_or(""));
            }
            "server_to_client" if data["type"] == "agent_message_done" => {
                output.push_str(&format!("\n{}\n", ".".repeat(40)));
                if let Ok(mut last) = self.last_type.write() {
                    *last = None;
                }
            }
            "error_report" => {
                output.push_str(&format!("\n[{}] CRITICAL ERROR in {}:\n", ts, data["module"].as_str().unwrap_or("unknown")));
                output.push_str(&format!("ISSUE: {}\n", data["issue"].as_str().unwrap_or("unknown")));
                output.push_str(&format!("DETAILS: {}\n", data["details"].as_str().unwrap_or("none")));
                output.push_str(&format!("{}\n", "!".repeat(60)));
            }
            _ => {}
        }

        if !output.is_empty() {
            self.write_raw(&output);
        }
    }

    pub fn get_log_dir(&self) -> PathBuf {
        self.log_dir.clone()
    }

    pub fn get_current_log_path(&self) -> PathBuf {
        self.log_file.clone()
    }
}
