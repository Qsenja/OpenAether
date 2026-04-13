use anyhow::{anyhow, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use vte::{Parser, Perform};

pub struct ShellManager {
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    output_rx: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
}

struct AnsiStripper {
    output: String,
}

impl Perform for AnsiStripper {
    fn print(&mut self, c: char) {
        self.output.push(c);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' {
            self.output.push('\n');
        } else if byte == b'\r' {
            // Ignore carriage returns
        }
    }
}

pub fn get_hyprland_env() -> std::collections::HashMap<String, String> {
    let mut env = std::collections::HashMap::new();
    
    // Default env
    for (k, v) in std::env::vars() {
        env.insert(k, v);
    }

    // Ensure Wayland/Display defaults
    env.entry("DISPLAY".to_string()).or_insert(":0".to_string());
    env.entry("WAYLAND_DISPLAY".to_string()).or_insert("wayland-1".to_string());
    
    let uid = unsafe { libc::getuid() };
    let runtime_dir = env.get("XDG_RUNTIME_DIR")
        .cloned()
        .unwrap_or_else(|| format!("/run/user/{}", uid));
    env.insert("XDG_RUNTIME_DIR".to_string(), runtime_dir.clone());

    // Recover HYPRLAND_INSTANCE_SIGNATURE
    let hypr_runtime = std::path::PathBuf::from(&runtime_dir).join("hypr");
    if hypr_runtime.exists() {
        if let Ok(entries) = std::fs::read_dir(hypr_runtime) {
            let mut dirs: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .collect();
            
            dirs.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            dirs.reverse(); // Newest first

            if let Some(latest) = dirs.first() {
                if let Some(sig) = latest.file_name().to_str() {
                    env.insert("HYPRLAND_INSTANCE_SIGNATURE".to_string(), sig.to_string());
                }
            }
        }
    }

    env
}

impl ShellManager {
    pub fn new() -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["--noprofile", "--norc"]);
        
        // Add Hyprland env
        let hypr_env = get_hyprland_env();
        for (k, v) in hypr_env {
            cmd.env(k, v);
        }
        cmd.env("PS1", "");
        cmd.env("PS2", "");

        let _child = pair.slave.spawn_command(cmd)?;
        
        let master = pair.master;
        let mut reader = master.try_clone_reader()?;
        // EXPLICIT CAST to dyn Write
        let writer: Box<dyn Write + Send> = master.take_writer()?;
        
        let (tx, rx) = mpsc::unbounded_channel();
        
        // Background read thread
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 { break; }
                let _ = tx.send(buf[..n].to_vec());
            }
        });

        let mut shell = Self {
            pty_writer: Arc::new(Mutex::new(writer)),
            output_rx: Arc::new(Mutex::new(rx)),
        };

        // Suppress echo and clear prompt
        shell.execute_sync("stty -echo; export PS1=''; export PS2=''", Duration::from_millis(500))?;

        Ok(shell)
    }

    fn execute_sync(&mut self, command: &str, timeout: Duration) -> Result<String> {
        let mut writer = self.pty_writer.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        let full_command = format!("{}; echo DONE_INIT\n", command);
        writer.write_all(full_command.as_bytes())?;
        writer.flush()?;
        
        let mut output = Vec::new();
        let start = Instant::now();
        let mut rx = self.output_rx.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        while start.elapsed() < timeout {
            while let Ok(data) = rx.try_recv() {
                output.extend(data);
            }
            if String::from_utf8_lossy(&output).contains("DONE_INIT") {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        Ok(String::from_utf8_lossy(&output).to_string())
    }

    pub async fn execute(&self, command: &str, timeout: Duration) -> Result<serde_json::Value> {
        let sentinel = format!("COMMAND_FINISHED_{}", chrono::Utc::now().timestamp_millis());
        let full_command = format!("{}; echo {}\n", command, sentinel);

        {
            let mut writer = self.pty_writer.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
            writer.write_all(full_command.as_bytes())?;
            writer.flush()?;
        }

        let mut output = Vec::new();
        let start = Instant::now();

        while start.elapsed() < timeout {
            {
                let mut rx = self.output_rx.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
                while let Ok(data) = rx.try_recv() {
                    output.extend(data);
                }
            }

            let current_str = String::from_utf8_lossy(&output);
            
            // Password detection
            if current_str.to_lowercase().contains("password") && current_str.trim().ends_with(':') {
                return Ok(serde_json::json!({
                    "status": "password_required",
                    "output_so_far": self.strip_ansi(&current_str),
                    "command": command
                }));
            }

            if current_str.contains(&sentinel) {
                break;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let clean_output = self.strip_ansi(&String::from_utf8_lossy(&output));
        let mut result = clean_output.replace(&sentinel, "").trim().to_string();
        
        // Remove the command if it was echoed back (though stty -echo should prevent this)
        if result.starts_with(command) {
            result = result.replace(command, "").trim().to_string();
        }

        Ok(serde_json::json!({
            "status": "success",
            "output": result
        }))
    }

    fn strip_ansi(&self, text: &str) -> String {
        let mut stripper = AnsiStripper { output: String::new() };
        let mut parser = Parser::new();

        for byte in text.as_bytes() {
            parser.advance(&mut stripper, *byte);
        }

        stripper.output
    }

    pub fn send_input(&self, text: &str) -> Result<()> {
        let mut writer = self.pty_writer.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        writer.write_all(format!("{}\n", text).as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    pub fn interrupt(&self) -> Result<()> {
        let mut writer = self.pty_writer.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        writer.write_all(&[0x03])?; // Ctrl+C
        writer.flush()?;
        Ok(())
    }
}
