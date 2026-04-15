use serde::{Deserialize, Serialize};
use reqwest::Client;
use futures_util::StreamExt;
use anyhow::{Result, anyhow};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    pub options: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatResponse {
    pub message: Option<Message>,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PullRequest {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PullResponse {
    pub status: String,
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
}

pub struct OllamaClient {
    client: Client,
    base_url: String,
}

impl OllamaClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn chat_stream(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: Option<serde_json::Value>,
    ) -> Result<impl StreamExt<Item = Result<ChatResponse>>> {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatRequest {
            model: model.to_string(),
            messages,
            stream: true,
            options,
        };

        let response = self.client.post(&url).json(&body).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow!("Ollama error: {}", response.status()));
        }

        let stream = response.bytes_stream().map(|item| {
            match item {
                Ok(bytes) => {
                    let res: ChatResponse = serde_json::from_slice(&bytes)?;
                    Ok(res)
                }
                Err(e) => Err(anyhow!(e)),
            }
        });

        Ok(stream)
    }

    pub async fn chat(
        &self,
        model: &str,
        messages: Vec<Message>,
    ) -> Result<ChatResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
            options: None,
        };

        let response = self.client.post(&url).json(&body).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow!("Ollama error: {}", response.status()));
        }

        let res: ChatResponse = response.json().await?;
        Ok(res)
    }

    pub async fn pull_model(
        &self,
        name: &str,
    ) -> Result<impl StreamExt<Item = Result<PullResponse>>> {
        let url = format!("{}/api/pull", self.base_url);
        let body = PullRequest {
            name: name.to_string(),
        };

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Ollama error: {}", response.status()));
        }

        let stream = response.bytes_stream().map(|item| {
            match item {
                Ok(bytes) => {
                    let res: PullResponse = serde_json::from_slice(&bytes)?;
                    Ok(res)
                }
                Err(e) => Err(anyhow!(e)),
            }
        });

        Ok(stream)
    }
}
