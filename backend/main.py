import sys, os, subprocess, platformdirs, argparse

# CLI Arguments
parser = argparse.ArgumentParser()
parser.add_argument("--setup-test", action="store_true", help="Force the setup/OOBE screen for testing")
args, _ = parser.parse_known_args()
FORCED_SETUP = args.setup_test

# Emergency stderr logging for headless environments (AUR)
def emergency_log(msg):
    sys.stderr.write(f"[EMERGENCY] {msg}\n")
    sys.stderr.flush()

try:
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'lib'))
except Exception as e:
    emergency_log(f"Path initialization failed: {e}")

import asyncio
import json
import time
import websockets
import ollama
import re
import yaml
import glob
from logger import global_logger, tlog, GLOBAL_START_TIME
from shell_manager import global_shell
import quick_dispatch
from registry import registry, set_main_loop

boot_log = tlog
boot_log("Backend Script Starting...")

try:
    from qwen_agent.agents import Assistant
    from qwen_agent.tools.base import TOOL_REGISTRY
    # Purge the built-in web_search that requires Serper API
    TOOL_REGISTRY.pop("web_search", None)
except ImportError:
    Assistant = None
    TOOL_REGISTRY = {}

from hypr_env import HYPRLAND_ENV

# Settings Management
SETTINGS_PATH = os.path.join(platformdirs.user_config_dir("openaether"), "user_settings.json")

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

# Configuration
PORT = 8765
MODEL = "qwen2.5:14b"
SAMPLE_OPTIONS = {
    "temperature": 0.1,
    "top_p": 0.9,
    "frequency_penalty": 1.1,
    "stop": ["<|im_end|>", "/think", "/no_think"]
}

def filter_llm_params(cfg):
    """Remove parameters that cause the OpenAI library to crash."""
    allowed = {"temperature", "top_p", "stream", "stop", "max_tokens", "presence_penalty", "frequency_penalty"}
    return {k: v for k, v in cfg.items() if k in allowed}

async def check_searxng():
    """Verify SearXNG reachability and attempt auto-start via Docker if down."""
    from skills.web import get_searxng_url
    import requests
    url = get_searxng_url()
    
    async def try_ping():
        try:
            # specifically test the JSON format since that's what we need
            response = await asyncio.to_thread(
                requests.get, f"{url}/search", params={"q": "ping", "format": "json"}, timeout=2
            )
            return response.status_code == 200
        except:
            return False

    if await try_ping():
        return

    print(f"SearXNG not reachable at {url}. Attempting auto-start via Docker...")
    try:
        # Check if container exists
        check_proc = await asyncio.create_subprocess_exec(
            "docker", "ps", "-a", "--filter", "name=searxng", "--format", "{{.Names}}",
            stdout=asyncio.subprocess.PIPE
        )
        stdout, _ = await check_proc.communicate()
        container_exists = "searxng" in stdout.decode().strip()

        if container_exists:
            tlog("SearXNG container found. Verifying compatibility...")
            # Try to start it first
            await (await asyncio.create_subprocess_exec("docker", "start", "searxng")).wait()
            
            # Test if it returns 403
            if not await try_ping():
                tlog("SearXNG exists but is unreachable or misconfigured. Re-creating...")
                await (await asyncio.create_subprocess_exec("docker", "rm", "-f", "searxng")).wait()
                container_exists = False

        if not container_exists:
            tlog("Creating fresh SearXNG instance with volume-mounted settings.yml...")
            # Use absolute path for mounting
            config_dir = os.path.join(os.path.dirname(__file__), "searxng")
            proc = await asyncio.create_subprocess_exec(
                "docker", "run", "-d", "--name", "searxng",
                "-p", "8888:8080",
                "-v", f"{config_dir}:/etc/searxng",
                "searxng/searxng"
            )
            await proc.wait()
        
        if proc.returncode == 0:
            tlog("SearXNG command successful. Waiting for initialization (5s)...")
            await asyncio.sleep(5)
            if await try_ping():
                tlog("SearXNG is now ONLINE.")
    except Exception as e:
        tlog(f"SearXNG auto-start failure: {e}")

def _load_system_prompt() -> str:
    prompt_path = os.path.join(os.path.dirname(__file__), "config", "system_prompt.txt")
    try:
        with open(prompt_path, "r", encoding="utf-8") as f:
            return f.read()
    except Exception as e:
        global_logger.log_message("system", f"[main] Failed to load system_prompt.txt: {e}")
        return "You are Aether Core, the intelligence layer of the OpenAether desktop framework."

async def check_setup_status():
    """Check Ollama, Models, and Docker/SearXNG status."""
    status = {
        "ollama": False,
        "models": {
            MODEL: {"installed": False, "size": "9.1GB"},
            "translategemma:4b": {"installed": False, "size": "4.1GB"}
        },
        "docker": False,
        "searxng": False,
        "forced": FORCED_SETUP
    }

    # 1. Check Ollama & Models
    try:
        models_resp = ollama.list()
        status["ollama"] = True
        
        # Robust parsing for different ollama-python versions
        installed_names = []
        # models_resp can be a list or an object with a .models attribute
        raw_models = models_resp.get('models', []) if isinstance(models_resp, dict) else getattr(models_resp, 'models', [])
        
        for m in raw_models:
            # Handle both objects (with .model) and dicts (with ['name'] or ['model'])
            if hasattr(m, 'model'):
                installed_names.append(m.model)
            elif hasattr(m, 'name'):
                installed_names.append(m.name)
            elif isinstance(m, dict):
                installed_names.append(m.get('model') or m.get('name', ''))

        for m_name in status["models"]:
            # Check for exact or substring match (to handle version tags)
            if any(m_name == name or name.startswith(m_name) for name in installed_names):
                status["models"][m_name]["installed"] = True
    except Exception as e:
        print(f"Ollama check failed: {e}")

    # 2. Check Docker
    try:
        proc = await asyncio.create_subprocess_exec(
            "docker", "info", stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )
        await proc.communicate()
        status["docker"] = (proc.returncode == 0)
    except:
        pass

    # 3. Check SearXNG
    from skills.web import get_searxng_url
    import requests
    url = get_searxng_url()
    try:
        resp = await asyncio.to_thread(requests.get, f"{url}/search?q=ping", timeout=1)
        status["searxng"] = (resp.status_code == 200)
    except:
        pass

    return status

SYSTEM_PROMPT = _load_system_prompt()

# Core tools that are ALWAYS available
# JIT Philosophy: Keep this minimal to avoid context swamping.
CORE_TOOLS = {
    "discover_tools", 
    "web_search", 
    "report_error"
}

boot_log("Loading quick_dispatch...")
quick_dispatch.load()
boot_log("Loading Skills Registry...")
registry.load_skills()
boot_log("Backend Ready for Connections.")

def safe_next(gen):
    try:
        return next(gen)
    except StopIteration:
        return None

class AgentSession:
    def __init__(self, websocket):
        self.websocket = websocket
        self.messages = [{'role': 'system', 'content': SYSTEM_PROMPT}]
        self.current_task = None
        self.pending_approval = None
        self.interrupted = False
        self.reinitialized = False
        self.loaded_tools = set(CORE_TOOLS)
        self.llm_cfg = {
            'model': MODEL,
            'model_server': 'http://localhost:11434/v1',
            'api_key': 'EMPTY',
            'generate_cfg': filter_llm_params(SAMPLE_OPTIONS)
        }
        self.agent = None
        self.call_id_map = {} # (tool_name) -> last_call_id
        # Removed blocking self._reinit_agent() from here

    async def _reinit_agent(self):
        """Asynchronously initialize the agent to avoid blocking the event loop."""
        if Assistant:
            def _create():
                return Assistant(
                    llm=self.llm_cfg,
                    system_message=self.messages[0]['content'] if self.messages else SYSTEM_PROMPT,
                    function_list=list(self.loaded_tools)
                )
            
            # This call can take 10s+ in Ollama due to capability checks
            self.agent = await asyncio.to_thread(_create)
            self.reinitialized = True # Mark that the current generator is stale

    def _prune_tools(self, force=False):
        """Reset loaded tools if context is getting too large."""
        if force or len(self.messages) > 10:
            before = len(self.loaded_tools)
            self.loaded_tools = set(CORE_TOOLS)
            if len(self.loaded_tools) < before:
                global_logger.log_message("system", f"[JIT] Pruned context tools. Reset to {len(self.loaded_tools)} core tools.")
                # For pruning, we still need to reset the agent to clear the internal LLM state
                self.agent = None 

    async def prune_history(self, max_len=20):
        if len(self.messages) > max_len + 1:
            system_msg = self.messages[0]
            recent_msgs = self.messages[-(max_len):]
            self.messages = [system_msg] + recent_msgs
            # Force tool prune when history is compressed to ensure intent cleanliness
            self._prune_tools(force=True)

    async def send_json(self, data):
        await self.websocket.send(json.dumps(data))
        global_logger.log_event("server_to_client", data)

    async def update_status(self, status):
        await self.send_json({"type": "status_update", "status": status, "model": "Aether Core"})

    async def run(self):
        tlog(f"New session started for WebSocket {id(self.websocket)}")
        try:
            async for message in self.websocket:
                data = json.loads(message)
                global_logger.log_event("client_to_server", data)
                
                if data.get("type") == "user_message":
                    user_msg = data.get("content")
                    self.messages.append({'role': 'user', 'content': user_msg})
                    global_logger.log_message("user", user_msg)
                    
                    # --- QUICK DISPATCH (Spark) Layer ---
                    try:
                        spark_res = await quick_dispatch.dispatch(user_msg, registry)
                        if spark_res and spark_res.get("handled"):
                            if "response" in spark_res:
                                await self.send_json({"type": "agent_message", "content": spark_res["response"]})
                                self.messages.append({'role': 'assistant', 'content': spark_res["response"]})
                                await self.send_json({"type": "agent_message_done"})
                                continue # Continue listener loop instead of returning

                            tool = spark_res["tool"]
                            args = spark_res["args"]
                            if spark_res.get("pre_msg"):
                                await self.send_json({"type": "agent_message", "content": spark_res["pre_msg"]})
                            
                            tlog(f"[Spark] Executing Instant Tool: {tool}")
                            res = await registry.execute(tool, args)
                            
                            # Log and update history correctly
                            self.messages.append({
                                'role': 'assistant', 
                                'content': '', 
                                'tool_calls': [{'name': tool, 'arguments': json.dumps(args)}]
                            })
                            self.messages.append({
                                'role': 'tool',
                                'content': json.dumps(res, ensure_ascii=False),
                                'name': tool
                            })
                            
                            # Final follow-up text? Spark usually just finishes or hands off.
                            # For simplicity, we just wrap it up here.
                            await self.send_json({"type": "agent_message_done"})
                            continue # Continue listener loop instead of returning
                    except Exception as e:
                        tlog(f"Spark Error (non-fatal): {e}")

                    self.current_task = asyncio.create_task(self.process_loop())
                
                elif data.get("type") == "stop_request":
                    self.interrupted = True
                    if self.current_task and not self.current_task.done():
                        global_shell.interrupt()
                        self.current_task.cancel()
                        await self.update_status("idle")
                
                elif data.get("type") == "approval_response":
                    if self.pending_approval:
                        self.pending_approval["future"].set_result(data.get("approved", False))
                
                elif data.get("type") == "open_logs":
                    log_dir = global_logger.log_dir
                    logs = []
                    
                    def format_size(size):
                        for unit in ['B', 'KB', 'MB', 'GB']:
                            if size < 1024: return f"{size:.1f}{unit}"
                            size /= 1024
                        return f"{size:.1f}TB"

                    if os.path.exists(log_dir):
                        files = [os.path.join(log_dir, f) for f in os.listdir(log_dir) if f.endswith(".log")]
                        # Sort by mtime
                        files.sort(key=lambda x: os.path.getmtime(x), reverse=True)
                        
                        for f in files[:10]: # Return top 10
                            mtime = os.path.getmtime(f)
                            logs.append({
                                "name": os.path.basename(f),
                                "path": f,
                                "is_active": f == global_logger.log_file,
                                "size": format_size(os.path.getsize(f)),
                                "time": time.strftime("%Y-%m-%d %H:%M", time.localtime(mtime))
                            })
                    settings = load_settings()
                    await self.send_json({"type": "logs_data", "logs": logs, "settings": settings})

                elif data.get("type") == "update_settings":
                    save_settings(data.get("settings", {}))

                elif data.get("type") == "get_log_content":
                    file_path = data.get("path")
                    if file_path and os.path.exists(file_path):
                        with open(file_path, "r") as f:
                            content = f.read()
                        await self.send_json({"type": "log_content", "content": content, "path": file_path})

                elif data.get("type") == "delete_log_file":
                    file_path = data.get("path")
                    if file_path and os.path.exists(file_path) and file_path != global_logger.log_file:
                        from skills.system import delete_path
                        delete_path(file_path)
                        await self.send_json({"type": "log_deleted", "path": file_path})

                elif data.get("type") == "get_setup_status":
                    status = await check_setup_status()
                    await self.send_json({"type": "setup_status", "status": status})

                elif data.get("type") == "fix_docker":
                    # Attempt to start or re-init SearXNG
                    await check_searxng()
                    status = await check_setup_status()
                    await self.send_json({"type": "setup_status", "status": status})

                elif data.get("type") == "pull_model":
                    model_name = data.get("model")
                    if model_name:
                        try:
                            # Stream progressive status
                            progress = ollama.pull(model=model_name, stream=True)
                            for chunk in progress:
                                if self.interrupted: break
                                # Chunk format: {'status': '...', 'total': ..., 'completed': ...}
                                await self.send_json({
                                    "type": "pull_progress",
                                    "model": model_name,
                                    "status": chunk.get('status'),
                                    "percent": (chunk.get('completed', 0) / chunk.get('total', 1)) * 100 if chunk.get('total') else 0
                                })
                            
                            # Final status update
                            final_status = await check_setup_status()
                            await self.send_json({"type": "setup_status", "status": final_status})
                        except Exception as e:
                            await self.send_json({"type": "error", "content": f"Model pull failed: {e}"})

                elif data.get("type") == "upload_log_pastebin":
                    file_path = data.get("path")
                    api_key = data.get("api_key")
                    if file_path and os.path.exists(file_path):
                        result = await upload_to_pastebin(file_path, api_key)
                        await self.send_json({"type": "pastebin_result", "result": result, "path": file_path})
        except websockets.exceptions.ConnectionClosed:
            tlog("WebSocket connection closed normally.")
        except Exception as e:
            tlog(f"CRITICAL: Session listener crashed: {e}")
            import traceback
            traceback.print_exc()

    async def process_loop(self, depth=0, silent=False, initial_main="", initial_thought=""):
        if depth > 5:
            tlog("Depth limit reached. Stopping recursion.")
            return

        if not silent:
            await self.update_status("thinking")
            self.interrupted = False
            self.reinitialized = False
        
        tlog("Starting process_loop...")
        
        try:
            # 1. Prepare Context
            tlog("Preparing window context...")
            windows_context = []
            try:
                # Use the registry bridge to execute the tool safely
                res = await registry.execute("get_windows", {})
                if isinstance(res, list):
                    windows_context = res
            except Exception as e:
                tlog(f"Window context error (non-fatal): {e}")

            window_block = f"\n\n[ACTUAL OPEN WINDOWS]: {json.dumps(windows_context)}" if windows_context else ""
            
            # 2. Re-init Agent with updated system message
            tlog("Re-initializing Agent (Threaded)...")
            self.messages[0]['content'] = SYSTEM_PROMPT + window_block
            await self._reinit_agent()
            
            # Tracker for whether this SPECIFIC loop triggered a re-init
            triggered_reinit = False 
            self.reinitialized = False 
            
            if not self.agent:
                tlog("FATAL: Assistant initialization failed.")
                await self.send_json({"type": "error", "content": "Assistant initialization failed."})
                return

            # Pass ONLY user/assistant messages to run, as system prompt is in __init__
            current_history = self.messages[1:]
            
            # SLIDING WINDOW: Keep only the last 12 messages to prevent context bloat
            if len(current_history) > 12:
                tlog(f"Context Trimming: History length {len(current_history)} exceeded limit. Trimming to latest 12.")
                current_history = current_history[-12:]
                # Ensure we still start with a user message for Qwen compatibility
                if current_history[0].get('role') != 'user':
                    current_history = current_history[1:]

            # SEQUENCE GUARD: Ensure we start with a user message if we have history
            if len(current_history) > 0 and current_history[0].get('role') != 'user':
                tlog("Sequence Guard: Finding first user message...")
                while current_history and current_history[0].get('role') != 'user':
                    current_history.pop(0)
            
            tlog(f"History length for run(): {len(current_history)}")
            
            # Start Generator
            tlog("Requesting model generation (Ollama)...")
            response_generator = self.agent.run(messages=current_history)
            
            last_main_content = initial_main
            last_thought_content = initial_thought
            initial_count = len(current_history)
            seen_interactions = set() 
            last_heartbeat = time.time()
            # Initialize with current state so we always have a valid history to finalize
            last_history = list(current_history) 
            
            # 3. Stream Results
            while True:
                if self.interrupted: 
                    tlog("Interrupted by user.")
                    break
                
                # Use safe_next to prevent StopIteration RuntimeError in threads
                history = await asyncio.to_thread(safe_next, response_generator)
                
                # HEARTBEAT: Send a status update every 2 seconds to keep connection alive
                now = time.time()
                if now - last_heartbeat > 2.0:
                    await self.update_status("thinking") 
                    last_heartbeat = now

                if history is None:
                    tlog("Generation complete.")
                    break
                
                # Update last known state immediately
                last_history = history
                    
                # ANTI-SPIN: If history hasn't grown beyond our initial input, 
                # we are likely waiting for the first token. Sleep briefly to avoid CPU spin.
                if len(history) <= initial_count or history == current_history:
                    await asyncio.sleep(0.01)
                    continue
                
                # JIT BREAK: If the agent was re-initialized (new tools), 
                # we stop the current generator loop so we can finalize history and restart.
                if self.reinitialized:
                    tlog("Agent re-initialized mid-thought. Finalizing step before restart.")
                    triggered_reinit = True
                    break
                
                # 4. Handle Interleaved Content (Text & Tools)
                if len(history) > initial_count:
                    # m_idx is the global history index
                    for i, m in enumerate(history[initial_count:]):
                        m_idx = initial_count + i
                        
                        role = m.get('role')
                        
                        # A. Handle Assistant Text & Thoughts
                        if role == 'assistant':
                            content = m.get('content', '') or ""
                            # Extract thought vs main text
                            thought_match = re.search(r'<think>(.*?)(?:</think>|$)', content, re.DOTALL)
                            
                            # Thought Delta
                            if thought_match:
                                full_thought = thought_match.group(1).strip()
                                if full_thought.startswith(last_thought_content):
                                    thought_delta = full_thought[len(last_thought_content):]
                                    if thought_delta:
                                        await self.send_json({"type": "agent_thought_chunk", "content": thought_delta})
                                        last_thought_content = full_thought
                                main_content = re.sub(r'<think>.*?</think>', '', content, flags=re.DOTALL).strip()
                            else:
                                main_content = content.strip()

                            # Main Text Delta
                            if main_content.startswith(last_main_content) and len(main_content) > len(last_main_content):
                                msg_delta = main_content[len(last_main_content):]
                                if msg_delta:
                                    await self.send_json({"type": "agent_message", "content": msg_delta})
                                    last_main_content = main_content
                            elif main_content and not last_main_content:
                                # Backup for when startswith fails but we have content
                                await self.send_json({"type": "agent_message", "content": main_content})
                                last_main_content = main_content

                            # B. Handle Tool Calls within the assistant message
                            if 'function_call' in m and m_idx not in seen_interactions:
                                call = m['function_call']
                                if call.get('name'):
                                    tname = call.get('name')
                                    call_id = f"call_{tname}_{m_idx}"
                                    self.call_id_map[tname] = call_id
                                    
                                    tlog(f"Detected Tool Call: {tname} (ID: {call_id})")
                                    await self.update_status("executing")
                                    await self.send_json({
                                        "type": "tool_call",
                                        "name": tname,
                                        "args": call.get('arguments'),
                                        "call_id": call_id
                                    })
                                    seen_interactions.add(m_idx)
                        
                        # C. Handle Tool Outputs (Results)
                        elif role in ['function', 'tool'] and m_idx not in seen_interactions:
                            tname = m.get('name', 'unknown')
                            call_id = self.call_id_map.get(tname)
                            res_content = m.get('content', '{}')
                            
                            # JIT Hallucination Self-Correction
                            if "does not exists" in res_content and tname != 'unknown':
                                if registry.resolve_name(tname) in registry.tools:
                                    tlog(f"[JIT] Detected hallucinated tool: {tname}. Auto-loading...")
                                    self.loaded_tools.add(tname)
                                    await self._reinit_agent()
                                    seen_interactions.add(m_idx)
                                    continue

                            try:
                                res = json.loads(res_content)
                                tlog(f"Tool Result received for: {tname} (ID: {call_id})")
                                await self.send_json({
                                    "type": "tool_output",
                                    "name": tname,
                                    "output": res,
                                    "call_id": call_id
                                })
                                # JIT Discovery Logic
                                if isinstance(res, dict) and 'loaded_tools' in res:
                                    new_tools = [t for t in res['loaded_tools'] if t not in self.loaded_tools]
                                    if new_tools:
                                        self.loaded_tools.update(new_tools)
                                        await self._reinit_agent()
                            except Exception as e:
                                tlog(f"Error parsing tool output: {e}")
                            seen_interactions.add(m_idx)
                else:
                    await self.update_status("thinking")

            # 5. Finalize Session History
            # We ONLY finalize if we didn't just trigger a re-init (which will recurse)
            if not triggered_reinit and last_history:
                # DEBUG: Log the roles in the returned history
                roles = [m.get('role') for m in last_history]
                tlog(f"Generator history roles: {roles}")
                
                # Find new messages (beyond initial_count)
                new_msgs = last_history[initial_count:] if len(last_history) > initial_count else []
                
                # If new_msgs is empty but tokens were processed, qwen_agent might have internal state issues
                # with history slicing. As a fallback, try to find the last assistant message.
                if not new_msgs and len(last_history) > 0:
                   for m in reversed(last_history):
                       if m.get('role') == 'assistant' and m.get('content'):
                           tlog("Fallback: Found assistant message in last_history that was missed by slicing.")
                           new_msgs = [m]
                           break

                tlog(f"Finalizing history. Appending {len(new_msgs)} new messages.")
                for m in new_msgs:
                    role = m.get('role')
                    content = m.get('content', '')
                    tlog(f"New message role={role} len={len(content)}")
                    
                    # Sync check: don't double-append what's already there (merged assistant response)
                    if role == 'assistant' and self.messages and self.messages[-1].get('role') == 'assistant':
                        self.messages[-1] = m
                    else:
                        self.messages.append(m)
                    global_logger.log_message(role, m)

            # --- AUTO-RESUME TRIGGER ---
            if triggered_reinit and not self.interrupted:
                tlog(f"Self-Correcting: Auto-resuming after tool discovery (Depth: {depth+1})...")
                await self.process_loop(depth=depth+1, silent=True, initial_main=last_main_content, initial_thought=last_thought_content)
                if depth > 0: return

            if not silent:
                await self.send_json({"type": "agent_message_done"})
                await self.update_status("idle")
            
            tlog("Process loop finished successfully.")

        except asyncio.CancelledError:
            print("[TRACER] Session task cancelled.")
        except Exception as e:
            print(f"[FATAL ERROR] Agent Session Crash: {e}")
            import traceback
            traceback.print_exc()
            await self.send_json({"type": "error", "content": f"Agent crashed: {str(e)}"})
            await self.update_status("idle")

async def main():
    set_main_loop(asyncio.get_running_loop())
    # Run SearXNG check in background so it doesn't block server startup
    asyncio.create_task(check_searxng())
    async with websockets.serve(lambda ws: AgentSession(ws).run(), "localhost", PORT):
        await asyncio.Future()

if __name__ == "__main__":
    asyncio.run(main())
