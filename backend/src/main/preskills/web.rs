use crate::logic::logger::Logger;
use crate::logic::shell::ShellManager;
use anyhow::Result;
use rig::{
    completion::ToolDefinition,
    tool::{ToolDyn, ToolError},
};
use rig::wasm_compat::WasmBoxedFuture;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use scraper::{Html, Selector};
use std::time::Duration;

// --- Helper to handle shell output ---
async fn handle_shell_output(res: Result<serde_json::Value, anyhow::Error>) -> Result<String, ToolError> {
    match res {
        Ok(val) => {
            if let Some(output) = val.get("output").and_then(|v| v.as_str()) {
                Ok(output.to_string())
            } else if let Some(status) = val.get("status").and_then(|v| v.as_str()) {
                if status == "password_required" {
                    Ok("ERROR: Root password required for this command. Please run in a terminal first or use pkexec if supported.".to_string())
                } else {
                    Ok(val.to_string())
                }
            } else {
                Ok(val.to_string())
            }
        }
        Err(e) => Err(ToolError::ToolCallError(e.to_string().into())),
    }
}

// --- Web Search Tool ---

pub struct WebSearchTool {
    pub searxng_url: String,
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct WebSearchArgs {
    query: String,
}

impl ToolDyn for WebSearchTool {
    fn name(&self) -> String {
        "web_search".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "web_search".to_string(),
            description: "Search the internet for external facts, news, or weather. DO NOT use this for local system information.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let searxng_url = self.searxng_url.clone();
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: WebSearchArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            logger.log("WEB", &format!("Searching SearXNG for: {}", json_args.query));

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("OpenAether/1.0")
                .build()
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let res = client.get(format!("{}/search", searxng_url))
                .query(&[("q", json_args.query.as_str()), ("format", "json")])
                .send()
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            if !res.status().is_success() {
                return Err(ToolError::ToolCallError(format!("SearXNG returned status {}", res.status()).into()));
            }

            let data: serde_json::Value = res.json().await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let results = data.get("results").and_then(|r| r.as_array())
                .ok_or_else(|| ToolError::ToolCallError("Invalid response from SearXNG".into()))?;

            let combined: Vec<String> = results.iter().take(4).map(|res| {
                let title = res.get("title").and_then(|v| v.as_str()).unwrap_or("No Title");
                let url = res.get("url").and_then(|v| v.as_str()).unwrap_or("No URL");
                let content = res.get("content").and_then(|v| v.as_str()).unwrap_or("");
                
                // Simple stripping of HTML from snippet
                let clean_content = regex::Regex::new(r"<[^>]+>").unwrap().replace_all(content, "");
                let truncated = if clean_content.len() > 300 {
                    format!("{}...", &clean_content[..297])
                } else {
                    clean_content.to_string()
                };

                format!("### {}\nSource: {}\n{}", title, url, truncated)
            }).collect();

            if combined.is_empty() {
                Ok("No results found.".to_string())
            } else {
                Ok(combined.join("\n\n"))
            }
        })
    }
}

// --- Fetch URL Tool ---

pub struct FetchUrlTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct FetchUrlArgs {
    url: String,
}

impl ToolDyn for FetchUrlTool {
    fn name(&self) -> String {
        "fetch_url".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "fetch_url".to_string(),
            description: "Fetch text from a URL. Aggressively extracts meaningful content while ignoring fluff.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" }
                },
                "required": ["url"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: FetchUrlArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let url = json_args.url.trim();

            // Safety Guard: Reject local file paths and non-http schemas
            if url.starts_with('/') || url.starts_with("./") || url.starts_with("~/") || url.starts_with("file://") {
                return Err(ToolError::ToolCallError(
                    format!("ERROR: '{}' is a local path. fetch_url is for web URLs (http/https) only. Use 'read_file' for local files.", url).into()
                ));
            }

            if !url.starts_with("http://") && !url.starts_with("https://") {
                 return Err(ToolError::ToolCallError(
                    format!("ERROR: '{}' is not a valid web URL. Only http and https schemas are supported.", url).into()
                ));
            }

            logger.log("WEB", &format!("Fetching URL: {}", url));

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("OpenAether/1.0")
                .build()
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let body = client.get(url).send().await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                .text().await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let document = Html::parse_document(&body);
            
            // Aggressive selectors
            let selectors = vec![
                "article", "main", "p", "h1", "h2", "h3", "li"
            ];
            
            // Ignore selectors
            let ignore_selectors = vec![
                "nav", "footer", "sidebar", ".sidebar", "#sidebar", "script", "style", "header"
            ];

            let mut extracted_text = String::new();
            
            for selector_str in selectors {
                let selector = match Selector::parse(selector_str) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                for element in document.select(&selector) {
                    // Check if this element is inside any of the ignored ones
                    let mut parent = element.parent();
                    let mut should_ignore = false;
                    while let Some(p) = parent {
                        if let Some(el) = p.value().as_element() {
                            let tag_name = el.name();
                            if ignore_selectors.contains(&tag_name) {
                                should_ignore = true;
                                break;
                            }
                            // Also check classes for sidebar/nav
                            if let Some(classes) = el.attr("class") {
                                if classes.contains("sidebar") || classes.contains("nav") || classes.contains("footer") {
                                    should_ignore = true;
                                    break;
                                }
                            }
                            if let Some(id) = el.attr("id") {
                                if id.contains("sidebar") || id.contains("nav") || id.contains("footer") {
                                    should_ignore = true;
                                    break;
                                }
                            }
                        }
                        parent = p.parent();
                    }

                    if !should_ignore {
                        let text: String = element.text().collect::<Vec<_>>().join(" ");
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            extracted_text.push_str(trimmed);
                            extracted_text.push_str("\n\n");
                        }
                    }
                }
            }

            if extracted_text.is_empty() {
                Ok("No meaningful content extracted from the page.".to_string())
            } else if extracted_text.len() > 8000 {
                Ok(format!("{}...", &extracted_text[..7997]))
            } else {
                Ok(extracted_text)
            }
        })
    }
}

// --- Network Scan Tool ---

pub struct ScanNetworkTool {
    pub shell: Arc<ShellManager>,
}

impl ToolDyn for ScanNetworkTool {
    fn name(&self) -> String {
        "scan_network".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "scan_network".to_string(),
            description: "Scan local network for active devices using nmap.".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, _args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let shell = self.shell.clone();
        Box::pin(async move {
            let res = shell.execute("nmap -sn 192.168.1.0/24", Duration::from_secs(60)).await;
            handle_shell_output(res).await
        })
    }
}

// --- WiFi Info Tool ---

pub struct GetWifiInfoTool {
    pub shell: Arc<ShellManager>,
}

impl ToolDyn for GetWifiInfoTool {
    fn name(&self) -> String {
        "get_wifi_info".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "get_wifi_info".to_string(),
            description: "Get current SSID and signal strength via nmcli.".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, _args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let shell = self.shell.clone();
        Box::pin(async move {
            let res = shell.execute("nmcli -t -f active,ssid,signal dev wifi", Duration::from_secs(10)).await;
            let output = handle_shell_output(res).await?;
            
            for line in output.lines() {
                if line.starts_with("yes:") {
                    return Ok(line.to_string());
                }
            }
            Ok("No active WiFi detected.".to_string())
        })
    }
}

// --- Check Port Tool ---

pub struct CheckPortTool;

#[derive(Deserialize)]
struct CheckPortArgs {
    host: String,
    port: u16,
}

impl ToolDyn for CheckPortTool {
    fn name(&self) -> String {
        "check_port".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "check_port".to_string(),
            description: "Check if a TCP port is open on a host.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "host": { "type": "string" },
                    "port": { "type": "integer" }
                },
                "required": ["host", "port"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let json_args: CheckPortArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let addr = format!("{}:{}", json_args.host, json_args.port);
            match std::net::TcpStream::connect_timeout(
                &addr.parse().map_err(|_| ToolError::ToolCallError("Invalid host/port".into()))?,
                Duration::from_secs(3)
            ) {
                Ok(_) => Ok(json!({"status": "success", "open": true}).to_string()),
                Err(_) => Ok(json!({"status": "success", "open": false}).to_string()),
            }
        })
    }
}

// --- SSH Command Tool ---

pub struct SshCommandTool {
    pub shell: Arc<ShellManager>,
}

#[derive(Deserialize)]
struct SshCommandArgs {
    host: String,
    user: String,
    command: String,
}

impl ToolDyn for SshCommandTool {
    fn name(&self) -> String {
        "ssh_command".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "ssh_command".to_string(),
            description: "Run a command on a remote host via SSH.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "host": { "type": "string" },
                    "user": { "type": "string" },
                    "command": { "type": "string" }
                },
                "required": ["host", "user", "command"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let shell = self.shell.clone();
        Box::pin(async move {
            let json_args: SshCommandArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let cmd = format!("ssh -o BatchMode=yes {}@{} {}", json_args.user, json_args.host, json_args.command);
            let res = shell.execute(&cmd, Duration::from_secs(30)).await;
            handle_shell_output(res).await
        })
    }
}

// --- Get Device Info Tool ---

pub struct GetDeviceInfoTool {
    pub shell: Arc<ShellManager>,
}

#[derive(Deserialize)]
struct GetDeviceInfoArgs {
    ip: String,
}

impl ToolDyn for GetDeviceInfoTool {
    fn name(&self) -> String {
        "get_device_info".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "get_device_info".to_string(),
            description: "Deep scan a specific IP (ports/OS) using nmap.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ip": { "type": "string" }
                },
                "required": ["ip"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let shell = self.shell.clone();
        Box::pin(async move {
            let json_args: GetDeviceInfoArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            // Note: pkexec might be needed for OS detection in nmap if not root
            let res = shell.execute(&format!("pkexec nmap -sV -O {}", json_args.ip), Duration::from_secs(120)).await;
            handle_shell_output(res).await
        })
    }
}

// --- Open Website Tool ---

pub struct OpenWebsiteTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct OpenWebsiteArgs {
    url: String,
}

impl ToolDyn for OpenWebsiteTool {
    fn name(&self) -> String {
        "open_website".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "open_website".to_string(),
            description: "Open a specific URL in the system's default web browser.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" }
                },
                "required": ["url"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: OpenWebsiteArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let mut url = json_args.url.clone();
            if !url.contains("://") {
                url = format!("https://{}", url);
            }

            logger.log("WEB", &format!("Opening website: {}", url));
            
            // webbrowser::open is synchronous but typically just spawns a process and returns
            if let Err(e) = webbrowser::open(&url) {
                return Err(ToolError::ToolCallError(e.to_string().into()));
            }

            Ok(format!("Successfully opened {} in system browser.", url))
        })
    }
}
