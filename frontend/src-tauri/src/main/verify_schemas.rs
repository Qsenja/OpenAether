use std::path::PathBuf;
mod bridge;

fn main() {
    let python_path = PathBuf::from("../../backend/venv/bin/python");
    let worker_script = PathBuf::from("../../backend/skill_worker.py");
    let bridge = bridge::PythonBridge::new(python_path, worker_script);
    bridge.start().unwrap();
    let schemas = bridge.get_schemas().unwrap();
    println!("{}", serde_json::to_string_pretty(&schemas).unwrap());
}
