use anyhow::Result;
use rig::{
    completion::ToolDefinition,
    tool::{ToolDyn, ToolError},
};
use rig::wasm_compat::WasmBoxedFuture;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::Local;
use sysinfo::System;
use crate::logic::logger::Logger;

pub struct SystemTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct SystemArgs {
    action: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    destination: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    old_text: Option<String>,
    #[serde(default)]
    new_text: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    workspace: Option<i32>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

impl ToolDyn for SystemTool {
    fn name(&self) -> String {
        "system".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "system".to_string(),
            description: "Advanced system and filesystem management. Handles file editing (with sandboxing & backups), app launching, system monitoring, and process control.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "read_file", "write_file", "edit_file", "list_directory", 
                            "search_files", "delete_path", "move_path", "open_app", 
                            "get_installed_apps", "get_system_info", "kill_process", 
                            "get_software_version"
                        ],
                        "description": "The system action to perform."
                    },
                    "path": { "type": "string", "description": "Target file or directory path." },
                    "content": { "type": "string", "description": "Full content for write_file." },
                    "old_text": { "type": "string", "description": "Text to replace in edit_file." },
                    "new_text": { "type": "string", "description": "Replacement text in edit_file." },
                    "query": { "type": "string", "description": "Search query for search_files." },
                    "command": { "type": "string", "description": "Command to launch for open_app." },
                    "workspace": { "type": "integer", "description": "Target workspace for open_app." },
                    "source": { "type": "string", "description": "Source for move_path." },
                    "destination": { "type": "string", "description": "Destination for move_path." },
                    "target": { "type": "string", "description": "PID or name for kill_process." },
                    "name": { "type": "string", "description": "Package name for version check." }
                },
                "required": ["action"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: SystemArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(format!("Deserialization failed: {}. Args: {}", e, args).into()))?;

            match json_args.action.as_str() {
                "read_file" => {
                    let path = json_args.path.ok_or_else(|| ToolError::ToolCallError("Path required.".into()))?;
                    Self::do_read_file(&path)
                }
                "write_file" => {
                    let path = json_args.path.ok_or_else(|| ToolError::ToolCallError("Path required.".into()))?;
                    let content = json_args.content.ok_or_else(|| ToolError::ToolCallError("Content required.".into()))?;
                    Self::do_write_file(&logger, &path, &content)
                }
                "edit_file" => {
                    let path = json_args.path.ok_or_else(|| ToolError::ToolCallError("Path required.".into()))?;
                    let old = json_args.old_text.ok_or_else(|| ToolError::ToolCallError("old_text required.".into()))?;
                    let new = json_args.new_text.ok_or_else(|| ToolError::ToolCallError("new_text required.".into()))?;
                    Self::do_edit_file(&logger, &path, &old, &new)
                }
                "list_directory" => {
                    let path = json_args.path.ok_or_else(|| ToolError::ToolCallError("Path required.".into()))?;
                    Self::do_list_directory(&path)
                }
                "search_files" => {
                    let query = json_args.query.ok_or_else(|| ToolError::ToolCallError("Query required.".into()))?;
                    let path = json_args.path.unwrap_or_else(|| "~".to_string());
                    Self::do_search_files(&query, &path).await
                }
                "delete_path" => {
                    let path = json_args.path.ok_or_else(|| ToolError::ToolCallError("Path required.".into()))?;
                    Self::do_delete_path(&logger, &path)
                }
                "move_path" => {
                    let src = json_args.source.ok_or_else(|| ToolError::ToolCallError("Source required.".into()))?;
                    let dest = json_args.destination.ok_or_else(|| ToolError::ToolCallError("Destination required.".into()))?;
                    Self::do_move_path(&logger, &src, &dest)
                }
                "get_system_info" => Self::do_get_system_info(),
                "open_app" => {
                    let cmd = json_args.command.ok_or_else(|| ToolError::ToolCallError("Command required.".into()))?;
                    Self::do_open_app(&logger, &cmd, json_args.workspace).await
                }
                "get_installed_apps" => Self::do_get_installed_apps(),
                "kill_process" => {
                    let target = json_args.target.ok_or_else(|| ToolError::ToolCallError("Target (PID/Name) required.".into()))?;
                    Self::do_kill_process(&logger, &target)
                }
                "get_software_version" => {
                    let name = json_args.name.ok_or_else(|| ToolError::ToolCallError("Package name required.".into()))?;
                    Self::do_get_software_version(&name).await
                }
                _ => Err(ToolError::ToolCallError(format!("Invalid action: {}", json_args.action).into())),
            }
        })
    }
}

impl SystemTool {
    // --- Utilities ---
    
    fn normalize_path(path_str: &str) -> (PathBuf, Option<String>) {
        let mut p = if path_str.starts_with('~') {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(path_str.replace('~', &home))
        } else {
            PathBuf::from(path_str)
        };

        if !p.is_absolute() {
             if let Ok(cwd) = std::env::current_dir() {
                 p = cwd.join(p);
             }
        }

        if let Some(filename) = p.file_name().and_then(|h| h.to_str()) {
            if filename.starts_with('.') && filename.len() > 1 {
                let clean_name = &filename[1..];
                let clean_path = p.with_file_name(clean_name);
                if clean_path.exists() {
                     return (clean_path, Some(format!("Redirected from '{}' to standard version '{}'.", filename, clean_name)));
                }
            }
        }

        (p, None)
    }

    fn validate_sandbox(path: &Path) -> Result<(), ToolError> {
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/home"));
        if path.starts_with(&home) {
            return Ok(());
        }
        Err(ToolError::ToolCallError(format!("SANDBOX_BLOCK: Modification of '{}' prohibited. Write access limited to $HOME.", path.display()).into()))
    }

    fn create_backup(path: &Path) -> Result<PathBuf, ToolError> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let backup_dir = PathBuf::from(home).join(".cache/openaether/backups");
        fs::create_dir_all(&backup_dir).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let rnd = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() % 1000;
        let filename = path.file_name().unwrap().to_str().unwrap();
        let backup_name = format!("{}.bak_{}_{}", filename, timestamp, rnd);
        let backup_path = backup_dir.join(backup_name);

        fs::copy(path, &backup_path).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        Ok(backup_path)
    }

    // --- Actions ---

    fn do_read_file(path_str: &str) -> Result<String, ToolError> {
        let (path, warning) = Self::normalize_path(path_str);
        if path.exists() && path.is_file() {
             use std::io::Read;
             let mut f = fs::File::open(&path).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
             let mut buffer = [0; 4];
             let _ = f.read(&mut buffer);
             if &buffer == b"\x7fELF" || &buffer[..2] == b"MZ" {
                 return Err(ToolError::ToolCallError(format!("BINARY_ERROR: Content at '{}' appears to be binary.", path_str).into()));
             }
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| ToolError::ToolCallError(format!("Read failed: {}", e).into()))?;

        Ok(json!({"status": "success", "content": content, "path": path.to_string_lossy(), "warning": warning}).to_string())
    }

    fn do_write_file(logger: &Arc<Logger>, path_str: &str, content: &str) -> Result<String, ToolError> {
        let (path, warning) = Self::normalize_path(path_str);
        Self::validate_sandbox(&path)?;

        let mut backup = None;
        if path.exists() {
            let bak = Self::create_backup(&path)?;
            backup = Some(bak.to_string_lossy().to_string());
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        }

        fs::write(&path, content).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        logger.log("SYSTEM", &format!("File written: {}. Backup: {:?}", path.display(), backup));
        Ok(json!({"status": "success", "path": path.to_string_lossy(), "backup": backup, "warning": warning}).to_string())
    }

    fn do_edit_file(logger: &Arc<Logger>, path_str: &str, old: &str, new: &str) -> Result<String, ToolError> {
        let (path, warning) = Self::normalize_path(path_str);
        Self::validate_sandbox(&path)?;

        if !path.exists() {
            return Err(ToolError::ToolCallError("File does not exist.".into()));
        }

        let content = fs::read_to_string(&path).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        if !content.contains(old) {
            return Err(ToolError::ToolCallError("old_text not found.".into()));
        }

        let bak = Self::create_backup(&path)?;
        let new_content = content.replace(old, new);
        fs::write(&path, new_content).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

        logger.log("SYSTEM", &format!("File edited: {}. Backup: {}", path.display(), bak.display()));
        Ok(json!({"status": "success", "path": path.to_string_lossy(), "backup": bak.to_string_lossy(), "warning": warning}).to_string())
    }

    fn do_list_directory(path_str: &str) -> Result<String, ToolError> {
        let (path, _) = Self::normalize_path(path_str);
        let entries = fs::read_dir(&path).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        let mut list = Vec::new();
        for entry in entries {
            if let Ok(e) = entry {
                list.push(e.file_name().to_string_lossy().to_string());
            }
        }
        Ok(json!({"status": "success", "entries": list}).to_string())
    }

    async fn do_search_files(query: &str, path_str: &str) -> Result<String, ToolError> {
        let (path, _) = Self::normalize_path(path_str);
        let output = Command::new("find")
            .arg(path)
            .args(&["-maxdepth", "3", "-iname", &format!("*{}*", query)])
            .output()
            .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

        let res = String::from_utf8_lossy(&output.stdout).to_string();
        let results: Vec<&str> = res.lines().take(20).collect();
        Ok(json!({"status": "success", "results": results}).to_string())
    }

    fn do_delete_path(logger: &Arc<Logger>, path_str: &str) -> Result<String, ToolError> {
        let (path, _) = Self::normalize_path(path_str);
        Self::validate_sandbox(&path)?;

        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        } else {
            fs::remove_file(&path).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        }
        logger.log("SYSTEM", &format!("Deleted: {}", path.display()));
        Ok(json!({"status": "success", "message": format!("Deleted {}", path_str)}).to_string())
    }

    fn do_move_path(logger: &Arc<Logger>, src_str: &str, dest_str: &str) -> Result<String, ToolError> {
        let (src, _) = Self::normalize_path(src_str);
        let (dest, _) = Self::normalize_path(dest_str);
        Self::validate_sandbox(&src)?;
        Self::validate_sandbox(&dest)?;

        fs::rename(&src, &dest).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        logger.log("SYSTEM", &format!("Moved: {} to {}", src.display(), dest.display()));
        Ok(json!({"status": "success", "message": format!("Moved {} to {}", src_str, dest_str)}).to_string())
    }

    fn do_get_system_info() -> Result<String, ToolError> {
        let mut sys = System::new_all();
        sys.refresh_all();
        Ok(json!({
            "status": "success",
            "os": System::name().unwrap_or_else(|| "Unknown".to_string()),
            "cpu": {"count": sys.cpus().len(), "model": sys.cpus().get(0).map(|c| c.brand()).unwrap_or("Unknown")},
            "memory": {"total": sys.total_memory() / 1024 / 1024, "used": sys.used_memory() / 1024 / 1024}
        }).to_string())
    }

    async fn do_open_app(logger: &Arc<Logger>, command: &str, workspace: Option<i32>) -> Result<String, ToolError> {
        logger.log("SYSTEM", &format!("Launching app: {}. Workspace={:?}", command, workspace));
        let child = Command::new("/bin/bash").args(&["-c", command]).spawn().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        let _pid = child.id();
        Ok(json!({"status": "success", "message": format!("Launched '{}'", command)}).to_string())
    }

    fn do_get_installed_apps() -> Result<String, ToolError> {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
        let paths = ["/usr/share/applications", &format!("/home/{}/.local/share/applications", user)];
        let mut apps = Vec::new();
        for path in paths {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries {
                    if let Ok(e) = entry {
                        let name = e.file_name().to_string_lossy().to_string();
                        if name.ends_with(".desktop") {
                            apps.push(name.replace(".desktop", ""));
                        }
                    }
                }
            }
        }
        apps.sort();
        apps.dedup();
        Ok(json!({"status": "success", "apps": apps}).to_string())
    }

    fn do_kill_process(logger: &Arc<Logger>, target: &str) -> Result<String, ToolError> {
        let mut cmd = if target.chars().all(|c| c.is_numeric()) {
            let mut c = Command::new("kill");
            c.args(&["-9", target]);
            c
        } else {
            let mut c = Command::new("pkill");
            c.arg(target);
            c
        };

        if cmd.output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?.status.success() {
            logger.log("SYSTEM", &format!("Killed: {}", target));
            Ok(json!({"status": "success"}).to_string())
        } else {
            Err(ToolError::ToolCallError("Process kill failed.".into()))
        }
    }

    async fn do_get_software_version(name: &str) -> Result<String, ToolError> {
        if let Ok(out) = Command::new("pacman").args(&["-Qi", name]).output() {
            if out.status.success() { return Ok(String::from_utf8_lossy(&out.stdout).to_string()); }
        }
        if let Ok(out) = Command::new(name).arg("--version").output() {
            return Ok(String::from_utf8_lossy(&out.stdout).to_string());
        }
        Err(ToolError::ToolCallError("Version not found.".into()))
    }
}
