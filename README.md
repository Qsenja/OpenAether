# OpenAether

OpenAether is an intelligent desktop agent framework tailored for **Arch Linux** and **Hyprland**. It leverages AI models (via Ollama) to assist with system tasks, research, and navigation in a native Linux environment.

## 🚀 Features

- **State-of-the-art Intelligence**: Powered by Qwen 2.5 14B (via Ollama) and integrated with a high-performance reasoning engine.
- **Native Hyprland Integration**: Aware of your open windows and workspaces.
- **Agentic Tools**: Ability to run shell commands, manage files, search the web (via SearXNG), and interact with your system directly.
- **Electron UI**: A modern, responsive interface for interacting with your AI agent.
- **Fast Dispatch**: Aether Spark layer for instant responses and common task acceleration.

## 🛠️ Project Structure

- `backend/`: Python-based core logic, tool registry, and Ollama integration.
- `frontend/`: Electron-based user interface.
- `.agents/`: Local configuration for agentic behaviors.

## 📦 Getting Started

### Prerequisites

- **OS**: Arch Linux
- **Window Manager**: Hyprland
- **AI**: [Ollama](https://ollama.com/) (installed and running)
- **Search**: [SearXNG](https://github.com/searxng/searxng) (optional, supports auto-start via Docker)
- **Node.js & npm** (for the frontend)
- **Python 3.10+** (for the backend)

### 1. Backend Setup

1. Create a virtual environment:
   ```bash
   python -m venv venv
   source venv/bin/activate
   ```
2. Install dependencies:
   ```bash
   pip install -r backend/requirements.txt
   ```
3. Run the backend:
   ```bash
   python backend/main.py
   ```

### 2. Frontend Setup

1. Navigate to the frontend directory:
   ```bash
   cd frontend
   ```
2. Install Node dependencies:
   ```bash
   npm install
   ```
3. Start the application:
   ```bash
   npm start
   ```

## 📜 License

This project is licensed under the **Apache License 2.0**. See the `LICENSE` file for details.
