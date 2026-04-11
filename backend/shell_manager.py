import os
import pty
import subprocess
import asyncio
import time
import re
import fcntl

class ShellManager:
    def __init__(self):
        self.master_fd, self.slave_fd = pty.openpty()
        # Set master_fd to non-blocking
        fl = fcntl.fcntl(self.master_fd, fcntl.F_GETFL)
        fcntl.fcntl(self.master_fd, fcntl.F_SETFL, fl | os.O_NONBLOCK)
        
        from hypr_env import HYPRLAND_ENV
        self.process = subprocess.Popen(
            ["bash", "--noprofile", "--norc"],
            stdin=self.slave_fd,
            stdout=self.slave_fd,
            stderr=self.slave_fd,
            close_fds=True,
            env={**HYPRLAND_ENV, "PS1": "", "PS2": ""},
            preexec_fn=os.setsid
        )
        os.close(self.slave_fd)
        
        # Initial wait and prompt suppression
        os.write(self.master_fd, b"export PS1=''; export PS2=''; stty -echo\n")
        self._sync_read_init()

    def _sync_read_init(self):
        # Initial sync read to clear prompt, used only in __init__
        time.sleep(0.5)
        try:
            os.read(self.master_fd, 8192)
        except:
            pass

    async def _read_async(self, timeout=0.1):
        output = b""
        start = time.time()
        while time.time() - start < timeout:
            try:
                data = os.read(self.master_fd, 8192)
                if not data:
                    break
                output += data
            except (OSError, BlockingIOError):
                await asyncio.sleep(0.01)
                continue
        return output.decode(errors='replace')

    def _strip_ansi(self, text):
        ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
        return ansi_escape.sub('', text)

    async def execute(self, command, timeout=30):
        sentinel = f"COMMAND_FINISHED_{int(time.time())}"
        full_command = f"{command}; echo {sentinel}\n"
        
        os.write(self.master_fd, full_command.encode())
        
        output = ""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            chunk = await self._read_async(timeout=0.2)
            output += chunk
            
            clean_output = self._strip_ansi(output).strip()
            
            # Password detection
            if re.search(r"password.*:\s*$", clean_output, re.IGNORECASE):
                return {
                    "status": "password_required",
                    "output_so_far": clean_output,
                    "command": command
                }
            
            # Bypass terminal echo: wait for the sentinel to be PRINTED, not just ECHOED
            if output.count(sentinel) >= 2:
                break
            
            # Allow other tasks to run
            await asyncio.sleep(0.05)
        
        # Final cleaning
        cleaned = self._strip_ansi(output)
        
        # Remove the command echo (first line usually)
        cmd_strip = self._strip_ansi(full_command).strip()
        cleaned = cleaned.replace(cmd_strip, "")
        
        # Remove sentinel and trailing prompts
        cleaned = cleaned.replace(sentinel, "").strip()
        
        # Strip carriage returns and excessive whitespace
        cleaned = cleaned.replace("\r", "").strip()
        
        return {
            "status": "success",
            "output": cleaned
        }

    def interrupt(self):
        """Send Ctrl+C to the shell."""
        os.write(self.master_fd, b"\x03")

    def send_input(self, text):
        os.write(self.master_fd, f"{text}\n".encode())

    async def get_cwd(self):
        res = await self.execute("pwd")
        return res.get("output", "/")

    def __del__(self):
        try:
            self.process.terminate()
            os.close(self.master_fd)
        except:
            pass

global_shell = ShellManager()
