import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'lib'))

import json
import time
import platformdirs

class SessionLogger:
    def __init__(self):
        self.log_dir = os.path.join(platformdirs.user_log_dir("openaether"), "logs")
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
                f.write(f"[{ts}] {role}: {data.get('content')}\n\n")
                self.last_type = None
            elif event_type == "tool":
                f.write(f"[{ts}] TOOL_CALL: {data.get('name')}\n")
                f.write(f"[{ts}] ARGUMENTS: {json.dumps(data.get('args'), indent=2)}\n")
                out = str(data.get("output"))
                f.write(f"[{ts}] TOOL_RESULT (Raw): {out[:2000]}...\n\n")
                self.last_type = None
            elif event_type == "error_report":
                f.write(f"[{ts}] !!! ERROR_REPORT !!!\n")
                f.write(f"[{ts}] MODULE: {data.get('module')}\n")
                f.write(f"[{ts}] ISSUE: {data.get('issue')}\n")
                f.write(f"[{ts}] DETAILS: {data.get('details')}\n\n")
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
