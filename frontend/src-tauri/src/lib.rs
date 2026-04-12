// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn log_message(message: String) {
    println!("{}", message);
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // FIX: NVIDIA + Wayland + WebKitGTK compatibility workarounds
    // Prevents "Error 71 (Protocol error) dispatching to Wayland display"
    std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, log_message])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
