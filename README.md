# OpenAether

OpenAether is a high-performance, intelligent desktop agent framework, purpose-built for **Arch Linux** and **Hyprland**. It features a 100% native Rust core for maximum speed, security, and deep system integration.

---

## 🏗️ Architecture

OpenAether is designed for low-latency autonomy and privacy:

- **Core (Rust/Tauri)**: The high-performance backbone. Manages window context, system IPC, and the **Rig Agent** framework for native tool execution.
- **Native Skills (Rust)**: All tools are implemented directly in Rust, eliminating the overhead of external bridges and ensuring instant responsiveness.
- **Interface (React)**: A modern UI built with React 19, Vite, and TypeScript, featuring transparent windows, glassmorphism aesthetics, and real-time streaming.
- **Memory (LanceDB)**: Persistent local vector memory (RAG), allowing the AI to recall previous conversations and system facts with low overhead.

---

## 🚀 Key Features

- **Blazing Fast Native Core**: Pure Rust implementation with zero Python overhead.
- **System-Native Intelligence**: Deep integration with `hyprctl`, allowing the AI to see your open windows, workspaces, and system state.
- **Tool-Augmented Reasoning**: Powered by the Rig framework, enabling multi-step tool calls for complex tasks.
- **Web Search (SearXNG)**: Privacy-focused web search integration with automatic Docker management.
- **Persistent Memory**: Self-healing RAG system that purges malformed data and persists facts in a local vector database.
- **Desktop Automation**: High-speed skills for controlling volume, notifications, workspaces, and window management.

---

## 🧰 Available Skills (Native Rust)

OpenAether features a comprehensive suite of native tools:

- **Web**: `web_search`, `fetch_url`, `open_website`, `scan_network`, `get_wifi_info`
- **System**: `run_command`, `install_software`, `get_system_info`, `kill_process`, `get_software_version`
- **Desktop (Hyprland)**: `get_workspaces`, `switch_workspace`, `get_windows`, `move_window_to_workspace`
- **Automation**: `click`, `type_text`, `set_volume`, `send_notification`, `play_audio`
- **Files**: `read_file`, `write_file`, `edit_file`, `list_directory`, `search_files`
- **Agent Utilities**: `remember`, `recall`, `set_timer`, `schedule_task`, `translate`

---

## 🛠️ Getting Started

### 📋 Prerequisites

Ensure you have the following installed on your Arch Linux system:

- **Ollama**: For local LLM inference.
- **Docker**: For running SearXNG (search engine).
- **Rust/Cargo**: To build the Tauri backend.
- **Node.js & npm**: For the frontend.

## 📦 Installation

Choose one of the following methods to install OpenAether on Arch Linux:

### Option 1: Native Build (Recommended)
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

### Option 2: AUR (Arch User Repository)
If you use an AUR helper like `yay` or `paru`, you can install it directly:
```bash
yay -S openaether
```

### 🏃 Running OpenAether
For development:
```bash
npm run dev
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

- `backend/`: Rust source (Tauri commands, Rig Agent, Native Skills, Logger)
- `frontend/`: React 19 + TypeScript UI source
- `backend/src/main/preskills/`: Location of all native Rust tool implementations.
- `package.json`: Main orchestration scripts

---

## 📜 License

This project is licensed under the **Apache License 2.0**. See the `LICENSE` file for details.
