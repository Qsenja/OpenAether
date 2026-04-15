# OpenAether

OpenAether is a next-generation desktop intelligence platform, built for the **Linux ecosystem**. It provides a seamless, high-performance bridge between your local LLM and your OS, supporting a wide range of distributions and desktop environments.

---

## 🏗️ The Vision

OpenAether is a **universal system companion**. It's designed to understand and control your desktop regardless of your choice of Window Manager or Desktop Environment—all while running 100% locally and privately.

### ✨ Core Pillars
- **Native Performance**: 100% Pure Rust core for near-zero latency across all Linux distributions.
- **Universal Awareness**: Intelligent detection and support for GNOME, KDE, XFCE, Hyprland, Sway, i3, and more.
- **Privacy First**: Everything stays on your machine. Local inference via Ollama and local vector memory via LanceDB.
- **Distro Agnostic**: Built-in support for `pacman`, `apt`, `dnf`, `zypper`, and `apk`.

---

## 🚀 Key Features

- **Blazing Fast Native Core**: Engineered in Rust/Tauri for a premium, lightweight desktop integration.
- **Universal Desktop Control**: Unified tools for managing windows and workspaces across Wayland and X11.
- **Autonomous Reasoning**: An agent that doesn't just talk—it executes tasks, searches the web, and manages files.
- **Self-Healing Memory**: Persistent RAG (Retrieval-Augmented Generation) that learns from your interactions.
- **Ambient UI**: A stunning, glassmorphism-inspired interface crafted with React 19 and Vite.

---

## 🧰 Native Skill System

OpenAether launches with a powerful set of universal capabilities:

- **🌐 Web**: Real-time search (SearXNG), website interaction, and network analysis.
- **💻 Package Management**: Install and manage software across Arch, Debian, Fedora, openSUSE, and Alpine.
- **🪟 Desktop**: Intelligent control for Hyprland, Sway, i3, BSPWM, River, Awesome, GNOME, and KDE.
- **🛠️ Automation**: Universal keyboard/mouse simulation, volume control, and system notifications.
- **📂 File System**: Intelligent file reading, editing, and directory exploration.
- **🧠 Knowledge**: Local memory retrieval, task scheduling, and real-time translation.

---

## 🛠️ Getting Started

### 📋 Prerequisites

OpenAether is compatible with most modern Linux environments:

- **Ollama**: Local LLM host (e.g., Llama 3, Mistral, or Qwen).
- **Docker**: For running the private SearXNG search engine.
- **Rust & Node.js**: Standard build environment for the Tauri/React stack.

## 📦 Installation

### Option 1: Native Build (Recommended)
1. **Clone the Project**:
   ```bash
   git clone https://github.com/qsenja/OpenAether.git
   cd OpenAether
   ```
2. **Launch**:
   ```bash
   npm install
   npm run build  # For production build
   npm run dev    # For development mode
   ```

### Option 2: Distribution Packages
- **Arch Linux (AUR)**: `yay -S openaether`
- **Flatpak/AppImage**: Coming soon.

---

## 📂 Architecture at a Glance

- **Backend (`backend/`)**: High-performance Rust core and the Rig Agent orchestration engine.
- **Frontend (`frontend/`)**: Modern React 19 UI with TypeScript and Tailwind-ready components.
- **Native Skills (`backend/src/main/preskills/`)**: Multi-environment system abstractions.
- **Memory (`backend/lancedb/`)**: Local vector database for persistent agent recall.

---

## 📜 License

Licensed under the **Apache License 2.0**. Crafted with 🦀 and ⚡ for the Linux community.
