# OpenAether

OpenAether is a next-generation intelligent desktop agent framework, purpose-built for **Arch Linux** and **Hyprland**. It combines a high-performance Rust core with a flexible Python-based skill system to provide a seamless, native AI experience directly in your window manager.

---

## 🏗️ Architecture

OpenAether uses a hybrid architecture to balance performance, safety, and extensibility:

- **Core (Rust/Tauri)**: The heart of the system. Manages window context, system IPC, and the **Rig Agent** framework for orchestrated reasoning.
- **Logic Bridge (Python)**: A dynamic skill engine that hosts complex integrations. It registers tools via a Python-Rust bridge, allowing for easy skill extension.
- **Interface (React)**: A modern UI built with React 19, Vite, and TypeScript, featuring transparent windows and real-time streaming.
- **Memory (LanceDB)**: Persistent vector memory stored locally, allowing the AI to recall previous conversations and system facts.

---

## 🚀 Key Features

- **System-Native Intelligence**: Deep integration with `hyprctl`, allowing the AI to see your open windows, workspaces, and system state.
- **Tool-Augmented Reasoning**: Powered by the Rig framework, enabling multi-step tool calls for complex tasks.
- **Web Search (SearXNG)**: Privacy-focused web search integration with automatic Docker management.
- **Persistent Memory**: Conversation history and facts are embedded and stored in a local vector database.
- **Desktop Automation**: Skills for controlling volume, notifications, workspaces, and window management.

---

## 🧰 Available Skills

OpenAether comes with a wide range of built-in skills, including:

- **Web**: `web_search`, `fetch_url`, `open_website`, `scan_network`, `get_wifi_info`
- **System**: `run_command`, `install_software`, `get_system_info`, `kill_process`, `get_software_version`
- **Desktop (Hyprland)**: `get_workspaces`, `switch_workspace`, `get_windows`, `move_window_to_workspace`
- **Automation**: `click`, `type_text`, `set_volume`, `send_notification`, `play_audio`
- **Files**: `read_file`, `write_file`, `edit_file`, `list_directory`, `search_files`
- **Agent Utilities**: `remember`, `recall`, `set_timer`, `schedule_task`, `translate`, `run_python`

---

## 🛠️ Getting Started

### 📋 Prerequisites

Ensure you have the following installed on your Arch Linux system:

- **Ollama**: For local LLM inference.
- **Docker**: For running SearXNG (search engine).
- **Rust/Cargo**: To build the Tauri backend.
- **Node.js & npm**: For the frontend.
- **Python 3.10+**: For the logic bridge and skills.

## 📦 Installation

Choose one of the following methods to install OpenAether on Arch Linux:

### Option 1: AUR (Arch User Repository)
If you use an AUR helper like `yay` or `paru`, you can install it directly:
```bash
yay -S openaether
```

### Option 2: Pull and Compile
To build from source, follow these steps:
1. **Clone the Repository**:
   ```bash
   git clone https://github.com/qsenja/OpenAether.git
   cd OpenAether
   ```
2. **Build and Install**:
   ```bash
   npm install
   npm run build
   ```

### 🏃 Running OpenAether
For development:
```bash
npm run dev
```
If installed via AUR, simply run:
```bash
openaether
```

---

## 🔍 Diagnostics & Logs

If you encounter issues, logs are stored in the following locations:

- **Application Logs**: `~/.local/share/openaether/logs/`
- **Config Directory**: `~/.config/openaether/`

To check if SearXNG is running, use:
```bash
docker ps | grep searxng
```

---

## 📂 Project Structure

- `backend/`: Rust source (Tauri commands, Rig Agent, Logger)
- `logic/`: Python Skills & Bridge interface
- `frontend/`: React 19 + TypeScript UI source
- `package.json`: Main orchestration scripts

---

## 📜 License

This project is licensed under the **Apache License 2.0**. See the `LICENSE` file for details.
