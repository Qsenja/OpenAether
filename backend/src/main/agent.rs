use crate::logic::bridge::PythonBridge;
use crate::logic::logger::{LogLevel, Logger};
use crate::logic::memory::{MemoryManager, TextRecord};
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use rig::{
    client::Nothing,
    completion::{Message as RigMessage, ToolDefinition},
    prelude::*,
    streaming::StreamingChat,
    tool::{ToolDyn, ToolError},
};
use rig::wasm_compat::WasmBoxedFuture;
use serde_json::json;
use std::sync::Arc;

use arrow_array::{
    types::Float32Type, ArrayRef, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Fields, Schema};

pub struct Agent {
    ollama_client: rig::providers::ollama::Client,
    bridge: Arc<PythonBridge>,
    memory_manager: Arc<MemoryManager>,
    logger: Arc<Logger>,
    system_prompt: String,
    available_tools: Vec<serde_json::Value>,
}

struct PythonTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
    bridge: Arc<PythonBridge>,
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

            match bridge.execute(&name, json_args) {
                Ok(res) => Ok(res.to_string()),
                Err(e) => Err(ToolError::ToolCallError(e.to_string().into())),
            }
        })
    }
}

impl Agent {
    pub fn new(
        _ollama: Arc<crate::logic::ollama::OllamaClient>,
        bridge: Arc<PythonBridge>,
        memory_manager: Arc<MemoryManager>,
        logger: Arc<Logger>,
        system_prompt: String,
        available_tools: Vec<serde_json::Value>,
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
            logger,
            system_prompt,
            available_tools,
        }
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
                }));
            }
        }

        // 2. Window Context Injection
        let mut window_context = String::new();
        if let Ok(res) = self.bridge.execute("get_windows", json!({})) {
            window_context = format!(
                "\n\n[ACTUAL OPEN WINDOWS]: {}",
                serde_json::to_string(&res).unwrap_or_default()
            );
        }

        // 3. Setup Memory Context
        let _index = match self.memory_manager.get_index().await {
            Ok(idx) => Some(idx),
            Err(e) => {
                self.logger.log(
                    "AGENT",
                    &format!("Warning: Could not initialize memory index: {}", e),
                );
                None
            }
        };

        // 4. Build the Rig Agent
        let rig_agent = self
            .ollama_client
            .agent(model)
            .preamble(&(self.system_prompt.clone() + &window_context))
            .default_max_turns(10)
            .tools(rig_tools)
            .build();

        // 5. Run the Agent with history support
        let user_query = messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Convert history to Rig messages (excluding the last message which is the current query)
        let history = messages
            .iter()
            .take(messages.len().saturating_sub(1))
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
                    rig::agent::MultiTurnStreamItem::StreamAssistantItem(
                        rig::streaming::StreamedAssistantContent::Text(text),
                    ) => {
                        let text_val = text.text;
                        self.logger.log_at(
                            LogLevel::Trace,
                            "AGENT",
                            &format!("Content: {}", text_val),
                        );
                        assistant_content.push_str(&text_val);
                        event_callback(json!({"type": "agent_message", "content": text_val}));
                    }
                    _ => {}
                },
                Err(e) => return Err(anyhow!("Stream error: {}", e)),
            }
        }

        // Update local history
        messages.push(crate::logic::ollama::Message {
            role: "assistant".to_string(),
            content: assistant_content.clone(),
        });

        // 6. Save to memory if available
        if self.memory_manager.table.get().is_some() {
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
