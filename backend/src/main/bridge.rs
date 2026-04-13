use std::process::{Child, Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use std::path::PathBuf;
use crate::logic::logger::Logger;

#[derive(Serialize, Deserialize, Debug)]
pub struct BridgeRequest {
    pub id: String,
    #[serde(rename = "type")]
    pub req_type: String,
    pub name: Option<String>,
    pub args: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BridgeResponse {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub res_type: String,
    pub status: Option<String>,
    pub result: Option<serde_json::Value>,
    pub tools: Option<Vec<String>>,
    pub schemas: Option<Vec<serde_json::Value>>,
    pub message: Option<String>,
}

pub struct PythonBridge {
    child: Arc<Mutex<Option<Child>>>,
    python_path: PathBuf,
    worker_script: PathBuf,
    logger: Arc<Logger>,
}

impl PythonBridge {
    pub fn new(python_path: PathBuf, worker_script: PathBuf, logger: Arc<Logger>) -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            python_path,
            worker_script,
            logger,
        }
    }

    pub fn start(&self) -> Result<()> {
        let mut child_guard = self.child.lock().unwrap();
        if child_guard.is_some() {
            return Ok(());
        }

        let mut child = Command::new(&self.python_path)
            .arg(&self.worker_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        // WAIT FOR READY SIGNAL
        if let Some(stdout) = child.stdout.as_mut() {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            if let Ok(_) = reader.read_line(&mut line) {
                // Potential logging: "Bridge ready: {}", line
            }
        }

        *child_guard = Some(child);
        Ok(())
    }

    pub fn stop(&self) {
        let mut child_guard = self.child.lock().unwrap();
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill();
        }
    }

    pub fn execute(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value> {
        let id = format!("req_{}", chrono::Utc::now().timestamp_millis());
        let request = BridgeRequest {
            id: id.clone(),
            req_type: "execute".to_string(),
            name: Some(name.to_string()),
            args: Some(args),
        };

        let response = self.send_and_receive(request)?;
        
        if let Some(status) = response.status {
            if status == "success" {
                return Ok(response.result.unwrap_or(serde_json::Value::Null));
            } else {
                return Err(anyhow!(response.message.unwrap_or_else(|| "Unknown error".to_string())));
            }
        }
        
        Err(anyhow!("Invalid response from bridge"))
    }

    pub fn get_schemas(&self) -> Result<Vec<serde_json::Value>> {
        let id = "get_schemas".to_string();
        let request = BridgeRequest {
            id: id.clone(),
            req_type: "get_schemas".to_string(),
            name: None,
            args: None,
        };

        let response = self.send_and_receive(request)?;
        Ok(response.schemas.unwrap_or_default())
    }

    fn send_and_receive(&self, request: BridgeRequest) -> Result<BridgeResponse> {
        let mut child_guard = self.child.lock().unwrap();
        let child = child_guard.as_mut().ok_or_else(|| anyhow!("Bridge not started"))?;
        
        let stdin = child.stdin.as_mut().ok_or_else(|| anyhow!("No stdin"))?;
        let stdout = child.stdout.as_mut().ok_or_else(|| anyhow!("No stdout"))?;
        
        let json = serde_json::to_string(&request)? + "\n";
        stdin.write_all(json.as_bytes())?;
        stdin.flush()?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        
        // Skip potential debug output or empty lines until we find a JSON object
        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if val.get("type").and_then(|t| t.as_str()) == Some("log") {
                        // LOG PACKET: Send to logger
                        if let (Some(event_type), Some(data)) = (
                            val.get("event_type").and_then(|t| t.as_str()),
                            val.get("data").cloned()
                        ) {
                            self.logger.log_event(event_type, data);
                        }
                    } else if let Ok(response) = serde_json::from_value::<BridgeResponse>(val) {
                        if response.id.as_deref() == Some(&request.id) || response.id.is_none() {
                            return Ok(response);
                        }
                    }
                }
            }
            line.clear();
        }
        
        Err(anyhow!("No valid response from bridge"))
    }
}
