use crate::logic::logger::Logger;
use crate::logic::ollama::OllamaClient;
use anyhow::Result;
use rig::{
    completion::ToolDefinition,
    tool::{ToolDyn, ToolError},
};
use rig::wasm_compat::WasmBoxedFuture;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use chrono::Local;

// --- Memory Utility ---
fn get_memory_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/openaether/memory.json")
}

#[derive(Serialize, Deserialize, Clone)]
struct MemoryEntry {
    value: String,
    timestamp: String,
}

fn load_memory() -> HashMap<String, MemoryEntry> {
    let path = get_memory_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            return serde_json::from_str(&content).unwrap_or_default();
        }
    }
    HashMap::new()
}

fn save_memory(mem: &HashMap<String, MemoryEntry>) -> Result<()> {
    let path = get_memory_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(mem)?;
    fs::write(path, content)?;
    Ok(())
}

// --- Remember Tool ---
pub struct RememberTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct RememberArgs {
    key: String,
    value: String,
}

impl ToolDyn for RememberTool {
    fn name(&self) -> String {
        "remember".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "remember".to_string(),
            description: "Store a persistent note in memory. Does NOT write to user files. Use ONLY for preferences and facts.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "The name/identifier for this note (e.g. 'user_name', 'pending_plan')" },
                    "value": { "type": "string", "description": "The value to store." }
                },
                "required": ["key", "value"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: RememberArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let mut mem = load_memory();
            mem.insert(json_args.key.clone(), MemoryEntry {
                value: json_args.value,
                timestamp: Local::now().to_rfc3339(),
            });

            if let Err(e) = save_memory(&mem) {
                return Err(ToolError::ToolCallError(format!("Failed to save memory: {}", e).into()));
            }

            logger.log("AGENT", &format!("Memory saved for key: {}", json_args.key));
            Ok(json!({"status": "success", "message": format!("Stored memory for key '{}'.", json_args.key)}).to_string())
        })
    }
}

// --- Recall Tool ---
pub struct RecallTool;

#[derive(Deserialize)]
struct RecallArgs {
    key: Option<String>,
}

impl ToolDyn for RecallTool {
    fn name(&self) -> String {
        "recall".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "recall".to_string(),
            description: "Recall a previously remembered note by key. Omit key to see ALL stored memories.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "The key to look up. Leave empty to retrieve all stored memories." }
                }
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let json_args: RecallArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let mem = load_memory();
            if let Some(key) = json_args.key {
                if let Some(entry) = mem.get(&key) {
                    return Ok(json!({
                        "status": "success",
                        "key": key,
                        "value": entry.value,
                        "saved_at": entry.timestamp
                    }).to_string());
                }
                return Ok(json!({
                    "status": "error",
                    "message": format!("No memory found for key '{}'. Use recall() without a key to see all.", key)
                }).to_string());
            }

            if mem.is_empty() {
                return Ok(json!({"status": "success", "message": "Memory is empty.", "memory": {}}).to_string());
            }

            let simplified: HashMap<String, String> = mem.iter().map(|(k, v)| (k.clone(), v.value.clone())).collect();
            Ok(json!({"status": "success", "memory": simplified}).to_string())
        })
    }
}

// --- Set Timer Tool ---
pub struct SetTimerTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct SetTimerArgs {
    seconds: u64,
    label: Option<String>,
}

impl ToolDyn for SetTimerTool {
    fn name(&self) -> String {
        "set_timer".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "set_timer".to_string(),
            description: "Set a non-blocking countdown timer. Shows a notification when done.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "seconds": { "type": "integer", "description": "Number of seconds to wait" },
                    "label": { "type": "string", "description": "What this timer is for" }
                },
                "required": ["seconds"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: SetTimerArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let label = json_args.label.unwrap_or_else(|| "Timer".to_string());
            let seconds = json_args.seconds;

            let timer_label = label.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                let _ = std::process::Command::new("notify-send")
                    .args(&["-u", "critical", "-t", "0", "⏰ Timer expired!", &timer_label])
                    .spawn();
            });

            logger.log("AGENT", &format!("Timer set: {}s for {}", seconds, label));
            Ok(json!({"status": "success", "message": format!("Timer '{}' set for {}s.", label, seconds)}).to_string())
        })
    }
}

// --- Schedule Task Tool ---
pub struct ScheduleTaskTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct ScheduleTaskArgs {
    commands: String,
    delay_seconds: u64,
    label: Option<String>,
}

impl ToolDyn for ScheduleTaskTool {
    fn name(&self) -> String {
        "schedule_task".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "schedule_task".to_string(),
            description: "Schedule a shell command sequence to run after a delay using systemd-run.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "commands": { "type": "string", "description": "The shell command(s) to execute" },
                    "delay_seconds": { "type": "integer", "description": "How many seconds to wait before running." },
                    "label": { "type": "string", "description": "Label for this job." }
                },
                "required": ["commands", "delay_seconds"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: ScheduleTaskArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let label = json_args.label.unwrap_or_else(|| "scheduled-task".to_string());
            let _safe_label: String = label.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .take(40)
                .collect();
            
            let output = std::process::Command::new("systemd-run")
                .args(&[
                    "--user",
                    &format!("--on-active={}s", json_args.delay_seconds),
                    &format!("--description={}", label),
                    "/bin/bash",
                    "-c",
                    &json_args.commands
                ])
                .output()
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            if output.status.success() {
                let combined = String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);
                let re = regex::Regex::new(r"unit: ([a-zA-Z0-9.-]+)").unwrap();
                let unit = re.captures(&combined)
                    .and_then(|cap| cap.get(1))
                    .map(|m| m.as_str())
                    .unwrap_or("unknown");

                logger.log("AGENT", &format!("Task scheduled: {} in {}s", label, json_args.delay_seconds));
                Ok(json!({
                    "status": "success",
                    "message": format!("Task '{}' scheduled. It will run in {}s.", label, json_args.delay_seconds),
                    "unit": unit
                }).to_string())
            } else {
                Err(ToolError::ToolCallError(String::from_utf8_lossy(&output.stderr).to_string().into()))
            }
        })
    }
}

// --- Get DateTime Tool ---
pub struct GetDateTimeTool;

impl ToolDyn for GetDateTimeTool {
    fn name(&self) -> String {
        "get_current_datetime".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "get_current_datetime".to_string(),
            description: "Get the current date and time (ISO format or human-readable).".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, _args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let now = Local::now();
            Ok(json!({
                "status": "success",
                "datetime": now.format("%Y-%m-%d %H:%M:%S").to_string(),
                "iso": now.to_rfc3339(),
                "weekday": now.format("%A").to_string()
            }).to_string())
        })
    }
}

// --- Translate Tool ---
pub struct TranslateTool {
    pub ollama: Arc<OllamaClient>,
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct TranslateArgs {
    text: String,
    target_lang: String,
    _source_lang: Option<String>,
}

impl ToolDyn for TranslateTool {
    fn name(&self) -> String {
        "translate".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "translate".to_string(),
            description: "Translate text from one language to another.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "The text to translate." },
                    "target_lang": { "type": "string", "description": "Target language" },
                    "source_lang": { "type": "string", "description": "Source language (default: auto)" }
                },
                "required": ["text", "target_lang"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let ollama = self.ollama.clone();
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: TranslateArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let prompt = format!(
                "Translate the following to {}. Output only the translation:\n\n{}",
                json_args.target_lang, json_args.text
            );

            // Using the primary model
            match ollama.chat("translategemma:4b", vec![crate::logic::ollama::Message {
                role: "user".to_string(),
                content: prompt.clone(),
            }]).await {
                Ok(resp) => {
                    let text = resp.message.map(|m| m.content).unwrap_or_default();
                    Ok(json!({"status": "success", "translation": text.trim()}).to_string())
                },
                Err(e) => {
                    logger.log("AGENT", &format!("Primary translation model failed: {}. Trying fallback...", e));
                    match ollama.chat("qwen2.5:14b", vec![crate::logic::ollama::Message {
                        role: "user".to_string(),
                        content: prompt,
                    }]).await {
                        Ok(resp) => {
                            let text = resp.message.map(|m| m.content).unwrap_or_default();
                            Ok(json!({
                                "status": "success",
                                "translation": text.trim(),
                                "note": "Translated using fallback model qwen2.5:14b"
                            }).to_string())
                        },
                        Err(e2) => Err(ToolError::ToolCallError(format!("Translation failed on all models: {}", e2).into()))
                    }
                }
            }
        })
    }
}

// --- Run Python Tool ---
pub struct RunPythonTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct RunPythonArgs {
    code: String,
}

impl ToolDyn for RunPythonTool {
    fn name(&self) -> String {
        "run_python".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "run_python".to_string(),
            description: "Execute a Python code snippet.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string" }
                },
                "required": ["code"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: RunPythonArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            logger.log("AGENT", "Executing Python snippet");

            // Use the project's venv python if it exists, otherwise fallback to system python3
            let venv_python = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                .join("../logic/venv/bin/python");
            
            let python_exe = if venv_python.exists() {
                venv_python.to_string_lossy().to_string()
            } else {
                "python3".to_string()
            };

            let output = std::process::Command::new(python_exe)
                .args(&["-c", &json_args.code])
                .output()
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            if output.status.success() {
                Ok(json!({
                    "status": "success",
                    "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string()
                }).to_string())
            } else {
                Ok(json!({
                    "status": "error",
                    "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                    "exit_code": output.status.code()
                }).to_string())
            }
        })
    }
}

// --- Calculate Discount Tool ---
pub struct CalculateDiscountTool;

#[derive(Deserialize)]
struct CalculateDiscountArgs {
    original_price: f64,
    discount_percent: f64,
}

impl ToolDyn for CalculateDiscountTool {
    fn name(&self) -> String {
        "calculate_discount".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "calculate_discount".to_string(),
            description: "Calculates the final price after applying a discount percentage. Use this for math examples.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "original_price": { "type": "number", "description": "The initial price before discount" },
                    "discount_percent": { "type": "number", "description": "The discount percentage (0-100)" }
                },
                "required": ["original_price", "discount_percent"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let json_args: CalculateDiscountArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let final_price = json_args.original_price * (1.0 - (json_args.discount_percent / 100.0));
            Ok(json!({"final_price": final_price}).to_string())
        })
    }
}

// --- Report Error Tool ---
pub struct ReportErrorTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct ReportErrorArgs {
    issue: String,
    details: Option<String>,
}

impl ToolDyn for ReportErrorTool {
    fn name(&self) -> String {
        "report_error".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "report_error".to_string(),
            description: "Report a task failure.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "issue": { "type": "string" },
                    "details": { "type": "string" }
                },
                "required": ["issue"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: ReportErrorArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            logger.log("ERROR", &format!("Agent reported issue: {} - Details: {}", json_args.issue, json_args.details.unwrap_or_default()));
            Ok(json!({"status": "success"}).to_string())
        })
    }
}
