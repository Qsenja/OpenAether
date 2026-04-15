#![recursion_limit = "2048"]
#[path = "main/mod.rs"]
pub mod logic;

use tauri::{AppHandle, Emitter, State, Manager};
use serde_json::json;
use futures_util::StreamExt;

use crate::logic::AppState;
use crate::logic::ollama::Message;
use crate::logic::agent::Agent;
use crate::logic::docker::DockerManager;
use tokio_util::sync::CancellationToken;

#[tauri::command]
async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    content: String,
    history: Vec<Message>,
) -> Result<(), String> {
    let mut messages = history;
    messages.push(Message { role: "user".to_string(), content: content.clone() });

    state.logger.log("UI", &format!("User: {}", content));

    // NORMAL AGENT LOOP
    let base_path = std::env::current_dir().unwrap_or_default();
    let system_prompt_path = base_path.join("config/system_prompt.txt");
    let system_prompt = std::fs::read_to_string(&system_prompt_path).unwrap_or_else(|_| {
        state.logger.log("SYSTEM", "Warning: Could not read system_prompt.txt, using default.");
        "You are Fabel.".into()
    });

    // 1. Fetch available tools (All native now)
    let schemas = Vec::new();

    let (model, temperature, top_p, searxng_url) = {
        let s = state.settings.lock().unwrap();
        (s.ollama_model.clone(), s.temperature, s.top_p, s.searxng_url.clone())
    };

    let agent = Agent::new(
        state.ollama.clone(),
        state.memory.clone(),
        state.shell.clone(),
        state.logger.clone(),
        system_prompt,
        schemas,
        temperature,
        top_p,
        searxng_url,
    );

    // Create and store cancellation token
    let token = CancellationToken::new();
    {
        let mut t = state.cancel_token.lock().unwrap();
        *t = Some(token.clone());
    }

    let app_clone = app.clone();
    let res: anyhow::Result<()> = agent.process(&mut messages, &model, token.clone(), move |event| {
        let _ = app_clone.emit("backend-event", event);
    }).await;

    // Clear token after completion
    {
        let mut t = state.cancel_token.lock().unwrap();
        *t = None;
    }

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}#[tauri::command]
async fn stop_generation(state: State<'_, AppState>) -> Result<(), String> {
    state.logger.log("AGENT", "Stop requested by user.");
    let mut token_lock = state.cancel_token.lock().unwrap();
    if let Some(token) = token_lock.take() {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<crate::logic::settings::UserSettings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
fn update_settings(state: State<'_, AppState>, settings: crate::logic::settings::UserSettings) -> Result<(), String> {
    *state.settings.lock().unwrap() = settings.clone();
    
    // Apply log level change immediately
    state.logger.set_level(crate::logic::logger::LogLevel::from_i32(settings.log_level));
    
    state.settings_manager.save(&settings).map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
fn set_log_level(state: State<'_, AppState>, level: i32) {
    state.logger.set_level(crate::logic::logger::LogLevel::from_i32(level));
}

#[tauri::command]
async fn get_setup_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let searxng = DockerManager::is_searxng_up().await;
    let model = state.settings.lock().unwrap().ollama_model.clone();
    
    // Simple status check
    Ok(json!({
        "ollama": true,
        "models": {
            model: {"installed": true, "size": "N/A"}
        },
        "docker": true,
        "searxng": searxng,
        "forced": false
    }))
}

#[tauri::command]
async fn pull_model(app: AppHandle, state: State<'_, AppState>, model: String) -> Result<(), String> {
    let mut stream = state.ollama.pull_model(&model).await.map_err(|e: anyhow::Error| e.to_string())?;
    
    while let Some(res) = stream.next().await {
        if let Ok(chunk) = res {
            let _ = app.emit("backend-event", json!({
                "type": "pull_progress",
                "model": model,
                "status": chunk.status,
                "percent": (chunk.completed.unwrap_or(0) as f64 / chunk.total.unwrap_or(1) as f64) * 100.0
            }));
        }
    }
    
    Ok(())
}

#[tauri::command]
fn log_message(state: State<'_, AppState>, message: String) {
    state.logger.log("FE", &message);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // NVIDIA + Wayland + WebKitGTK compatibility workarounds
    std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    let base_path = std::env::current_dir().unwrap_or_default();
    let state = AppState::new();

    let _logic_path = base_path.join("../logic");
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            let handle = app.handle().clone();
            
            // Auto-start SearXNG in the background
            let base_path_clone = base_path.clone();
            tokio::spawn(async move {
                let state = handle.state::<AppState>();
                if let Err(e) = DockerManager::check_searxng(&base_path_clone).await {
                    state.logger.log("SYSTEM", &format!("Warning: SearXNG check failed: {}", e));
                } else {
                    state.logger.log("SYSTEM", "SearXNG is running and reachable.");
                }

                // Automated Memory Sanitization (Self-Healing)
                if let Err(e) = state.memory.sanitize_memory().await {
                    state.logger.log("SYSTEM", &format!("Warning: Memory sanitization failed: {}", e));
                }

                let (searxng_url, system_prompt) = {
                    let s = state.settings.lock().unwrap();
                    let prompt = std::fs::read_to_string(std::env::current_dir().unwrap_or_default().join("config/system_prompt.txt"))
                        .unwrap_or_else(|_| "You are Fabel.".into());
                    (s.searxng_url.clone(), prompt)
                };

                // Create a temporary agent instance to gather all definitions (All Native)
                let temp_agent = Agent::new(
                    state.ollama.clone(),
                    state.memory.clone(),
                    state.shell.clone(),
                    state.logger.clone(),
                    system_prompt,
                    Vec::new(),
                    0.7,
                    0.9,
                    searxng_url,
                );

                let _dummy_token = CancellationToken::new();

                let all_defs = temp_agent.get_all_tool_definitions().await;
                match state.memory.sync_tool_manual(all_defs).await {
                    Ok(_) => state.logger.log("MEMORY", "Internal tool manual synchronized successfully."),
                    Err(e) => state.logger.log("MEMORY", &format!("Warning: Tool manual sync failed: {}", e)),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send_message,
            stop_generation,
            get_settings,
            update_settings,
            get_setup_status,
            pull_model,
            log_message,
            set_log_level
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
