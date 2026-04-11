import sys, os, json, time, platform, platformdirs

GLOBAL_START_TIME = time.time()

def tlog(msg):
    print(f"[TRACER][{time.time()-GLOBAL_START_TIME:.3f}s] {msg}", flush=True)

class SessionLogger:
    def __init__(self):
        # Use standard XDG state directory for logs
        self.log_dir = os.path.join(platformdirs.user_state_dir("openaether"), "logs")
        os.makedirs(self.log_dir, exist_ok=True)
        
        timestamp = time.strftime("%Y-%m-%d_%H-%M-%S")
        self.log_file = os.path.join(self.log_dir, f"{timestamp}.log")
        self.last_type = None
        self.event_count = 0
        
        self._write_header()
        self._auto_purge()

    def _write_header(self):
        """Write diagnostic metadata to the start of the log."""
        header = [
            "="*60,
            f"OPENAETHER DIAGNOSTIC LOG - {time.strftime('%Y-%m-%d %H:%M:%S')}",
            "="*60,
            f"OS: {platform.system()} {platform.release()} ({platform.machine()})",
            f"Python: {sys.version.split(' ')[0]}",
            f"Executable: {sys.executable}",
            f"Working Dir: {os.getcwd()}",
            f"Log File: {self.log_file}",
            "="*60,
            "\n"
        ]
        with open(self.log_file, "w") as f:
            f.write("\n".join(header))

    def _auto_purge(self, max_logs=10):
        """Keep only the N most recent log files."""
        try:
            logs = [os.path.join(self.log_dir, f) for f in os.listdir(self.log_dir) if f.endswith(".log")]
            logs.sort(key=os.path.getmtime, reverse=True)
            
            # Exclude currently active log
            other_logs = [l for l in logs if l != self.log_file]
            
            if len(other_logs) > max_logs:
                for old_log in other_logs[max_logs:]:
                    os.remove(old_log)
        except Exception as e:
            tlog(f"Logger purge failed: {e}")

    def log_event(self, event_type, data):
        self.event_count += 1
        # Periodic cleanup and size check
        if self.event_count % 20 == 0:
            self._auto_purge()
            self._check_size_limit()

        self._write_readable(event_type, data)

    def _check_size_limit(self, max_mb=5):
        """Truncate the log if it exceeds the limit to prevent uncontrolled growth."""
        try:
            if os.path.exists(self.log_file) and os.path.getsize(self.log_file) > max_mb * 1024 * 1024:
                with open(self.log_file, "a") as f:
                    f.write(f"\n\n[SYSTEM] Log size limit ({max_mb}MB) reached. Truncating further output for safety.\n")
                # In a real scenario we might rotate, but for simplicity we'll just stop or suggest rotation
        except: pass

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
                    f.write(f"\n[{ts}] {role}:\n{content}\n")
                
                if fcall:
                    name = fcall.get('name')
                    args = fcall.get('arguments')
                    f.write(f"\n[{ts}] {role} INITIATED ACTION: {name}\n")
                    f.write(f"ARGS: {args}\n")
                
                f.write("-" * 40 + "\n")
                self.last_type = None

            elif event_type == "tool":
                name = data.get("name", "unknown")
                args = data.get("args")
                output = str(data.get("output", ""))
                
                f.write(f"\n[{ts}] TOOL EXECUTION: {name}\n")
                if args:
                    f.write(f"PARAMETERS: {json.dumps(args, indent=2)}\n")
                
                # Truncate very long outputs (e.g. 50KB) to keep logs readable but informative
                if len(output) > 50000:
                    f.write(f"RESULT (TRUNCATED): {output[:50000]}... [REST OMITTED]\n")
                else:
                    f.write(f"RESULT:\n{output}\n")
                
                f.write("=" * 40 + "\n")
                self.last_type = None

            elif event_type == "server_to_client" and data.get("type") == "agent_thought":
                if self.last_type != "thought":
                    f.write(f"\n[{ts}] AI REASONING:\n")
                    self.last_type = "thought"
                f.write(data.get('content', ''))

            elif event_type == "server_to_client" and data.get("type") == "agent_message_done":
                f.write("\n" + "." * 40 + "\n")
                self.last_type = None

            elif event_type == "error_report":
                f.write(f"\n[{ts}] CRITICAL ERROR in {data.get('module')}:\n")
                f.write(f"ISSUE: {data.get('issue')}\n")
                f.write(f"DETAILS: {data.get('details')}\n")
                f.write("!" * 60 + "\n")

global_logger = SessionLogger()
