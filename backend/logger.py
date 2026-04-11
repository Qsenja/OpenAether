import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'lib'))

import json
import time
import platformdirs
import time

GLOBAL_START_TIME = time.time()

def tlog(msg):
    print(f"[TRACER][{time.time()-GLOBAL_START_TIME:.3f}s] {msg}", flush=True)

class SessionLogger:
    def __init__(self):
        # Relocate logs to a local folder within the project for easier access
        self.log_dir = os.path.join(os.path.dirname(__file__), "logs")
        if not os.path.exists(self.log_dir):
            os.makedirs(self.log_dir, exist_ok=True)
        
        timestamp = time.strftime("%Y-%m-%d_%H-%M-%S")
        self.log_file = os.path.join(self.log_dir, f"{timestamp}.log")
        self.last_type = None
        self._auto_purge()

    def _auto_purge(self, max_logs=5):
        """Keep only the N most recent log files."""
        try:
            logs = [os.path.join(self.log_dir, f) for f in os.listdir(self.log_dir) if f.endswith(".log")]
            # Sort by filename descending (since filenames overlap with timestamps)
            logs.sort(reverse=True)
            
            # Exclude currently active log from deletion
            current_log = os.path.basename(self.log_file)
            other_logs = [l for l in logs if os.path.basename(l) != current_log]
            
            if len(other_logs) > max_logs:
                for old_log in other_logs[max_logs:]:
                    os.remove(old_log)
        except Exception as e:
            print(f"Logger purge failed: {e}")

    def log_event(self, event_type, data):
        entry = {
            "timestamp": time.time(),
            "type": event_type,
            "data": data
        }
        self._write_readable(event_type, data)

    def log_message(self, role, content):
        self.log_event("message", {"role": role, "content": content})

    def log_tool(self, name, args, output):
        self.log_event("tool", {"name": name, "args": args, "output": output})

    def log_error_report(self, module, issue, details):
        self.log_event("error_report", {"module": module, "issue": issue, "details": details})

    def _write_readable(self, event_type, data):
        ts = time.strftime("%H:%M:%S")
        with open(self.log_file, "a") as f:
            if event_type == "message":
                role = data.get("role", "unknown").upper()
                content = data.get("content")
                fcall = data.get("function_call")
                
                if content:
                    f.write(f"[{ts}] {role}: {content}\n\n")
                
                if fcall:
                    f.write(f"[{ts}] {role} (ACTION): {fcall.get('name')}({fcall.get('arguments')})\n\n")
                
                self.last_type = None

            elif event_type == "tool":
                # redundant but kept for safety if log_tool is called directly
                f.write(f"[{ts}] TOOL_RESULT: {data.get('name')}\n")
                out = str(data.get("output"))
                f.write(f"[{ts}] DATA: {out[:3000]}\n\n")
                self.last_type = None
            elif event_type == "server_to_client" and data.get("type") == "agent_thought":
                # Only write header if this is a NEW thought stream
                if self.last_type != "thought":
                    f.write(f"[{ts}] THOUGHT: ")
                    self.last_type = "thought"
                f.write(data.get('content'))
            elif event_type == "server_to_client" and data.get("type") == "agent_message_done":
                f.write("\n")
                self.last_type = None

global_logger = SessionLogger()
