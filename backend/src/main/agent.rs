use crate::logic::bridge::PythonBridge;
use crate::logic::logger::Logger;
use crate::logic::memory::{MemoryManager, TextRecord};
use crate::logic::shell::ShellManager;
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use rig::{
    client::Nothing,
    completion::{Message as RigMessage, ToolDefinition},
    prelude::*,
    streaming::StreamingChat,
    tool::{ToolDyn, ToolError},
    vector_store::{VectorSearchRequest, VectorStoreIndex},
};
use rig::wasm_compat::WasmBoxedFuture;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use regex::Regex;

use arrow_array::{
    types::Float32Type, ArrayRef, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Fields, Schema};

pub struct Agent {
    ollama_client: rig::providers::ollama::Client,
    bridge: Arc<PythonBridge>,
    memory_manager: Arc<MemoryManager>,
    shell: Arc<ShellManager>,
    logger: Arc<Logger>,
    system_prompt: String,
    available_tools: Vec<serde_json::Value>,
    temperature: f64,
    top_p: f64,
    searxng_url: String,
}

struct PythonTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
    bridge: Arc<PythonBridge>,
    logger: Arc<Logger>,
}

impl ToolDyn for PythonTool {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let bridge = self.bridge.clone();
        let name = self.name.clone();
        Box::pin(async move {
            let json_args: serde_json::Value = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            let json_args_clone = json_args.clone();

            let res = match bridge.execute(&name, json_args) {
                Ok(res) => Ok(res.to_string()),
                Err(e) => Err(ToolError::ToolCallError(e.to_string().into())),
            };

            if let Ok(ref output) = res {
                self.logger.log_tool(&name, json_args_clone, output);
            }

            res
        })
    }
}

// Native tool to discover other tools via RAG
struct DiscoverTools {
    memory_manager: Arc<MemoryManager>,
    logger: Arc<Logger>,
}

#[derive(serde::Deserialize)]
struct DiscoverToolsArgs {
    /// Search query for the internal tool manual (e.g., 'network scanning')
    query: String,
}

impl ToolDyn for DiscoverTools {
    fn name(&self) -> String {
        "discover_tools".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "discover_tools".to_string(),
            description: "Search your internal knowledge base for tools that can help fulfill the request. Use this if you don't know the name of a specific tool.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What capability are you looking for?"
                    }
                },
                "required": ["query"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let memory = self.memory_manager.clone();
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: DiscoverToolsArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let mut results_text = String::from("Internal Tool Manual Search Results:\n");
            
            if let Ok(index) = memory.get_tool_index().await {
                let request = VectorSearchRequest::builder()
                    .query(&json_args.query)
                    .samples(3)
                    .build()
                    .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

                if let Ok(results) = index.top_n::<TextRecord>(request).await {
                    for (_, _, record) in results {
                        results_text.push_str(&format!("---\n{}\n", record.text));
                    }
                }
            }

            if results_text.is_empty() {
                results_text = "No tools found matching that description.".to_string();
            }

            logger.log_tool("discover_tools", json!({"query": json_args.query}), &results_text);
            Ok(results_text)
        })
    }
}

// Native Rust tool for the shell
struct RunCommandTool {
    shell: Arc<ShellManager>,
    logger: Arc<Logger>,
}

#[derive(serde::Deserialize)]
struct RunCommandArgs {
    /// The shell command to execute
    command: String,
}

impl ToolDyn for RunCommandTool {
    fn name(&self) -> String {
        "run_command".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "run_command".to_string(),
            description: "Run a generic shell command. This session is persistent and maintains directory/environment between calls.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to run"
                    }
                },
                "required": ["command"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let shell = self.shell.clone();
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: RunCommandArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let res = match shell.execute(&json_args.command, Duration::from_secs(60)).await {
                Ok(res) => Ok(res.to_string()),
                Err(e) => Err(ToolError::ToolCallError(e.to_string().into())),
            };

            if let Ok(ref output) = res {
                logger.log_tool("run_command", json!({"command": json_args.command}), output);
            }

            res
        })
    }
}

impl Agent {
    pub fn new(
        _ollama: Arc<crate::logic::ollama::OllamaClient>,
        bridge: Arc<PythonBridge>,
        memory_manager: Arc<MemoryManager>,
        shell: Arc<ShellManager>,
        logger: Arc<Logger>,
        system_prompt: String,
        available_tools: Vec<serde_json::Value>,
        temperature: f64,
        top_p: f64,
        searxng_url: String,
    ) -> Self {
        let ollama_client = rig::providers::ollama::Client::builder()
            .base_url("http://localhost:11434")
            .api_key(Nothing)
            .build()
            .expect("Failed to create Ollama client");

        Self {
            ollama_client,
            bridge,
            memory_manager,
            shell,
            logger,
            system_prompt,
            available_tools,
            temperature,
            top_p,
            searxng_url,
        }
    }

    pub fn get_all_tool_definitions(&self) -> Vec<serde_json::Value> {
        let mut defs = self.available_tools.clone();
        
        // Add native tool schemas manually for indexing
        defs.push(json!({
            "name": "run_command",
            "description": "Run a generic shell command. This session is persistent and maintains directory/environment between calls."
        }));
        defs.push(json!({
            "name": "discover_tools",
            "description": "Search your internal knowledge base for tools that can help fulfill the request. Use this if you don't know the name of a specific tool."
        }));
        defs.push(json!({
            "name": "web_search",
            "description": "Search the web for current information, news, or specific facts."
        }));
        defs.push(json!({
            "name": "fetch_url",
            "description": "Extract the text content from a specific URL. Useful for reading articles or deep-diving into a result."
        }));
        defs.push(json!({
            "name": "scan_network",
            "description": "Scan the local network for active devices and open ports using nmap."
        }));
        defs.push(json!({
            "name": "get_wifi_info",
            "description": "List available Wi-Fi networks and connection status."
        }));
        
        defs
    }

    /// Detects if the assistant's response is likely a "leaked" tool call (raw JSON)
    /// instead of actual helpful text. Scanning the full content ensures we catch
    /// leaks even if they follow a paragraph of normal text.
    fn is_likely_garbage(&self, text: &str) -> bool {
        // 1. Detect Non-Latin Garbage (Mojibake/Thai/Chinese leaks)
        let non_latin_count = text.chars()
            .filter(|c| !c.is_ascii() && !c.is_numeric() && !c.is_whitespace() && !c.is_ascii_punctuation())
            .count();
        
        if non_latin_count > 5 || text.contains("檐僇") {
             return true;
        }

        // 2. Detect Leaked Tool Calls (JSON in text)
        // We use a robust regex to find {"name": ... "args": ...} patterns
        let re = Regex::new(r#"(?i)\{\s*["']name["']\s*:\s*["'][^"']+["']\s*,\s*["']args"#).unwrap();
        if re.is_match(text) {
            return true;
        }

        false
    }

    pub async fn process<F>(
        &self,
        messages: &mut Vec<crate::logic::ollama::Message>,
        model: &str,
        mut event_callback: F,
    ) -> Result<()>
    where
        F: FnMut(serde_json::Value) + Send + 'static,
    {
        self.logger.log("AGENT", "Initializing Rig Agent");

        // 1. Build Rig tools
        let mut rig_tools: Vec<Box<dyn ToolDyn>> = Vec::new();
        for tool in &self.available_tools {
            if let Some(func) = tool.get("function") {
                let name = func
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let description = func
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let parameters = func.get("parameters").cloned().unwrap_or(json!({}));

                rig_tools.push(Box::new(PythonTool {
                    name,
                    description,
                    parameters,
                    bridge: self.bridge.clone(),
                    logger: self.logger.clone(),
                }));
            }
        }

        // Add native tools
        rig_tools.push(Box::new(RunCommandTool {
            shell: self.shell.clone(),
            logger: self.logger.clone(),
        }));
        rig_tools.push(Box::new(DiscoverTools {
            memory_manager: self.memory_manager.clone(),
            logger: self.logger.clone(),
        }));

        // Add native preskills
        rig_tools.push(Box::new(crate::logic::preskills::web::WebSearchTool {
            searxng_url: self.searxng_url.clone(),
            logger: self.logger.clone(),
        }));
        rig_tools.push(Box::new(crate::logic::preskills::web::FetchUrlTool {
            logger: self.logger.clone(),
        }));
        rig_tools.push(Box::new(crate::logic::preskills::web::ScanNetworkTool {
            shell: self.shell.clone(),
        }));
        rig_tools.push(Box::new(crate::logic::preskills::web::GetWifiInfoTool {
            shell: self.shell.clone(),
        }));
        rig_tools.push(Box::new(crate::logic::preskills::web::CheckPortTool));
        rig_tools.push(Box::new(crate::logic::preskills::web::SshCommandTool {
            shell: self.shell.clone(),
        }));
        rig_tools.push(Box::new(crate::logic::preskills::web::GetDeviceInfoTool {
            shell: self.shell.clone(),
        }));
        rig_tools.push(Box::new(crate::logic::preskills::web::OpenWebsiteTool {
            logger: self.logger.clone(),
        }));

        // 2. Window Context Injection (Filtered for intelligence)
        let mut window_context = String::new();
        if let Ok(res) = self.shell.execute("hyprctl clients -j", Duration::from_secs(2)).await {
            // Unpack the shell result which is {status: "success", output: "..."}
            if let Some(raw_output) = res.get("output").and_then(|o| o.as_str()) {
                let windows: Vec<serde_json::Value> = serde_json::from_str(raw_output).unwrap_or_default();
                let filtered: Vec<String> = windows.into_iter()
                    .filter(|w| w.get("title").and_then(|t| t.as_str()).map(|s| !s.is_empty()).unwrap_or(false))
                    .map(|w| {
                        let focused = if w.get("focus").and_then(|f| f.as_bool()).unwrap_or(false) { " [FOCUSED]" } else { "" };
                        format!("{}: {} (Workspace {}){}", 
                            w.get("class").and_then(|c| c.as_str()).unwrap_or("Unknown"),
                            w.get("title").and_then(|t| t.as_str()).unwrap_or("Untitled"),
                            w.get("workspace").and_then(|ws| ws.get("id")).and_then(|id| id.as_i64()).unwrap_or(0),
                            focused
                        )
                    })
                    .collect();
                
                if !filtered.is_empty() {
                    window_context = format!(
                        "\n\n[OPEN WINDOWS]:\n{}",
                        filtered.join("\n")
                    );
                }
            }
        }

        // 3. Run the Agent with history support
        let user_query = messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // 4. Setup Memory Context (Retrieval-Augmented Generation)
        let mut memory_context = String::new();
        if let Ok(index) = self.memory_manager.get_index().await {
            // Search for top 3 relevant conversations/facts
            let request = VectorSearchRequest::builder()
                .query(&user_query)
                .samples(3)
                .build()
                .ok();

            if let Some(req) = request {
                if let Ok(results) = index.top_n::<TextRecord>(req).await {
                    if !results.is_empty() {
                        memory_context = String::from("\n\n[RELEVANT MEMORIES FROM PAST CONVERSATIONS]:");
                        for (_, _, record) in results {
                            memory_context.push_str(&format!("\n- {}", record.text.trim()));
                        }
                    }
                }
            }
        }

        // 5. Build the Rig Agent with Dynamic Preamble
        let preamble = format!("{}{}{}", self.system_prompt, window_context, memory_context);
        
        let rig_agent = self
            .ollama_client
            .agent(model)
            .preamble(&preamble)
            .additional_params(json!({
                "options": {
                    "temperature": (self.temperature).min(0.7), // Cap temperature for stability
                    "top_p": self.top_p,
                    "repeat_penalty": 1.2,
                    "repeat_last_n": 64
                }
            }))
            .default_max_turns(10)
            .tools(rig_tools)
            .build();

        // Convert history to Rig messages (excluding the last message which is the current query)
        // QUARANTINE: Filter out previous messages that contains mojibake/Thai/Chinese to prevent relapses
        let history = messages
            .iter()
            .take(messages.len().saturating_sub(1))
            .filter(|m| {
                if m.role == "assistant" && self.is_likely_garbage(&m.content) {
                    self.logger.log("AGENT", "Quarantining garbage history message to prevent relapses.");
                    return false;
                }
                true
            })
            .map(|m| {
                if m.role == "user" {
                    RigMessage::user(&m.content)
                } else {
                    RigMessage::assistant(&m.content)
                }
            })
            .collect::<Vec<_>>();

        // Use streaming for real-time UI updates
        let mut stream = rig_agent.stream_chat(&user_query, history).await;
        let mut assistant_content = String::new();

        while let Some(chunk_res) = stream.next().await {
            match chunk_res {
                Ok(item) => match item {
                    rig::agent::MultiTurnStreamItem::StreamAssistantItem(content) => match content {
                        rig::streaming::StreamedAssistantContent::Text(text) => {
                            let text_val = text.text;
                            
                            // SHIELD: If this specific chunk looks like a raw JSON tool call,
                            // we block it from the UI to keep the chat clean.
                            if (text_val.contains("\"name\":") || text_val.contains("'name':")) && 
                               (text_val.contains("\"args") || text_val.contains("'args")) {
                                self.logger.log("AGENT", "SHIELD: Suppressed JSON leak in text stream.");
                            } else {
                                assistant_content.push_str(&text_val);
                                event_callback(json!({"type": "agent_message", "content": text_val}));
                            }
                        }
                        rig::streaming::StreamedAssistantContent::ToolCall { tool_call, .. } => {
                            self.logger.log("AGENT", &format!("Calling tool: {} ({})", tool_call.function.name, tool_call.id));
                            event_callback(json!({
                                "type": "tool_call",
                                "name": tool_call.function.name,
                                "call_id": tool_call.id,
                                "args": tool_call.function.arguments.to_string()
                            }));
                        }
                        _ => {}
                    },
                    rig::agent::MultiTurnStreamItem::StreamUserItem(content) => match content {
                        rig::streaming::StreamedUserContent::ToolResult { tool_result, .. } => {
                            self.logger.log("AGENT", &format!("Tool result received for: {}", tool_result.id));
                            event_callback(json!({
                                "type": "tool_output",
                                "name": "tool_result",
                                "call_id": tool_result.id,
                                "output": format!("{:?}", tool_result.content)
                            }));
                        }
                    },
                    _ => {}
                },
                Err(e) => {
                    self.logger.log("AGENT", &format!("Stream error: {}", e));
                    return Err(anyhow!("Stream error: {}", e));
                }
            }
        }

        // --- GARBAGE FILTER ---
        // If the model leaked a tool call as raw text, its content will contain raw JSON.
        // We detect this and skip saving to memory to prevent "muscle memory" pollution.
        let is_garbage = self.is_likely_garbage(&assistant_content);
        if is_garbage {
            self.logger.log("AGENT", "Memory save BLOCKED: Detected malformed tool call leak (JSON in text stream).");
        }

        // Update local history
        messages.push(crate::logic::ollama::Message {
            role: "assistant".to_string(),
            content: assistant_content.clone(),
        });

        // 6. Save to memory if available AND not garbage
        if !is_garbage && self.memory_manager.table.get().is_some() {
            let memory_id = format!("mem_{}", chrono::Utc::now().timestamp_millis());
            let memory_text = format!("User: {}\nAssistant: {}", user_query, assistant_content);

            let record = TextRecord {
                id: memory_id,
                text: memory_text,
            };

            let embedding_model = self
                .memory_manager
                .embedding_model
                .get()
                .ok_or_else(|| anyhow!("Embedding model not initialized"))?;
            let table = self
                .memory_manager
                .table
                .get()
                .ok_or_else(|| anyhow!("LanceDB table not initialized"))?;

            let batch = rig::embeddings::EmbeddingsBuilder::new(embedding_model.clone())
                .document(record)?
                .build()
                .await
                .map_err(|e| anyhow!("Failed to embed record: {}", e))?;

            // Convert Embeddings to RecordBatch for LanceDB 0.23 IntoArrow requirement
            let schema = Arc::new(Schema::new(Fields::from(vec![
                Field::new("id", DataType::Utf8, false),
                Field::new("text", DataType::Utf8, false),
                Field::new(
                    "vector",
                    DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float32, true)),
                        768,
                    ),
                    false,
                ),
            ])));

            let ids = StringArray::from_iter_values(batch.iter().map(|(r, _)| &r.id));
            let texts = StringArray::from_iter_values(batch.iter().map(|(r, _)| &r.text));
            let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                batch
                    .into_iter()
                    .map(|(_, embs)| {
                        Some(
                            embs.first()
                                .vec
                                .into_iter()
                                .map(|v| Some(v as f32))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
                768,
            );

            let record_batch = RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(ids) as ArrayRef,
                    Arc::new(texts) as ArrayRef,
                    Arc::new(vectors) as ArrayRef,
                ],
            )
            .map_err(|e| anyhow!("Failed to create RecordBatch: {}", e))?;

            let schema = record_batch.schema();
            let batches = RecordBatchIterator::new(vec![Ok(record_batch)], schema);

            if let Err(e) = table.add(batches).execute().await {
                self.logger.log(
                    "AGENT",
                    &format!("Warning: Failed to save to memory: {}", e),
                );
            } else {
                self.logger
                    .log("AGENT", "Conversation saved to LanceDB memory.");
            }
        }

        event_callback(json!({"type": "agent_message_done"}));
        Ok(())
    }
}
