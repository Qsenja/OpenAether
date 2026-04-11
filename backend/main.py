import sys, os, subprocess
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'lib'))

import asyncio
import json
import time
import websockets
import ollama
import re
import yaml
import glob
from tools import registry
from logger import global_logger
from shell_manager import global_shell
import quick_dispatch

from hypr_env import HYPRLAND_ENV

# Settings Management
SETTINGS_PATH = os.path.join(os.path.dirname(__file__), "config", "user_settings.json")

def load_settings():
    if os.path.exists(SETTINGS_PATH):
        try:
            with open(SETTINGS_PATH, "r") as f:
                return json.load(f)
        except Exception as e:
            print(f"Error loading settings: {e}")
    return {"pastebin_api_key": ""}

def save_settings(settings):
    try:
        os.makedirs(os.path.dirname(SETTINGS_PATH), exist_ok=True)
        with open(SETTINGS_PATH, "w") as f:
            json.dump(settings, f, indent=4)
        return True
    except Exception as e:
        print(f"Error saving settings: {e}")
    return False

    return False

async def upload_to_pastebin(file_path, api_key):
    """Upload file content to Pastebin using the providing API key."""
    import requests
    if not api_key:
        return {"status": "error", "message": "No API key provided."}
    
    try:
        with open(file_path, "r") as f:
            content = f.read()
        
        if not content.strip():
            return {"status": "error", "message": "The log file is empty and cannot be uploaded."}
        
        data = {
            'api_dev_key': api_key,
            'api_option': 'paste',
            'api_paste_code': content,
            'api_paste_name': os.path.basename(file_path),
            'api_paste_format': 'text',
            'api_paste_private': '1', # Unlisted
            'api_paste_expire_date': '1D' # 1 Day
        }
        
        response = requests.post("https://pastebin.com/api/api_post.php", data=data)
        if response.status_code == 200 and response.text.startswith("https://"):
            return {"status": "success", "url": response.text}
        else:
            return {"status": "error", "message": f"Pastebin Error: {response.text}"}
    except Exception as e:
        return {"status": "error", "message": f"Upload failed: {str(e)}"}

# -------------------------------------

# Configuration
PORT = 8765

# Model Configuration
# Qwen 2.5 14B (Standard) — Direct, smart, no R1-style overthinking
MODEL = "qwen2.5:14b"

# Sampling options (Direct & Precise)
SAMPLE_OPTIONS = {
    "temperature": 0.1,
    "top_p": 0.9,
    "top_k": 40,
    "repeat_penalty": 1.1,
    "stop": ["<|im_start|>", "<|im_end|>", "/think", "/no_think"]
}

def _parse_narrated_tool_calls(text: str, known_tools: dict) -> list:
    """
    Gemma4 E4B often outputs tool calls as narrated text instead of native calls.
    This parser handles all known patterns and returns a tool_calls list.
    
    Supported patterns:
    1. Python-style:  tool_name(arg1="val", arg2=123)
    2. JSON block:    {"name": "tool", "arguments": {...}}
                      {"name": "tool", "parameters": {...}}
    3. Bare text:     tool_name(query: "value")   <- Gemma4 common pattern
    """
    tool_calls = []

    # --- Pattern 1: <tool_call> XML tags (Custom Template Support) ---
    # Pattern: <tool_call>{"name": "...", "arguments": {...}}</tool_call>
    # Also handle UNCLOSED tags caused by stop tokens or stream cuts.
    xml_calls = re.findall(r'<tool_call>(.*?)(?:</tool_call>|$)', text, re.DOTALL)
    for call_text in xml_calls:
        if not call_text.strip(): continue
        try:
            # If the block is incomplete (missing closing brace), try to add them
            clean_call = call_text.strip()
            if clean_call.count('{') > clean_call.count('}'):
                clean_call += '}' * (clean_call.count('{') - clean_call.count('}'))
            
            block = json.loads(clean_call)
            name = block.get("name", "")
            args = block.get("arguments") or {}
            if name in registry.tools:
                tool_calls.append({'function': {'name': name, 'arguments': args}})
        except:
            pass

    # Strip markdown code blocks more thoroughly (including language identifiers)
    text = re.sub(r'```(?:json|python)?\s*(\{.*?\})\s*```', r'\1', text, flags=re.DOTALL)
    text = re.sub(r'```(?:json|python)?\s*(.*?)\s*```', r'\1', text, flags=re.DOTALL)
    
    text = text.strip()
    # We find all JSON blocks that look like tool calls using a stack to handle nested braces
    # Also handle malformed JSON where colon may be missing: {"name" "tool"} or {"name" : "tool"}
    matches = re.finditer(r'\{\s*"name"\s*[:"]\s*"([^"]+)"', text)
    for match in matches:
        start_idx = match.start()
        brace_count = 0
        end_idx = -1
        for i in range(start_idx, len(text)):
            if text[i] == '{':
                brace_count += 1
            elif text[i] == '}':
                brace_count -= 1
                if brace_count == 0:
                    end_idx = i + 1
                    break

        if end_idx != -1:
            try:
                block_str = text[start_idx:end_idx]
                # Ensure we only have the JSON object
                block = json.loads(block_str)
                name = block.get("name", "")
                args = block.get("arguments") or block.get("parameters") or block.get("args") or {}
                
                # Resolve aliases here too for robustness
                if name in registry.aliases:
                    name = registry.aliases[name]

                # We allow all known tools to be parsed, so the executor can handle
                # undiscovered tools with a descriptive error instead of silent failure.
                if name in registry.tools:
                    tool_calls.append({'function': {'name': name, 'arguments': args}})
            except:
                pass
    
    if tool_calls:
        # Final discipline check: strip hallucinated parameters from known tools
        for call in tool_calls:
            name = call['function']['name']
            args = call['function']['arguments']
            schema = next((s for s in registry.get_schemas() if s['function']['name'] == name), None)
            if schema:
                allowed = schema['function']['parameters'].get('properties', {}).keys()
                # If the tool has defined properties, remove any keys NOT in that list
                if allowed:
                    call['function']['arguments'] = {k: v for k, v in args.items() if k in allowed}
        return tool_calls

    # --- Pattern 2: Python-style: tool_name(key="val", key2=123) ---
    # Also handles Gemma4's colon-notation: tool_name(key: "val")
    #
    # IMPORTANT: Strip inline-backtick code FIRST so mentions like
    # `get_device_info(ip)` in explanatory text are never executed.
    safe_text = re.sub(r'`[^`]*`', '', text)

    for tool_name in registry.tools:
        escaped = re.escape(tool_name)
        call_match = re.search(
            escaped + r'\s*\(([^)]*)\)',
            safe_text, re.DOTALL | re.IGNORECASE
        )
        if call_match:
            args_str = call_match.group(1).strip()
            args = {}
            if args_str:
                # Normalize colon-notation to equals
                normalized = re.sub(r'(\w+)\s*:\s*', r'\1=', args_str)
                # Extract key=value pairs, handling quoted strings and numbers
                found_any = False
                for pair in re.finditer(r'(\w+)\s*=\s*("(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)*\'|\w+|-?\d+(?:\.\d+)?)', normalized):
                    key = pair.group(1)
                    val_raw = pair.group(2)
                    # Reject bare identifiers as values — these are likely placeholders
                    # e.g. tool_name(ip) where 'ip' is a param name, not a value
                    if re.match(r'^[a-zA-Z_][a-zA-Z0-9_]*$', val_raw) and val_raw not in ('true', 'false', 'True', 'False', 'null', 'None'):
                        # Might be a placeholder — skip
                        continue
                    # Parse the value type
                    if val_raw.startswith('"') or val_raw.startswith("'"):
                        val = val_raw[1:-1]
                    elif val_raw.lower() == "true":
                        val = True
                    elif val_raw.lower() == "false":
                        val = False
                    else:
                        try:
                            val = int(val_raw)
                        except ValueError:
                            try:
                                val = float(val_raw)
                            except ValueError:
                                val = val_raw
                    args[key] = val
                    found_any = True

                # If args_str was non-empty but we found NO valid key=value pairs,
                # the call is likely a placeholder like tool(ip) — skip it entirely.
                if not found_any:
                    continue

            tool_calls.append({'function': {'name': tool_name, 'arguments': args}})
            return tool_calls

    return tool_calls


async def check_searxng():
    """Verify SearXNG reachability and attempt auto-start via Docker if down."""
    from skills.web import get_searxng_url
    import requests
    url = get_searxng_url()
    
    async def try_ping():
        try:
            # Query the search endpoint directly to verify engine is actually ready
            response = requests.get(f"{url}/search?q=ping", timeout=2)
            return response.status_code == 200
        except:
            return False

    if await try_ping():
        return

    print(f"SearXNG not reachable at {url}. Attempting auto-start via Docker...")
    try:
        # Attempt to start the container named 'searxng'
        proc = await asyncio.create_subprocess_exec(
            "docker", "start", "searxng",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE
        )
        stdout, stderr = await proc.communicate()
        
        if proc.returncode == 0:
            print("SearXNG container start command sent. Waiting for initialization (5s)...")
            await asyncio.sleep(5)
            if await try_ping():
                print("SearXNG is now ONLINE.")
            else:
                print("SearXNG container started but service is not responding yet. It may take longer to boot.")
        else:
            err = stderr.decode().strip()
            print(f"Failed to start SearXNG container: {err}")
            print("Please ensure Docker is running and a container named 'searxng' exists.")
    except Exception as e:
        print(f"Docker auto-start error: {e}")

# Load system prompt from editable config file
def _load_system_prompt() -> str:
    prompt_path = os.path.join(os.path.dirname(__file__), "config", "system_prompt.txt")
    try:
        with open(prompt_path, "r", encoding="utf-8") as f:
            return f.read()
    except Exception as e:
        global_logger.log_message("system", f"[main] Failed to load system_prompt.txt: {e}")
        return "You are Aether Core, the intelligence layer of the OpenAether desktop framework."

SYSTEM_PROMPT = _load_system_prompt()


# Wire up quick_dispatch module
quick_dispatch.load()


class AgentSession:
    def __init__(self, websocket):
        self.websocket = websocket
        self.messages = [{'role': 'system', 'content': SYSTEM_PROMPT}]
        self.current_task = None
        self.pending_approval = None
        self.interrupted = False
        self.empty_retries = 0  # Guard against infinite empty-response loop
        # Minimal preloads: only tools the AI should ALWAYS have.
        # Everything else MUST be discovered via discover_tools first.
        self.loaded_tools = {
            "discover_tools",  # Discovery system — always available
            "web_search",      # Research — always available
            "fetch_url",       # Research — always available
            "run_command",     # Guarded — intercepts common patterns
            "report_error",    # System logging — always available
            "search_files",    # Filesystem — preloaded for speed
            "list_directory",  # Filesystem — preloaded for speed
            "write_file",      # Filesystem — preloaded for speed
            "open_app",        # App Launching — preloaded for speed
            "read_file",       # Filesystem — preloaded for speed
            "run_in_workspace",# Workspace — preloaded for speed
            "get_current_datetime", # Time — preloaded for speed
            "translate",       # Intelligence — preloaded for speed
            "remember",        # Memory — preloaded for speed
            "delete_path",     # Filesystem — essential for management
            "move_path",       # Filesystem — essential for management
            "edit_file",       # Filesystem — safe updates
            "install_software",# System — essential for management
            "run_command",     # System — essential for management
            "get_system_info", # System — essential for management
        }
        self.last_tool_calls = [] # History of (name, args) for loop prevention

    async def prune_history(self, max_len=15):
        """Keep system prompt (index 0) and the most recent N messages."""
        if len(self.messages) > max_len + 1:
            system_msg = self.messages[0]
            recent_msgs = self.messages[-(max_len):]
            self.messages = [system_msg] + recent_msgs
            global_logger.log_message("system", f"[Context] Pruned history to last {max_len} messages.")

    async def send_json(self, data):
        await self.websocket.send(json.dumps(data))
        global_logger.log_event("server_to_client", data)

    async def update_status(self, status):
        # Default to Core, but specific parts of the loop (like Spark) will override via manual send_json if needed
        await self.send_json({"type": "status_update", "status": status, "model": "Aether Core"})

    async def run(self):
        print(f"New agent session started")
        try:
            async for message in self.websocket:
                data = json.loads(message)
                global_logger.log_event("client_to_server", data)
                
                if data.get("type") == "user_message":
                    user_msg = data.get("content")
                    self.messages.append({'role': 'user', 'content': user_msg})
                    global_logger.log_message("user", user_msg)
                    self.last_tool_calls = [] # Reset loop protection for new user intent
                    self.current_task = asyncio.create_task(self.process_loop())
                
                elif data.get("type") == "stop_request":
                    self.interrupted = True
                    if self.current_task and not self.current_task.done():
                        print("Received STOP request. Interrupting...")
                        global_shell.interrupt()
                        self.current_task.cancel()
                        await self.update_status("idle")
                
                elif data.get("type") == "approval_response":
                    if self.pending_approval:
                        self.pending_approval["future"].set_result(data.get("approved", False))
                
                elif data.get("type") == "open_logs":
                    print("[Backend] Processing open_logs request...")
                    # This requested the modal data
                    log_dir = global_logger.log_dir
                    logs = []
                    if os.path.exists(log_dir):
                        files = [os.path.join(log_dir, f) for f in os.listdir(log_dir) if f.endswith(".log")]
                        # Sort by mtime descending, then by filename descending for stability
                        files.sort(key=lambda x: (os.path.getmtime(x), x), reverse=True)
                        for f in files[:5]:
                            logs.append({
                                "name": os.path.basename(f),
                                "path": f,
                                "time": time.ctime(os.path.getmtime(f)),
                                "is_active": f == global_logger.log_file
                            })
                    
                    settings = load_settings()
                    await self.send_json({"type": "logs_data", "logs": logs, "settings": settings})

                elif data.get("type") == "update_settings":
                    settings = data.get("settings", {})
                    save_settings(settings)
                    # No response needed, silent save

                elif data.get("type") == "get_log_content":
                    file_path = data.get("path")
                    if file_path and os.path.exists(file_path):
                        try:
                            with open(file_path, "r") as f:
                                content = f.read()
                            await self.send_json({"type": "log_content", "content": content, "path": file_path})
                        except Exception as e:
                            await self.send_json({"type": "error", "content": f"Failed to read log: {e}"})

                elif data.get("type") == "delete_log_file":
                    file_path = data.get("path")
                    print(f"[Backend] Deletion requested for: {file_path}")
                    if file_path and os.path.exists(file_path):
                        # Prevent deleting the actively writing log
                        if file_path == global_logger.log_file:
                            global_logger.log_message("system", f"Refusing to delete active log: {file_path}")
                        else:
                            try:
                                # Hook into the registry tool logic
                                from skills.system import delete_path
                                result = delete_path(file_path)
                                if result.get("status") == "success":
                                    await self.send_json({"type": "log_deleted", "path": file_path})
                                else:
                                    global_logger.log_message("system", f"Deletion tool error: {result.get('message')}")
                            except Exception as e:
                                global_logger.log_message("system", f"Exception during deletion: {e}")

                elif data.get("type") == "upload_log_pastebin":
                    file_path = data.get("path")
                    api_key = data.get("api_key")
                    if file_path and os.path.exists(file_path):
                        result = await upload_to_pastebin(file_path, api_key)
                        await self.send_json({"type": "pastebin_result", "result": result, "path": file_path})
                    
        except websockets.exceptions.ConnectionClosed:
            print("Client disconnected")

    async def process_loop(self):
        await self.update_status("thinking") # Initial emit to set model badge
        self.interrupted = False
        try:
            # PRUNE HISTORY: Maintain a clean sliding window for context stability
            await self.prune_history(max_len=15)

            # CAP: Max 15 tool iterations per turn to prevent infinite loops or "spiraling" behavior.
            for turn in range(15):
                # Ensure other messages (like open_logs) can be processed between turns
                await asyncio.sleep(0.01)
                # Context Injection (Fetched early for Spark awareness)
                windows_context = []
                try:
                    if "get_open_windows" in registry.tools:
                        windows_context = registry.tools["get_open_windows"]()
                except Exception as ce:
                    global_logger.log_message("system", f"[Context Fetch Error]: {ce}")

                last_msg = self.messages[-1] if self.messages else {}
                if last_msg.get('role') == 'user':
                    quick = await quick_dispatch.dispatch(last_msg.get('content', ''), registry, windows_context)
                    if quick:
                        if "response" in quick:
                            # INSTANT GREETING: No tool involved, just a text response.
                            response_text = quick["response"]
                            self.messages.append({'role': 'assistant', 'content': response_text})
                            global_logger.log_message("assistant", response_text)
                            await self.send_json({"type": "agent_message", "content": response_text, "model": "Aether Spark"})
                            await self.send_json({"type": "agent_message_done"})
                            await self.update_status("idle")
                            return

                        # TWO-STAGE SPARK DISPATCH
                        tool_name = quick['tool']
                        tool_args = quick['args']
                        pre_msg = quick.get('pre_msg', 'Processing...')
                        
                        try:
                            # 1. Notify intent
                            await self.send_json({"type": "status_update", "status": "dispatching", "model": "Aether Spark"})
                            await self.send_json({"type": "agent_message", "content": pre_msg, "model": "Aether Spark"})
                            
                            # 2. Execute
                            result = await registry.execute(tool_name, tool_args)
                            
                            # LOGGING: Record the tool execution for visibility in the logs
                            global_logger.log_tool(tool_name, tool_args, result)

                            # FAIL-FORWARD HANDOVER: 
                            if isinstance(result, dict) and result.get("status") == "error":
                                global_logger.log_message("system", f"[Spark Fail-Forward]: Tool '{tool_name}' failed with: {result.get('message')}. Handing over to Core.")
                                handover_msg = {
                                    "role": "user", 
                                    "content": f"(System Note: Fast-dispatch layer attempted {tool_name}({json.dumps(tool_args)}) but it failed: '{result.get('message')}'. AI Core: Please investigate manually.)"
                                }
                                self.messages.append(handover_msg)
                                # Continue the loop to hit Core reasoning
                            else:
                                response_text = quick_dispatch._format_result(tool_name, tool_args, result)
                                if response_text:
                                    await self.send_json({"type": "agent_message", "content": f"\n\n{response_text}", "model": "Aether Spark"})
                                    # Store results in history
                                    self.messages.append({'role': 'assistant', 'content': f"{pre_msg}\n{response_text}"})
                                
                                await self.send_json({"type": "agent_message_done"})
                                await self.update_status("idle")
                                return # Successful Spark execution ends the turn
                        except Exception as e:
                            global_logger.log_message("system", f"[Spark Handover]: Execution crash, handing off to AI Core. Error: {e}")
                            # Fall through to Core
                            pass

                await self.update_status("thinking")
                
                # Context Injection (System Prompt)
                try:
                    window_block = f"\n\n[ACTUAL OPEN WINDOWS]: {json.dumps(windows_context)}" if windows_context else ""
                    
                    app_db_block = ""
                    try:
                        from skills.system import get_installed_apps_index
                        apps_cache = get_installed_apps_index()
                        app_db_block = f"\n\n[INSTALLED APPS DATABASE (Cache)]: {', '.join(apps_cache)}"
                    except: pass
                    
                    if self.messages[0]['role'] == 'system':
                        self.messages[0]['content'] = SYSTEM_PROMPT + (window_block or "") + (app_db_block or "")
                except Exception as ce:
                    global_logger.log_message("system", f"[Context Injection Error]: {ce}")

                # Call Ollama with Streaming
                full_content = ""
                thought_content = ""  # Gemma4 <|channel>thought<channel|> blocks
                tool_calls = []
                
                try:
                    # Filter current session schemas
                    active_schemas = [s for s in registry.get_schemas() if s['function']['name'] in self.loaded_tools]
                    
                    response_stream = ollama.chat(
                        model=MODEL,
                        messages=self.messages,
                        tools=active_schemas,
                        stream=True,
                        options=SAMPLE_OPTIONS
                    )

                    # ── STREAMING LOOP ──────────────────────────────────────────────
                    # Handles three mechanisms for thinking content:
                    #  A) Ollama native 'thinking' field (qwen3.5, Ollama >= 0.7)  ← primary
                    #  B) <think>...</think> tags embedded in content              ← fallback
                    #  C) <|channel|>thought...<channel|> tags (Gemma-family)      ← fallback
                    raw_buf = ""
                    in_thought = False
                    thought_sent = False
                    think_open = None
                    think_close = None
                    native_thinking_buf = ""  # Accumulates Ollama 'thinking' field chunks

                    THINK_PARSERS = [
                        ("<think>",            "</think>"),
                        ("<|channel|>thought", "<channel|>"),
                    ]

                    for chunk in response_stream:
                        if self.interrupted or (self.current_task and self.current_task.cancelled()):
                            break

                        msg_chunk = chunk['message']

                        # ── A) Native tool calls ──
                        if msg_chunk.get('tool_calls'):
                            tool_calls = msg_chunk['tool_calls']
                            # Flush any accumulated native thinking before breaking
                            if native_thinking_buf.strip():
                                thought_content = native_thinking_buf
                                await self.send_json({"type": "agent_thought", "content": native_thinking_buf})
                                native_thinking_buf = ""
                            break

                        # ── B) Ollama native 'thinking' field (qwen3.5, Ollama >= 0.7) ──
                        if msg_chunk.get('thinking'):
                            thinking_chunk = msg_chunk['thinking']
                            native_thinking_buf += thinking_chunk
                            # Stream thinking in real-time to UI
                            await self.send_json({"type": "agent_thought_chunk", "content": thinking_chunk})
                            thought_content += thinking_chunk
                            thought_sent = True
                            continue

                        if not msg_chunk.get('content'):
                            continue

                        text = msg_chunk['content']
                        raw_buf += text

                        # ── C) <think> tags embedded in content (fallback) ──
                        if not in_thought and not thought_sent and think_open is None:
                            match_found = False
                            for open_tok, close_tok in THINK_PARSERS:
                                if open_tok in raw_buf:
                                    in_thought = True
                                    think_open = open_tok
                                    think_close = close_tok
                                    match_found = True
                                    break
                            
                            if in_thought:
                                # We just entered a thought block - don't finalize yet
                                continue

                            # Look-ahead guard: don't output if it MIGHT be a think tag.
                            # We only 'continue' (buffer) if raw_buf is a valid prefix of a think tag.
                            is_prefix = any(open_tok.startswith(raw_buf.lstrip()) for open_tok, _ in THINK_PARSERS)
                            
                            if is_prefix:
                                # Special case: if it starts with "<tool_call", it's NOT a think tag prefix
                                # (even though "<" or "<t" match both).
                                if raw_buf.lstrip().startswith("<tool_call"):
                                    # Not a think tag prefix after all - fall through and flush!
                                    pass
                                else:
                                    # It might be a think tag - keep buffering
                                    continue

                        # ── Inside <think> block: wait for closing tag ──
                        if in_thought:
                            if think_close not in raw_buf:
                                continue
                            parts = raw_buf.split(think_close, 1)
                            thought_raw = parts[0].replace(think_open, "").lstrip("\n")
                            remainder = parts[1] if len(parts) > 1 else ""
                            if thought_raw.strip():
                                thought_content = thought_raw
                                await self.send_json({"type": "agent_thought", "content": thought_raw})
                            in_thought = False
                            thought_sent = True
                            raw_buf = remainder
                            if remainder.strip():
                                full_content += remainder
                                await self.send_json({"type": "agent_message", "content": remainder})
                            continue

                        # ── Normal text/Tool call: flush and stream directly to UI ──
                        if raw_buf:
                            # If we are NOT in a think tag and NOT currently buffering a prefix, flush!
                            full_content += raw_buf
                            await self.send_json({"type": "agent_message", "content": raw_buf})
                            raw_buf = "" 

                    # Strip any remaining think-tags from full_content (safety net)
                    think_pattern = re.compile(
                        r'(<think>.*?</think>|<\|channel\|>thought.*?<channel\|>)',
                        re.DOTALL
                    )
                    
                    # Final Flush: If anything is left in raw_buf, it's normal text
                    if raw_buf.strip():
                        # If a think tag was opened but never closed, treat it as text
                        if in_thought:
                            full_content += raw_buf
                        else:
                            full_content += raw_buf
                        # Avoid double-sending if already sent via chunk logic, 
                        # but for safety in case of early break:
                        if not full_content.endswith(raw_buf):
                             pass 

                    full_content = think_pattern.sub('', full_content).strip()

                    # Final fallback: use raw_buf if full_content is empty
                    if not full_content and raw_buf:
                        cleaned_raw = think_pattern.sub('', raw_buf).strip()
                        if cleaned_raw:
                            full_content = cleaned_raw



                except asyncio.CancelledError:
                    return
                except Exception as e:
                    await self.send_json({"type": "error", "content": f"Model error: {str(e)}"})
                    await self.update_status("idle")
                    return

                # [FALLBACK] Multi-format tool call detection
                # Some models narrate tool calls as text, or put them inside <think> blocks.
                # We parse multiple formats, RESTRICTED to discovered tools.
                if not tool_calls:
                    tool_calls = _parse_narrated_tool_calls(full_content, self.loaded_tools)

                # [FALLBACK 2] Tool call was inside <think> block (model put JSON in reasoning)
                # In this case full_content is empty but thought_content has the tool call.
                if not tool_calls and not full_content and thought_content:
                    tool_calls = _parse_narrated_tool_calls(thought_content, self.loaded_tools)
                    if tool_calls:
                        global_logger.log_message("system", "[think-rescue] Extracted tool call from thought block.")

                # --- BASH-BLOCK INTERCEPTOR ---
                # Catch responses where the AI outputs shell commands in code blocks
                # instead of calling the proper tool. Force a correction loop.
                BASH_INTERCEPT_PATTERNS = [
                    ("nmap", "network scanning", "scan_network or get_device_info"),
                    ("hyprctl dispatch workspace", "workspace switching", "switch_workspace or run_in_workspace"),
                    ("hyprctl dispatch movetoworkspace", "window moving", "move_window_to_workspace"),
                    ("kitty", "launching kitty terminal", "open_app or run_in_workspace"),
                    ("firefox", "launching firefox", "open_app"),
                    ("chromium", "launching browser", "open_app"),
                    ("pacman -s", "package installation", "install_software"),
                    ("pacman -r", "package removal", "uninstall_software"),
                    ("pacman -qt", "cleaning orphaned packages", "cleanup_system_orphans"),
                    ("pacman -sc", "cleaning package cache", "manage_package_cache"),
                    ("paccache", "cleaning package cache", "manage_package_cache"),
                    ("journalctl --vacuum", "trimming system logs", "trim_system_logs"),
                    ("du -h", "search for large files", "find_large_entities"),
                    ("iwconfig", "wifi info", "get_wifi_info"),
                    ("pactl", "audio control", "set_volume"),
                ]
                if not tool_calls and full_content:
                    bash_blocks = re.findall(r'```(?:bash|sh|zsh)\n(.*?)```', full_content, re.DOTALL)
                    bash_content = " ".join(bash_blocks).lower()
                    for pattern, category, suggestion in BASH_INTERCEPT_PATTERNS:
                        if pattern.lower() in bash_content:
                            global_logger.log_message("system", f"[bash-intercept] Blocked shell instruction for '{category}'.")
                            # Inject correction as a tool result to force AI to retry properly
                            correction_msg = {
                                'role': 'tool',
                                'name': 'system',
                                'content': (
                                    f"CORRECTION: You output shell commands for '{category}' instead of calling a tool. "
                                    f"This is FORBIDDEN. You must call discover_tools('{category}') to find '{suggestion}' "
                                    f"and use it directly. Do NOT tell the user to run anything. DO IT YOURSELF."
                                ),
                                'tool_call_id': 'bash_intercept'
                            }
                            self.messages.append({'role': 'assistant', 'content': full_content})
                            self.messages.append(correction_msg)
                            global_logger.log_message("assistant", full_content)
                            full_content = ""  # Clear so the loop retries
                            continue  # Force a new AI iteration
                # --- END BASH-BLOCK INTERCEPTOR ---

                # Build history entry (Keep thoughts to maintain multi-step coherence)
                # Formats reasoning as <think> content for the model to review in the next turn
                history_content = full_content
                if thought_content:
                    history_content = f"<think>\n{thought_content}\n</think>\n{full_content}"

                final_msg = {'role': 'assistant', 'content': history_content}
                if tool_calls: final_msg['tool_calls'] = tool_calls
                self.messages.append(final_msg)
                global_logger.log_message("assistant", history_content)

                if tool_calls:
                    await self.update_status("executing")
                    for i, tool_call in enumerate(tool_calls):
                        func_name = tool_call['function']['name']
                        args = tool_call['function']['arguments']
                        call_id = f"call-{int(time.time())}-{i}"
                        
                        # -- HALLUCINATION GUARD / LOOP BLOCKER --
                        # Prevent duplicate identical calls in a single turn
                        call_sig = (func_name, json.dumps(args, sort_keys=True))
                        if call_sig in self.last_tool_calls:
                            raw_result = {"status": "error", "message": "FATAL_LOOP_LIMIT: You have already attempted this exact action in this turn. You MUST NOT call any more tools now. Summarize the information you already possess from SOURCE entries in your history and provide a final answer to the user immediately."}
                        elif func_name not in self.loaded_tools:
                            raw_result = {"status": "error", "message": f"Tool '{func_name}' has not been discovered yet. Use discover_tools first."}
                        else:
                            self.last_tool_calls.append(call_sig)
                            await self.send_json({"type": "tool_call", "name": func_name, "args": args, "call_id": call_id})

                            # --- GUARD: send_notification used as chat reply ---
                            if func_name == "send_notification":
                                msg_len = len(args.get("message", ""))
                                if msg_len > 80:
                                    raw_result = {
                                        "status": "error",
                                        "message": "TOOL_MISUSE: send_notification is for background alerts only. DO NOT use it to reply to the user. Write your response as a normal chat message instead."
                                    }
                                else:
                                    raw_result = await registry.execute(func_name, args)

                            # --- GUARD: run_command interceptor ---
                            elif func_name == "run_command":
                                cmd = str(args.get("command", "")).lower()
                                INTERCEPTS = [
                                    (["nmap", "arp-scan", "arp "],      "network scanning",   "scan_network or get_device_info"),
                                    (["iwconfig", "iw dev", "nmcli"],   "wifi info",          "get_wifi_info"),
                                    (["ifconfig", "ip addr", "ip link"], "network interfaces", "get_wifi_info"),
                                    (["find ", "locate "],              "searching files",     "search_files"),
                                    (["mkdir ", "rmdir "],              "directory management","create_directory"),
                                    (["ls ", "dir ", "ls -"],           "listing files",       "list_directory"),
                                    (["rm ", "rm -"],                   "deleting files",      "delete_path"),
                                    (["touch ", "echo >"],              "creating files",      "write_file"),
                                    (["kitty", "firefox", "chromium", "thunar", "nautilus",
                                      "vlc ", "mpv ", "gimp", "code ", "steam", "discord"],
                                                                         "launching apps",     "open_app"),
                                    (["hyprctl dispatch workspace", "hyprctl dispatch movetoworkspace"],
                                                                         "workspace management","switch_workspace or move_window_to_workspace"),
                                    (["pactl set-sink-volume", "amixer"],"audio control",       "set_volume"),
                                    (["pacman -s", "yay -s", "paru -s"], "package install",     "install_software"),
                                    (["pacman -r", "yay -r", "paru -r"], "package remove",      "uninstall_software"),
                                    (["pacman -qt", "pacman -qd"],      "orphan cleanup",      "cleanup_system_orphans"),
                                    (["pacman -sc", "paccache"],        "cache cleanup",       "manage_package_cache"),
                                    (["journalctl --vacuum"],           "log cleanup",         "trim_system_logs"),
                                    (["scrot", "grim", "screenshot"],   "screenshot",          "take_screenshot"),
                                ]
                                intercepted = False
                                for patterns, category, suggestion in INTERCEPTS:
                                    if any(p in cmd for p in patterns):
                                        raw_result = {
                                            "status": "error",
                                            "message": f"TOOL_INTERCEPT: Do NOT use run_command for {category}. "
                                                       f"Use a specialized tool instead: {suggestion}. "
                                                       f"Call discover_tools('{category}') to find the exact tool and parameters."
                                        }
                                        intercepted = True
                                        break
                                if not intercepted:
                                    raw_result = await registry.execute(func_name, args)

                            else:
                                raw_result = await registry.execute(func_name, args)
                        
                        # Handle Approval Gate
                        if isinstance(raw_result, dict) and raw_result.get("status") == "requires_approval":
                            await self.send_json({
                                "type": "approval_required",
                                "tool": func_name, "args": args, "message": raw_result.get("message")
                            })
                            approval_future = asyncio.get_event_loop().create_future()
                            self.pending_approval = {"future": approval_future}
                            approved = await approval_future
                            self.pending_approval = None
                            
                            if approved:
                                if func_name == "run_command":
                                    raw_result = await global_shell.execute(args["command"], args.get("timeout", 30))
                                else: raw_result = {"status": "error", "message": "Approval only for run_command."}
                            else: raw_result = {"status": "error", "message": "Rejected by user."}

                        # Finalize tool result
                        # Wrap in <tool_response> tags as per user's template
                        raw_content = json.dumps({k: v for k, v in raw_result.items() if k != 'image_base64'}, ensure_ascii=False) if isinstance(raw_result, dict) else str(raw_result)
                        
                        # Truncate for AI context to keep context window stable
                        if len(raw_content) > 3000:
                            raw_content = raw_content[:3000] + "\n\n[TRUNCATED: Only the first 3000 chars shown.]"

                        # Format for history according to custom template
                        history_content = f"<tool_response>\n{raw_content}\n</tool_response>"

                        # Intent Persistence: Remind the model of the original task
                        original_user_msg = next(
                            (m['content'] for m in reversed(self.messages) if m['role'] == 'user' and not m['content'].startswith("(")), ''
                        )
                        if original_user_msg:
                            history_content += f"\n\n(System Note: Task Context Reminder. Original goal: '{original_user_msg}'. Continue until ALL parts are answered.)"

                        tool_msg = {
                            'role': 'tool',
                            'name': func_name,
                            'content': history_content,
                            'tool_call_id': tool_call.get('id', '')
                        }
                        if isinstance(raw_result, dict) and raw_result.get('image_base64'):
                            tool_msg['images'] = [raw_result['image_base64']]
                        
                        if func_name == "discover_tools" and isinstance(raw_result, dict) and raw_result.get("status") == "success":
                            # Auto-load the discovered tools into context
                            # re is imported globally at the top of the file
                            discovered = re.findall(r"- ([a-zA-Z0-9_]+)\(", raw_result.get("message", ""))
                            for d in discovered:
                                self.loaded_tools.add(d)

                        self.messages.append(tool_msg)
                        
                        # -- SMART LOGGING & METADATA --
                        log_output = raw_result
                        if isinstance(raw_result, dict) and "content" in raw_result and len(str(raw_result["content"])) > 1000:
                            # Log metadata but keep the actual history full for LLM
                            log_output = {**raw_result, "content": f"[BLOAT SHIELD] {len(str(raw_result['content']))} chars suppressed from logs."}
                        global_logger.log_tool(func_name, args, log_output)
                        
                        await self.send_json({"type": "tool_output", "name": func_name, "output": raw_result, "call_id": call_id})
                        
                    continue
                elif not full_content:
                    self.empty_retries += 1
                    if self.empty_retries >= 3:
                        global_logger.log_message("system", "[LOOP-GUARD] Max empty retries or nudges reached. Stopping.")
                        await self.send_json({"type": "agent_message", "content": "Ich konnte keine Antwort generieren (zu viele Versuche). Bitte versuche es noch einmal."})
                        await self.send_json({"type": "agent_message_done"})
                        await self.update_status("idle")
                        break

                    # Model returned empty response with no tool calls.
                    # CHECK FOR THOUGHTS: If it thought but didn't act, nudge it.
                    # CRITICAL: Prevent fueling a repetitive reasoning loop.
                    # If thinking is already very long (> 2000 chars), the model is likely stuck.
                    # In that case, do NOT nudge—just proceed to Spark fallback or retry.
                    if thought_content and len(thought_content) < 2000:
                        global_logger.log_message("system", f"[LOOP-GUARD] Short thought ({len(thought_content)} chars) found but no action. Nudging...")
                        self.messages.append({'role': 'user', 'content': "(System Note: I see your reasoning but no result was provided. Please either produce a text response for the user or call a tool to proceed.)"})
                        continue
                    elif thought_content:
                        global_logger.log_message("system", f"[LOOP-GUARD] Long thought ({len(thought_content)} chars) detected. Model is likely stuck in overthinking. Skipping nudge to prevent loop fueling.")

                    # Otherwise, try quick_dispatch on the original user message as fallback.
                    original_user_msg = next(
                        (m['content'] for m in reversed(self.messages) if m['role'] == 'user'), ''
                    )
                    quick = await quick_dispatch.dispatch(original_user_msg, registry)
                    if quick:
                        response_text = quick.get('response') or quick_dispatch._format_result(quick.get('tool'), quick.get('args'), "(Action executed by fallback)")
                        self.messages.append({'role': 'assistant', 'content': response_text})
                        global_logger.log_message("assistant", response_text)
                        await self.send_json({"type": "agent_message", "content": response_text, "model": "Aether Spark"})
                        await self.send_json({"type": "agent_message_done"})
                        await self.update_status("idle")
                        break
                    else:
                        global_logger.log_message("assistant", "[Empty response — retrying]")
                        continue
                else:
                    self.empty_retries = 0  # Reset on successful response
                    
                    await self.send_json({"type": "agent_message_done"})
                    await self.update_status("idle")
                    return # Use return instead of break in the turn-based for loop
            
            # If we exited the loop without returning, it means we hit the turn limit
            global_logger.log_message("system", f"[TURN-LIMIT] Agent reached max iterations (15).")
            await self.send_json({"type": "agent_message", "content": "Ich habe die maximale Anzahl an Arbeitsschritten für diese Anfrage erreicht. Bitte präzisiere deine Anfrage, falls das Ergebnis unvollständig ist."})
            await self.send_json({"type": "agent_message_done"})
            await self.update_status("idle")
        except asyncio.CancelledError:
            # Silent exit on Stop button
            pass
        except Exception as e:
            if not self.interrupted:
                await self.send_json({"type": "error", "content": f"Session error: {str(e)}"})
            await self.update_status("idle")
        finally:
            if not self.interrupted:
                await self.update_status("idle")


async def main():
    await check_searxng()
    async with websockets.serve(lambda ws: AgentSession(ws).run(), "localhost", PORT):
        await asyncio.Future()

if __name__ == "__main__":
    asyncio.run(main())
