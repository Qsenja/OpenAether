use rig::prelude::*;
use rig::client::Nothing;
use rig_lancedb::{LanceDbVectorIndex, SearchParams};
use lancedb::connect;
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
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
                .base_url("http://localhost:11434/v1")
                .api_key(Nothing)
                .build()
                .expect("Failed to create Ollama client"),
            db_path,
            table: tokio::sync::OnceCell::new(),
            embedding_model: tokio::sync::OnceCell::new(),
        }
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
}
