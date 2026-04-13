import sys, os

import asyncio
import json
import shutil
import subprocess
import re
import io
import contextlib
from datetime import datetime
import ollama
from registry import registry, BaseTool
from shell_manager import global_shell

# --- CONFIGURATION ---
MEMORY_PATH = os.path.expanduser("~/.config/openaether/memory.json")
TRANSLATE_MODEL = "translategemma:4b"
# Default fallback if specialized model fails
FALLBACK_MODEL = "qwen2.5:14b"

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
        subprocess.run(["notify-send", "-u", "critical", "-t", "0", "⏰ Timer expired!", label])
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
        combined = res.stdout + res.stderr
        unit_match = re.search(r'unit: ([a-zA-Z0-9.-]+)', combined)
        unit = unit_match.group(1) if unit_match else "unknown"
        return {"status": "success", "message": f"Task '{label}' scheduled. It will run in {delay_seconds}s.", "unit": unit}
    return {"status": "error", "message": res.stderr}

@registry.register(
    "get_current_datetime",
    "Get the current date and time (ISO format or human-readable).",
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
    "english": ("English", "en"), "englisch": ("English", "en"), "en": ("English", "en")
}

def _resolve_lang(lang_input: str) -> tuple[str, str]:
    lower = lang_input.lower().strip()
    return LANGUAGE_MAP.get(lower, (lang_input.capitalize(), lower))

@registry.register(
    "translate",
    "Translate text from one language to another.",
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
    
    prompt = (
        f"Translate the following to {target_name}. Output only the translation:\n\n"
        f"{text}"
    )

    try:
        response = ollama.chat(model=TRANSLATE_MODEL, messages=[{"role": "user", "content": prompt}])
        return {"status": "success", "translation": response["message"]["content"].strip()}
    except Exception as e:
        registry.log_message("warn", f"[translate] Primary model {TRANSLATE_MODEL} failed: {e}. Trying fallback...")
        try:
            # Fallback to the main model (we assume qwen2.5:14b or what is default)
            response = ollama.chat(model=FALLBACK_MODEL, messages=[{"role": "user", "content": prompt}])
            return {
                "status": "success", 
                "translation": response["message"]["content"].strip(),
                "note": f"Translated using fallback model {FALLBACK_MODEL}"
            }
        except Exception as e2:
            return {"status": "error", "message": f"Translation failed on all models: {str(e2)}"}

@registry.register(
    "run_python",
    "Execute a Python code snippet.",
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

@registry.register
class CalculateDiscount(BaseTool):
    description = 'Calculates the final price after applying a discount percentage. Use this for math examples.'
    parameters = {
        'type': 'object',
        'properties': {
            'original_price': {'type': 'number', 'description': 'The initial price before discount'},
            'discount_percent': {'type': 'number', 'description': 'The discount percentage (0-100)'}
        },
        'required': ['original_price', 'discount_percent']
    }

    def call(self, params: str, **kwargs) -> str:
        try:
            import json5
            args = json5.loads(params)
            price = args['original_price']
            discount = args['discount_percent']
            final_price = price * (1 - (discount / 100))
            # Strict alignment with guide format
            return json.dumps({"final_price": final_price}, ensure_ascii=False)
        except Exception as e:
            return json.dumps({"error": f"Execution failed: {str(e)}"})

@registry.register("report_error", "Report a task failure.", {"type": "object", "properties": {"issue": {"type": "string"}}, "required": ["issue"]})
def report_error(issue: str, details: str = ""):
    registry.log_event("error_report", {"issue": issue, "details": details, "tag": "AGENT"})
    return {"status": "success"}

@registry.register
class DiscoverTools(BaseTool):
    name = "discover_tools"
    description = "Search the INTERNAL SKILL DATABASE and documentation for specific functions. Use this if you are unsure about your available capabilities or need to see tool parameters."
    parameters = {
        "type": "object",
        "properties": {"query": {"type": "string", "description": "Keyword to search for"}},
        "required": ["query"]
    }

    def call(self, params: str, **kwargs) -> str:
        try:
            import json5
            args = json5.loads(params)
            query = args.get('query', '').lower().strip()
            
            results = registry.search_tools(query)
            found_tools = [r['function']['name'] for r in results.get("tools", [])]
            
            if not results.get("tools") and not results.get("knowledge"):
                return json.dumps({"error": f"No tools found matching query '{query}'."}, ensure_ascii=False)

            output = ["Discovered tools (now being loaded):"]
            for r in results.get("tools", []):
                # Include the FULL schema so the model knows parameters immediately
                schema_block = json.dumps(r['function'], indent=2, ensure_ascii=False)
                output.append(schema_block)
            
            return json.dumps({
                "found_tools": found_tools, 
                "loaded_tools": found_tools,
                "details": "\n".join(output),
                "instruction": "SUCCESS: These tools are now LOADED and ready for use. Proceed with the task using these tools directly. DO NOT call discover_tools again for these same functions."
            }, ensure_ascii=False)
        except Exception as e:
            return json.dumps({"error": str(e)})
