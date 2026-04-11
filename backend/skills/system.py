import os
import shutil
import subprocess
import re
import shlex
import json
import time
import asyncio
import platform
import psutil
import random
from datetime import datetime
from registry import registry
from logger import global_logger
from shell_manager import global_shell

# --- APPS & BINARIES ---
async def _get_window_hashes():
    try:
        from hypr_env import HYPRLAND_ENV
        out = subprocess.check_output(["hyprctl", "clients", "-j"], text=True, env=HYPRLAND_ENV)
        return {c.get("address") for c in json.loads(out) if c.get("address")}
    except: return set()

@registry.register(
    "open_app",
    "Open an application by its command name. Handles GUI/CLI and verifies a window appears.",
    {
        "type": "object",
        "properties": {
            "command": {"type": "string", "description": "App or command name"},
            "workspace": {"type": "integer", "description": "Optional: target Hyprland workspace"}
        },
        "required": ["command"]
    }
)
async def open_app(command, workspace=None):
    initial_windows = await _get_window_hashes()
    args = shlex.split(command.strip())
    args = [os.path.expanduser(a) for a in args]
    base_cmd = args[0].lower()
    full_cmd = shlex.join(args)

    if base_cmd in {"nvim", "vim", "htop", "btop", "python", "bash"} and "kitty" not in full_cmd:
        full_cmd = f"kitty -e {full_cmd}"
        base_cmd = "kitty"

    from hypr_env import HYPRLAND_ENV
    subprocess.Popen(shlex.split(full_cmd), start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=HYPRLAND_ENV)
    
    for _ in range(30):
        await asyncio.sleep(0.5)
        new_windows = (await _get_window_hashes()) - initial_windows
        if new_windows:
            addr = list(new_windows)[0]
            if workspace:
                subprocess.run(["hyprctl", "dispatch", "movetoworkspacesilent", f"{workspace},address:{addr}"], env=HYPRLAND_ENV)
            return {"status": "success", "message": f"Launched {base_cmd}."}
    return {"status": "error", "message": "App started but no window appeared."}

@registry.register("get_installed_apps", "List common apps and binaries installed on the system.", {})
def get_installed_apps_index():
    """Live index for AI discovery."""
    apps = set(["python", "node", "git", "docker", "pacman", "yay"])
    paths = ["/usr/share/applications", os.path.expanduser("~/.local/share/applications")]
    for d in paths:
        if os.path.exists(d):
            for f in os.listdir(d):
                if f.endswith(".desktop"):
                    apps.add(f.replace(".desktop", "").split(".")[-1].lower())
    return sorted(list(apps))

# --- FILESYSTEM ---
BACKUP_DIR = os.path.expanduser("~/.cache/openaether/backups")

def _create_backup(path: str):
    """Create a backup of an existing file before it is overwritten."""
    if not os.path.exists(path) or os.path.isdir(path):
        return
    os.makedirs(BACKUP_DIR, exist_ok=True)
    ts = datetime.now().strftime("%Y%m%d_%H%M%S")
    rnd = random.randint(100, 999)
    name = os.path.basename(path)
    bak_name = f"{name}.bak_{ts}_{rnd}"
    bak_path = os.path.join(BACKUP_DIR, bak_name)
    shutil.copy2(path, bak_path)
    return bak_path

def _normalize_path(path: str):
    """Path Redirection: Redirect hidden dotfiles to standard versions if standard version exists."""
    p = os.path.expanduser(path)
    if not os.path.isabs(p):
        p = os.path.join(os.getcwd(), p)
    
    filename = os.path.basename(p)
    dirname = os.path.dirname(p)
    
    # If it's a dotfile (hallucination risk)
    if filename.startswith(".") and len(filename) > 1:
        clean_name = filename[1:] # e.g. .notiz.md -> notiz.md
        clean_path = os.path.join(dirname, clean_name)
        
        # If the clean version exists, redirect and warn
        if os.path.exists(clean_path):
            warning = f"PATH_REDIRECT: redirected from '{filename}' to '{clean_name}' (standard version)."
            return clean_path, warning
            
    return p, None

@registry.register(
    "read_file", 
    "Read file content as text. NEVER use this on binary executables (ELFs) or files in /usr/bin or /usr/lib. Use 'get_software_version' for software inspection.", 
    {"type":"object", "properties":{"path":{"type":"string"}}, "required":["path"]}
)
def read_file(path: str):
    # Hallucination Guard
    if "/home/user" in path:
        actual_user = os.getenv("USER") or "current user"
        return {"status": "error", "message": f"PATH_ERROR: '/home/user' is a placeholder. Use '/home/{actual_user}' or '~/' instead."}
    
    try:
        p, warning = _normalize_path(path)
        
        # Binary Guard: Refuse to read if it's in a known binary dir or has binary extension
        BINARY_EXTS = {".exe", ".so", ".bin", ".o", ".a", ".pyc", ".node"}
        BINARY_DIRS = {"/usr/bin", "/bin", "/usr/sbin", "/sbin", "/usr/lib", "/lib"}
        
        if any(p.endswith(ext) for ext in BINARY_EXTS) or any(p.startswith(d) for d in BINARY_DIRS):
            # Check if it's actually an ELF or other binary (simple check)
            with open(p, "rb") as f:
                header = f.read(4)
                if b"\x7fELF" in header or b"MZ" in header:
                    return {"status": "error", "message": f"BINARY_ERROR: Content at '{path}' appears to be a binary executable. Use 'get_software_version' or 'run_command' (e.g. strings, ldd) to inspect it instead."}

        with open(p, "r", errors="replace") as f:
            # Limit read size for safety
            content = f.read(1_000_000) # Max 1MB
            res = {"status": "success", "content": content}
            if warning: res["message"] = warning
            return res
    except Exception as e: return {"status": "error", "message": str(e)}

@registry.register("write_file", "Write content to a file. Parameters: 'path' and 'content' ONLY. Do NOT pass translation arguments.", {"type":"object", "properties":{"path":{"type":"string"}, "content":{"type":"string"}}, "required":["path","content"]})
def write_file(path: str, content: str):
    # Hallucination Guard
    if "/home/user" in path:
        actual_user = os.getenv("USER") or "current user"
        return {"status": "error", "message": f"PATH_ERROR: '/home/user' is a placeholder. Use '/home/{actual_user}' or '~/' instead."}

    try:
        p, warning = _normalize_path(path)
        
        # Collision Protection & Automatic Backup
        if os.path.exists(p):
            # If the file exists, we ALWAYS backup
            bak = _create_backup(p)
            backup_msg = f" (Backup created: {os.path.basename(bak)})" if bak else ""
            
            # If the user's task was to 'create' a NEW file but it exists, we append a number
            # We detect this if the assistant didn't 'read' the file first in history (simulated by logic)
            # For now, let's just enforce the backup.
            
        os.makedirs(os.path.dirname(p) or ".", exist_ok=True)
        with open(p, "w") as f: f.write(content)
        res = {"status": "success"}
        if warning: res["message"] = warning + (backup_msg if 'backup_msg' in locals() else "")
        elif 'backup_msg' in locals(): res["message"] = backup_msg.strip()
        return res
    except Exception as e: return {"status": "error", "message": str(e)}

@registry.register("edit_file", "Safely edit an existing file using pattern replacement. PREFERRED over write_file for updates.", {"type":"object", "properties":{"path":{"type":"string"}, "old_text":{"type":"string"}, "new_text":{"type":"string"}}, "required":["path","old_text","new_text"]})
def edit_file(path: str, old_text: str, new_text: str):
    try:
        p, warning = _normalize_path(path)
        if not os.path.exists(p):
            return {"status": "error", "message": f"File {path} does not exist. Use write_file to create new files."}
        
        with open(p, "r") as f: content = f.read()
        
        if old_text not in content:
            return {"status": "error", "message": "The exact 'old_text' was not found in the file. Check the content and try again."}
        
        # Create tactical backup
        bak = _create_backup(p)
        new_content = content.replace(old_text, new_text)
        
        with open(p, "w") as f: f.write(new_content)
        
        res = {"status": "success", "message": f"Edit applied successfully. Backup created: {os.path.basename(bak)}"}
        if warning: res["message"] += f" | {warning}"
        return res
    except Exception as e: return {"status": "error", "message": str(e)}

@registry.register("list_directory", "List directory contents.", {"type":"object", "properties":{"path":{"type":"string"}}, "required":["path"]})
def list_directory(path: str):
    try:
        p = os.path.expanduser(path)
        return {"status": "success", "entries": [e.name for e in os.scandir(p)]}
    except Exception as e: return {"status": "error", "message": str(e)}

@registry.register("search_files", "Search files recursively by name pattern.", {"type":"object", "properties":{"query":{"type":"string"}, "path":{"type":"string"}}, "required":["query"]})
async def search_files(query: str, path: str = "~"):
    p = os.path.expanduser(path)
    cmd = f"find {p} -maxdepth 3 -iname '*{query}*' 2>/dev/null | head -n 20"
    res = await global_shell.execute(cmd, timeout=5)
    return {"status": "success", "results": res.get("output", "").strip().split("\n")}

@registry.register("delete_path", "Delete a file or directory. USE THIS instead of rm.", {"type":"object", "properties":{"path":{"type":"string"}}, "required":["path"]})
def delete_path(path: str):
    try:
        p = os.path.expanduser(path)
        if os.path.isdir(p):
            shutil.rmtree(p)
        else:
            os.remove(p)
        return {"status": "success", "message": f"Deleted {path}"}
    except Exception as e: return {"status": "error", "message": str(e)}

@registry.register("move_path", "Move or rename a file or directory.", {"type":"object", "properties":{"source":{"type":"string"}, "destination":{"type":"string"}}, "required":["source", "destination"]})
def move_path(source: str, destination: str):
    try:
        s = os.path.expanduser(source)
        d = os.path.expanduser(destination)
        shutil.move(s, d)
        return {"status": "success", "message": f"Moved {source} to {destination}"}
    except Exception as e: return {"status": "error", "message": str(e)}

# --- SYSTEM & HARDWARE ---
@registry.register("get_system_info", "Get CPU, RAM, and GPU information.", {})
def get_system_info():
    mem = psutil.virtual_memory()
    info = {
        "os": platform.platform(),
        "cpu": f"{psutil.cpu_count()} cores",
        "ram": f"{round(mem.total / (1024**3), 1)}GB",
    }
    try:
        res = subprocess.run(["nvidia-smi", "--query-gpu=name", "--format=csv,noheader"], capture_output=True, text=True)
        if res.returncode == 0: info["gpu"] = res.stdout.strip()
    except: pass
    return info

@registry.register("run_command", "Run a generic shell command. Use with caution.", {"type":"object", "properties":{"command":{"type":"string"}}, "required":["command"]})
async def run_command(command):
    if any(q in command for q in ["pacman", "systemctl", "yay"]) and not command.startswith("pkexec"):
        command = f"pkexec {command}"
    res = await global_shell.execute(command)
    return res

@registry.register("install_software", "Install a package via Pacman or Yay.", {"type":"object", "properties":{"name":{"type":"string"}}, "required":["name"]})
async def install_software(name):
    cmd = f"pkexec pacman -S --noconfirm {name}" if shutil.which("pacman") else f"yay -S --noconfirm {name}"
    return await run_command(cmd)

@registry.register("kill_process", "Kill a process by name or PID.", {"type":"object", "properties":{"target":{"type":"string"}}, "required":["target"]})
async def kill_process(target: str):
    cmd = f"kill -9 {target}" if target.isdigit() else f"pkill {target}"
    return await run_command(cmd)

@registry.register(
    "get_software_version",
    "Get version AND detailed installation info (date, reason, source). MANDATORY for locally installed software questions on Arch Linux (Pacman). Use this BEFORE web_search.",
    {"type": "object", "properties": {"name": {"type": "string", "description": "Package name or binary"}}, "required": ["name"]}
)
async def get_software_version(name: str):
    # 1. Try Pacman (Reliable for system packages)
    if shutil.which("pacman"):
        # Try -Qi (Information) first for more detail
        res = subprocess.run(["pacman", "-Qi", name], capture_output=True, text=True)
        if res.returncode == 0:
            lines = res.stdout.strip().split("\n")
            info = {}
            for line in lines:
                if ":" in line:
                    k, v = line.split(":", 1)
                    info[k.strip()] = v.strip()
            
            return {
                "status": "success", 
                "method": "pacman", 
                "version": info.get("Version"),
                "install_date": info.get("Install Date"),
                "install_reason": info.get("Install Reason"),
                "packager": info.get("Packager"),
                "source": "official_repo" if "Extra" in info.get("Repository", "") or "Core" in info.get("Repository", "") else "AUR/External"
            }
        
        # Fallback to -Q (simple query)
        res = subprocess.run(["pacman", "-Q", name], capture_output=True, text=True)
        if res.returncode == 0:
            parts = res.stdout.strip().split()
            version = parts[1] if len(parts) > 1 else parts[0]
            return {"status": "success", "method": "pacman_simple", "version": version}

    # 2. Try --version (Common for CLI tools)
    binary = shutil.which(name)
    if binary:
        try:
            # Try common flags
            for flag in ["--version", "-v", "version"]:
                res = subprocess.run([binary, flag], capture_output=True, text=True, timeout=2)
                if res.returncode == 0 and res.stdout.strip():
                    version = res.stdout.strip().split("\n")[0]
                    return {"status": "success", "method": "binary_flag", "output": version}
        except:
            pass

    return {"status": "error", "message": f"Could not determine version for '{name}'. Software might not be installed or doesn't support version flags."}
