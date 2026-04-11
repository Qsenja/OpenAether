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
    "frequency_penalty": 1.1, # Renamed from repeat_penalty for OpenAI-compat
    "stop": ["<|im_start|>", "<|im_end|>", "/think", "/no_think"]
}

async def check_searxng():
    """Verify SearXNG reachability and attempt auto-start via Docker if down."""
    from skills.web import get_searxng_url
    import requests
    url = get_searxng_url()
    
    async def try_ping():
        try:
            response = requests.get(f"{url}/search?q=ping", timeout=2)
            return response.status_code == 200
        except:
            return False

    if await try_ping():
        return

    print(f"SearXNG not reachable at {url}. Attempting auto-start via Docker...")
    try:
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
    except Exception as e:
        print(f"Docker auto-start error: {e}")

def _load_system_prompt() -> str:
    prompt_path = os.path.join(os.path.dirname(__file__), "config", "system_prompt.txt")
    try:
        with open(prompt_path, "r", encoding="utf-8") as f:
            return f.read()
    except Exception as e:
        global_logger.log_message("system", f"[main] Failed to load system_prompt.txt: {e}")
        return "You are Aether Core, the intelligence layer of the OpenAether desktop framework."

SYSTEM_PROMPT = _load_system_prompt()

# Core tools that are ALWAYS available
# JIT Philosophy: Keep this minimal to avoid context swamping.
CORE_TOOLS = {
    "discover_tools", 
    "aether_search", 
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
            'generate_cfg': SAMPLE_OPTIONS
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
        try:
            async for message in self.websocket:
                data = json.loads(message)
                global_logger.log_event("client_to_server", data)
                
                if data.get("type") == "user_message":
                    user_msg = data.get("content")
                    self.messages.append({'role': 'user', 'content': user_msg})
                    global_logger.log_message("user", user_msg)
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
                    if os.path.exists(log_dir):
                        files = [os.path.join(log_dir, f) for f in os.listdir(log_dir) if f.endswith(".log")]
                        files.sort(key=lambda x: (os.path.getmtime(x), x), reverse=True)
                        for f in files[:5]:
                            logs.append({"name": os.path.basename(f), "path": f, "is_active": f == global_logger.log_file})
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

                elif data.get("type") == "upload_log_pastebin":
                    file_path = data.get("path")
                    api_key = data.get("api_key")
                    if file_path and os.path.exists(file_path):
                        result = await upload_to_pastebin(file_path, api_key)
                        await self.send_json({"type": "pastebin_result", "result": result, "path": file_path})
        except websockets.exceptions.ConnectionClosed:
            pass

    async def process_loop(self, depth=0, silent=False):
        if depth > 3:
            tlog("Max recursion depth reached for JIT restarts.")
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
            
            # CRITICAL FIX: Reset the reinitialized flag AFTER setup. 
            # Otherwise, the loop breaks on the very first token.
            self.reinitialized = False 
            
            if not self.agent:
                tlog("FATAL: Assistant initialization failed.")
                await self.send_json({"type": "error", "content": "Assistant initialization failed."})
                return

            # Pass ONLY user/assistant messages to run, as system prompt is in __init__
            current_history = self.messages[1:]
            
            # SEQUENCE GUARD: Ensure we start with a user message
            if current_history and current_history[0].get('role') != 'user':
                tlog("Sequence Guard: Removing leading non-user message from history.")
                current_history = [m for m in current_history if m.get('role') != 'assistant']
            
            tlog(f"History length for run(): {len(current_history)}")
            
            # Start Generator
            tlog("Requesting model generation (Ollama)...")
            response_generator = self.agent.run(messages=current_history)
            
            last_history = []
            last_main_content = ""
            last_thought_content = ""
            initial_count = len(current_history)
            seen_interactions = set() # Track hash of (role, name, content) to prevent duplicates
            
            # 3. Stream Results
            while True:
                if self.interrupted: 
                    tlog("Interrupted by user.")
                    break
                
                # Use safe_next to prevent StopIteration RuntimeError in threads
                history = await asyncio.to_thread(safe_next, response_generator)
                if history is None:
                    tlog("Generation complete.")
                    break
                    
                last_history = history
                if not history: continue
                
                latest = history[-1]
                
                # Update text deltas for assistant responses
                if latest['role'] == 'assistant':
                    content = latest.get('content', '') or ""
                    # Extract thought vs main text
                    thought_match = re.search(r'<think>(.*?)(?:</think>|$)', content, re.DOTALL)
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
                    if main_content.startswith(last_main_content):
                        msg_delta = main_content[len(last_main_content):]
                        if msg_delta:
                            await self.send_json({"type": "agent_message", "content": msg_delta})
                            last_main_content = main_content
                    elif main_content and not last_main_content:
                        await self.send_json({"type": "agent_message", "content": main_content})
                    last_main_content = main_content
                
                # JIT BREAK: If the agent was re-initialized (new tools), 
                # we stop the current generator loop so we can finalize history and restart.
                if self.reinitialized:
                    tlog("Agent re-initialized mid-thought. Finalizing step before restart.")
                    break
                
                # 4. Handle Tool Interactions
                if len(history) > initial_count:
                    # m_idx is the global history index
                    for i, m in enumerate(history[initial_count:]):
                        m_idx = initial_count + i
                        if m_idx in seen_interactions:
                            continue
                        
                        # Handle Tool Calls
                        if 'function_call' in m:
                            call = m['function_call']
                            if call.get('name'):
                                tname = call.get('name')
                                # Generate a semi-unique ID based on name and count or index
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
                        
                        # Handle Tool Outputs (Results)
                        if m.get('role') == 'function':
                            try:
                                tname = m.get('name', 'unknown')
                                call_id = self.call_id_map.get(tname)
                                res_content = m.get('content', '{}')
                                res = json.loads(res_content)
                                tlog(f"Tool Result received for: {tname} (ID: {call_id})")
                                await self.send_json({
                                    "type": "tool_output",
                                    "name": tname,
                                    "output": res,
                                    "call_id": call_id
                                })
                                # JIT Discovery Logic: Re-init to update prompt
                                if isinstance(res, dict) and 'loaded_tools' in res:
                                    new_tools = [t for t in res['loaded_tools'] if t not in self.loaded_tools]
                                    if new_tools:
                                        tlog(f"Loading new tools: {new_tools}")
                                        self.loaded_tools.update(new_tools)
                                        await self._reinit_agent()
                                        global_logger.log_message("system", f"[JIT] Discovered new tools: {new_tools}")
                            except Exception as e:
                                tlog(f"Error parsing tool output: {e}")
                            seen_interactions.add(m_idx)
                else:
                    await self.update_status("thinking")

            # 5. Finalize Session History
            # IMPORTANT: We only append/merge the NEW parts of history to keep self.messages valid
            if last_history:
                # Find where the response starts (the part after what we sent in)
                if len(last_history) > initial_count:
                    new_msgs = last_history[initial_count:]
                else:
                    # If it doesn't contain inputs, take everything as new
                    new_msgs = [m for m in last_history if m.get('role') != 'system']
                
                tlog(f"Finalizing history. Appending {len(new_msgs)} new messages.")
                for m in new_msgs:
                    self.messages.append(m)
                    global_logger.log_message(m.get('role'), m)

            # --- AUTO-RESUME TRIGGER ---
            if self.reinitialized and not self.interrupted:
                tlog(f"Self-Correcting: Auto-resuming with new tools (Depth: {depth+1})...")
                await self.process_loop(depth=depth+1, silent=True)
                return

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
    await check_searxng()
    async with websockets.serve(lambda ws: AgentSession(ws).run(), "localhost", PORT):
        await asyncio.Future()

if __name__ == "__main__":
    asyncio.run(main())
