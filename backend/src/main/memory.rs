use rig::prelude::*;
use rig::client::Nothing;
use rig_lancedb::{LanceDbVectorIndex, SearchParams};
use lancedb::connect;
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use arrow_array::RecordBatchIterator;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use rig::Embed;

#[derive(Serialize, Deserialize, Debug, Clone, Embed)]
pub struct TextRecord {
    pub id: String,
    #[embed]
    pub text: String,
}

pub struct MemoryManager {
    ollama_client: rig::providers::ollama::Client,
    db_path: String,
    pub table: tokio::sync::OnceCell<lancedb::Table>,
    pub tool_table: tokio::sync::OnceCell<lancedb::Table>,
    pub embedding_model: tokio::sync::OnceCell<rig::providers::ollama::EmbeddingModel>,
}

impl MemoryManager {
    pub fn new() -> Self {
        let proj_dirs = ProjectDirs::from("com", "openaether", "openaether")
            .unwrap_or_else(|| panic!("Could not determine project directories"));
        let data_dir = proj_dirs.data_dir();
        std::fs::create_dir_all(data_dir).ok();
        
        let db_path = data_dir.join("memory.lancedb").to_string_lossy().to_string();
        
        Self {
            ollama_client: rig::providers::ollama::Client::builder()
                .base_url("http://localhost:11434")
                .api_key(Nothing)
                .build()
                .expect("Failed to create Ollama client"),
            db_path,
            table: tokio::sync::OnceCell::new(),
            tool_table: tokio::sync::OnceCell::new(),
            embedding_model: tokio::sync::OnceCell::new(),
        }
    }

    pub async fn sync_tool_manual(&self, tool_definitions: Vec<serde_json::Value>) -> Result<()> {
        let embedding_model = self.ollama_client.embedding_model("nomic-embed-text");
        let db = connect(&self.db_path).execute().await
            .map_err(|e| anyhow!("Failed to connect to LanceDB for sync: {}", e))?;

        let table_name = "tool_knowledge";
        
        // Use a schema that identifies tools by name (id)
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new("vector", DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                768
            ), false),
        ]));

        let table = match db.open_table(table_name).execute().await {
            Ok(t) => t,
            Err(_) => {
                db.create_empty_table(table_name, schema.clone())
                    .execute().await
                    .map_err(|e| anyhow!("Failed to create tool_knowledge table: {}", e))?
            }
        };

        let mut records_to_add = Vec::new();

        for tool in tool_definitions {
            let name = tool["name"].as_str().or(tool["function"]["name"].as_str()).unwrap_or("unknown");
            let desc = tool["description"].as_str().or(tool["function"]["description"].as_str()).unwrap_or("");
            
            // Check if tool already exists in the manual
            // Note: Simplistic check for now, Rig-LanceDB 0.23 doesn't have an 'exists' helper easily available without a scan
            // But for a tool manual (small), we can just replace or append
            let record = TextRecord {
                id: name.to_string(),
                text: format!("Tool: {}\nDescription: {}", name, desc),
            };
            records_to_add.push(record);
        }

        if !records_to_add.is_empty() {
             let batch = rig::embeddings::EmbeddingsBuilder::new(embedding_model.clone())
                .documents(records_to_add)?
                .build()
                .await
                .map_err(|e| anyhow!("Failed to embed tool records: {}", e))?;

            // Convert to RecordBatch and overwrite/add
            // For now, simplify and just append - we can add uniqueness checks in a later refactor
            use arrow_array::{StringArray, FixedSizeListArray, RecordBatch as ArrowBatch};
            use arrow_array::types::Float32Type;
            use arrow_array::ArrayRef;

            let ids = StringArray::from_iter_values(batch.iter().map(|(r, _)| &r.id));
            let texts = StringArray::from_iter_values(batch.iter().map(|(r, _)| &r.text));
            let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                batch.into_iter().map(|(_, embs)| {
                    Some(embs.first().vec.into_iter().map(|v| Some(v as f32)).collect::<Vec<_>>())
                }).collect::<Vec<_>>(),
                768
            );

            let record_batch = ArrowBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(ids) as ArrayRef,
                    Arc::new(texts) as ArrayRef,
                    Arc::new(vectors) as ArrayRef,
                ],
            )?;

            let batches = RecordBatchIterator::new(vec![Ok(record_batch)], schema.clone());
            
            // Idempotent sync: Drop and recreate the tool knowledge table
            // This ensures a fresh manual on every launch without privacy issues
            let _ = db.drop_table(table_name, &[]).await;
            db.create_table(table_name, batches)
                .execute()
                .await?;
        }

        let _ = self.tool_table.set(table);
        Ok(())
    }

    pub async fn get_tool_index(&self) -> Result<LanceDbVectorIndex<rig::providers::ollama::EmbeddingModel>> {
        let embedding_model = self.ollama_client.embedding_model("nomic-embed-text");
        let db = connect(&self.db_path).execute().await?;
        let table = db.open_table("tool_knowledge").execute().await?;

        LanceDbVectorIndex::new(
            table,
            embedding_model,
            "id",
            SearchParams::default()
        ).await
        .map_err(|e| anyhow!("Failed to initialize tool index: {}", e))
    }

    pub async fn get_index(&self) -> Result<LanceDbVectorIndex<rig::providers::ollama::EmbeddingModel>> {
        // 1. Setup embedding model
        let embedding_model = self.ollama_client.embedding_model("nomic-embed-text");

        // 2. Connect to LanceDB
        let db = connect(&self.db_path).execute().await
            .map_err(|e| anyhow!("Failed to connect to LanceDB: {}", e))?;

        // 3. Open or create table
        let table_name = "conversations";
        let table = match db.open_table(table_name).execute().await {
            Ok(t) => t,
            Err(_) => {
                // Table doesn't exist, create it with a basic schema
                // rig-lancedb expects a string ID field, a data field (text), and a vector field
                let schema = Arc::new(Schema::new(vec![
                    Field::new("id", DataType::Utf8, false),
                    Field::new("text", DataType::Utf8, false),
                    Field::new("vector", DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float32, true)),
                        768 // dimensions for nomic-embed-text
                    ), false),
                ]));

                db.create_empty_table(table_name, schema)
                    .execute().await
                    .map_err(|e| anyhow!("Failed to create LanceDB table: {}", e))?
            }
        };

        // 4. Initialize the Rig LanceDB Integration
        let _ = self.embedding_model.set(embedding_model.clone());
        let _ = self.table.set(table.clone());

        LanceDbVectorIndex::new(
            table,
            embedding_model,
            "id", // The ID column
            SearchParams::default()
        ).await
        .map_err(|e| anyhow!("Failed to initialize rig-lancedb index: {}", e))
    }

    /// Automatically scans the conversation history for malformed tool call leaks
    /// (raw JSON blocks) and purges them to prevent model regression.
    pub async fn sanitize_memory(&self) -> Result<()> {
        let db = connect(&self.db_path).execute().await
            .map_err(|e| anyhow!("Failed to connect to LanceDB for sanitization: {}", e))?;
            
        let table_name = "conversations";
        if let Ok(table) = db.open_table(table_name).execute().await {
            // Aggressive Pattern Match: Search the entire string for JSON tool call keys
            let filter = "(text LIKE '%\"name\":%\"arguments\":%') OR (text LIKE '%\"name\":%\"args\":%')";
            
            match table.delete(filter).await {
                Ok(_) => println!("[MEMORY] Successfully purged malformed records from conversation history."),
                Err(e) => println!("[MEMORY] Note: No records to sanitize or delete error: {}", e),
            }
        }
        Ok(())
    }
}
