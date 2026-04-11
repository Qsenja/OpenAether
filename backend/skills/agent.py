import asyncio
import json
import os
import shutil
import subprocess
import re
import io
import contextlib
from datetime import datetime
import ollama
from registry import registry
from logger import global_logger
from shell_manager import global_shell

# --- CONFIGURATION ---
MEMORY_PATH = os.path.expanduser("~/.config/openetude/memory.json")
TRANSLATE_MODEL = "translategemma:4b"

# --- MEMORY LOGIC ---
def _load_memory() -> dict:
    """Load the persistent memory store."""
    try:
        if os.path.exists(MEMORY_PATH):
            with open(MEMORY_PATH, "r", encoding="utf-8") as f:
                return json.load(f)
    except:
        pass
    return {}

def _save_memory(data: dict):
    """Save the persistent memory store."""
    os.makedirs(os.path.dirname(MEMORY_PATH), exist_ok=True)
    with open(MEMORY_PATH, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)

@registry.register(
    "remember",
    "Store a persistent note in memory. Does NOT write to user files. Use ONLY for preferences and facts.",
    {
        "type": "object",
        "properties": {
            "key":   {"type": "string", "description": "The name/identifier for this note (e.g. 'user_name', 'pending_plan')"},
            "value": {"type": "string", "description": "The value to store."}
        },
        "required": ["key", "value"]
    }
)
async def remember(key: str, value: str):
    mem = _load_memory()
    mem[key] = {
        "value": value,
        "timestamp": datetime.now().isoformat()
    }
    _save_memory(mem)
    return {"status": "success", "message": f"Stored memory for key '{key}'."}

@registry.register(
    "recall",
    "Recall a previously remembered note by key. Omit key to see ALL stored memories.",
    {
        "type": "object",
        "properties": {
            "key": {"type": "string", "description": "The key to look up. Leave empty to retrieve all stored memories."}
        }
    }
)
async def recall(key: str = None):
    mem = _load_memory()
    if key:
        entry = mem.get(key)
        if entry:
            return {"status": "success", "key": key, "value": entry["value"], "saved_at": entry.get("timestamp")}
        return {"status": "error", "message": f"No memory found for key '{key}'. Use recall() without a key to see all."}
    if not mem:
        return {"status": "success", "message": "Memory is empty.", "memory": {}}
    return {"status": "success", "memory": {k: v["value"] for k, v in mem.items()}}

# --- SCHEDULING ---
@registry.register(
    "set_timer",
    "Set a non-blocking countdown timer. Shows a notification when done.",
    {
        "type": "object",
        "properties": {
            "seconds": {"type": "integer", "description": "Number of seconds to wait"},
            "label":   {"type": "string",  "description": "What this timer is for"}
        },
        "required": ["seconds"]
    }
)
async def set_timer(seconds: int, label: str = "Timer"):
    async def _run():
        await asyncio.sleep(seconds)
        subprocess.run(["notify-send", "-u", "critical", "-t", "0", "⏰ Timer abgelaufen!", label])
    asyncio.create_task(_run())
    return {"status": "success", "message": f"Timer '{label}' set for {seconds}s."}

@registry.register(
    "schedule_task",
    "Schedule a shell command sequence to run after a delay using systemd-run.",
    {
        "type": "object",
        "properties": {
            "commands":      {"type": "string",  "description": "The shell command(s) to execute"},
            "delay_seconds": {"type": "integer", "description": "How many seconds to wait before running."},
            "label":         {"type": "string",  "description": "Label for this job."}
        },
        "required": ["commands", "delay_seconds"]
    }
)
async def schedule_task(commands: str, delay_seconds: int, label: str = "scheduled-task"):
    if not shutil.which("systemd-run"):
        return {"status": "error", "message": "systemd-run not found."}
    safe_label = re.sub(r'[^a-zA-Z0-9-]', '-', label)[:40]
    cmd = ["systemd-run", "--user", f"--on-active={delay_seconds}s", f"--description={label}", "/bin/bash", "-c", commands]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode == 0:
        return {"status": "success", "message": f"Task '{label}' scheduled."}
    return {"status": "error", "message": res.stderr}

@registry.register(
    "get_current_datetime",
    "Get the current date and time (ISO format or human-readable). Use this for scheduling or checking the time.",
    {}
)
def get_current_datetime():
    now = datetime.now()
    return {
        "status": "success", 
        "datetime": now.strftime("%Y-%m-%d %H:%M:%S"),
        "iso": now.isoformat(),
        "weekday": now.strftime("%A")
    }

# --- AI UTILITIES ---
LANGUAGE_MAP = {
    "german": ("German", "de"), "deutsch": ("German", "de"), "de": ("German", "de"),
    "english": ("English", "en"), "englisch": ("English", "en"), "en": ("English", "en"),
    "french": ("French", "fr"), "französisch": ("French", "fr"), "fr": ("French", "fr"),
    "spanish": ("Spanish", "es"), "spanisch": ("Spanish", "es"), "es": ("Spanish", "es"),
    "latin": ("Latin", "la"), "latein": ("Latin", "la"), "la": ("Latin", "la"),
    "japanese": ("Japanese", "ja"), "japanisch": ("Japanese", "ja"), "ja": ("Japanese", "ja"),
    "dutch": ("Dutch", "nl"), "niederländisch": ("Dutch", "nl"), "nl": ("Dutch", "nl")
}

def _resolve_lang(lang_input: str) -> tuple[str, str]:
    lower = lang_input.lower().strip()
    return LANGUAGE_MAP.get(lower, (lang_input.capitalize(), lower))

@registry.register(
    "translate",
    "Translate a string in memory. Does NOT read or write files. Perform file I/O separately.",
    {
        "type": "object",
        "properties": {
            "text":        {"type": "string", "description": "The text to translate."},
            "target_lang": {"type": "string", "description": "Target language"},
            "source_lang": {"type": "string", "description": "Source language (default: auto)"}
        },
        "required": ["text", "target_lang"]
    }
)
async def translate(text: str, target_lang: str, source_lang: str = "auto"):
    target_name, target_code = _resolve_lang(target_lang)
    source_name, source_code = _resolve_lang(source_lang)
    
    if source_lang.lower() == "auto":
        source_name = "Auto-detected"
        source_code = "auto"

    prompt = (
        f"You are a professional {source_name} ({source_code}) to {target_name} ({target_code}) translator. "
        f"Your goal is to accurately convey the meaning and nuances of the original {source_name} text "
        f"while adhering to {target_name} grammar, vocabulary, and cultural sensitivities.\n"
        f"Produce only the {target_name} translation, without any additional explanations or commentary. "
        f"Please translate the following {source_name} text into {target_name}:\n\n\n"
        f"{text}"
    )

    global_logger.log_message("system", f"[agent] Translating from {source_name} to {target_name}...")
    try:
        response = ollama.chat(model=TRANSLATE_MODEL, messages=[{"role": "user", "content": prompt}])
        return {"status": "success", "translation": response["message"]["content"].strip()}
    except Exception as e:
        return {"status": "error", "message": str(e)}

@registry.register(
    "summarize_file",
    "Summarize file contents using AI.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Path to file"},
            "max_chars": {"type": "integer", "default": 8000}
        },
        "required": ["path"]
    }
)
async def summarize_file(path: str, max_chars: int = 8000):
    expanded = os.path.expanduser(path)
    try:
        with open(expanded, "r", encoding="utf-8", errors="replace") as f:
            content = f.read(max_chars)
        from main import MODEL # Use current core model
        prompt = f"Summarize concisely:\n\n{content}"
        response = ollama.chat(model=MODEL, messages=[{"role": "user", "content": prompt}])
        return {"status": "success", "summary": response["message"]["content"].strip()}
    except Exception as e:
        return {"status": "error", "message": str(e)}

@registry.register(
    "run_python",
    "Execute a Python code snippet and return output.",
    {"type": "object", "properties": {"code": {"type": "string"}}, "required": ["code"]}
)
def run_python(code: str):
    stdout_capture = io.StringIO()
    try:
        with contextlib.redirect_stdout(stdout_capture):
            local_scope = {}
            exec(code, {"__builtins__": __builtins__}, local_scope)
        return {"status": "success", "stdout": stdout_capture.getvalue()}
    except Exception as e:
        return {"status": "error", "error": str(e)}

# --- CORE LOGIC (JIT) ---
@registry.register("report_error", "Report a task failure.", {"type": "object", "properties": {"issue": {"type": "string"}}, "required": ["issue"]})
def report_error(issue: str, details: str = ""):
    global_logger.log_error_report("agent", issue, details)
    return {"status": "success"}

@registry.register(
    "discover_tools",
    "Search the INTERNAL SKILL DATABASE for a specific function signature. Use this ONLY to load new tools into memory (e.g. 'installing', 'email', 'timer'). NOT for searching files or system info.",
    {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}
)
def discover_tools(query: str):
    query = query.lower().strip()
    results = registry.search_tools(query)
    
    DOMAIN_TIPS = {
        "network": "WiFi -> 'get_wifi_info'. Scan -> 'scan_network'. Host info -> 'get_device_info(ip)'.",
        "app": "MANDATORY: Launch apps via 'open_app(name)'. Do NOT use run_command.",
        "file": "Search -> 'search_files'. Read -> 'read_file'. Write -> 'write_file'.",
        "system": "Info -> 'get_system_info'. Install -> 'install_software'. Command -> 'run_command'.",
#        "desktop": "Workspaces -> 'switch_workspace'. Screen -> 'take_screenshot'. OCR -> 'find_on_screen'.",
        "media": "Play -> 'play_audio'. Record -> 'record_audio'. Volume -> 'set_volume'.",
        "datetime": "Time/Date -> 'get_current_datetime'. Timer -> 'set_timer'. Schedule -> 'schedule_task'."
    }
    
    output = ["Discovered tools:"]
    for r in results.get("tools", []):
        output.append(f"- {r['function']['name']}: {r['function']['description']}")
    
    for domain, tip in DOMAIN_TIPS.items():
        if domain in query or (domain == "system" and "install" in query):
            output.append(f"\nTIP: {tip}")
            break
            
    return {"status": "success", "message": "\n".join(output)}
