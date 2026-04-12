#![recursion_limit = "2048"]
#[path = "main/mod.rs"]
pub mod logic;

use tauri::{AppHandle, Emitter, State};
use serde_json::json;
use futures_util::StreamExt;

use crate::logic::AppState;
use crate::logic::ollama::Message;
use crate::logic::agent::Agent;
use crate::logic::docker::DockerManager;

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
    let system_prompt_path = base_path.join("../../backend/config/system_prompt.txt");
    let system_prompt = std::fs::read_to_string(&system_prompt_path).unwrap_or_else(|_| {
        state.logger.log("SYSTEM", "Warning: Could not read system_prompt.txt, using default.");
        "You are Aether Core.".into()
    });

    // 1. Fetch available tools from bridge
    let schemas = match state.bridge.get_schemas() {
        Ok(s) => s,
        Err(e) => {
            state.logger.log("BRIDGE", &format!("Failed to fetch schemas: {}", e));
            Vec::new()
        }
    };

    let agent = Agent::new(
        state.ollama.clone(),
        state.bridge.clone(),
        state.memory.clone(),
        state.logger.clone(),
        system_prompt,
        schemas,
    );

    let model = {
        let s = state.settings.lock().unwrap();
        s.ollama_model.clone()
    };

    let app_clone = app.clone();
    let res: anyhow::Result<()> = agent.process(&mut messages, &model, move |event| {
        let _ = app_clone.emit("backend-event", event);
    }).await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
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

    // Use absolute, canonicalized paths to avoid Python RuntimeWarnings (sys.prefix mismatch)
    let base_path = std::env::current_dir().unwrap_or_default();
    let python_path = base_path.join("../../backend/venv/bin/python")
        .canonicalize()
        .unwrap_or_else(|_| base_path.join("../../backend/venv/bin/python"));
        
    let worker_script = base_path.join("../../backend/skill_worker.py")
        .canonicalize()
        .unwrap_or_else(|_| base_path.join("../../backend/skill_worker.py"));

    let state = AppState::new(python_path, worker_script);
    let _ = state.bridge.start(); // Pre-start bridge

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            send_message,
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
