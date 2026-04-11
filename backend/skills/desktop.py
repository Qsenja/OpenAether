import json
import asyncio
import subprocess
import base64
import os
import re
from registry import registry
from shell_manager import global_shell

# --- HYPRLAND CONTROLS ---
async def _hypr(cmd: str) -> dict: return await global_shell.execute(f"hyprctl {cmd}")

@registry.register("get_workspaces", "List all Hyprland workspaces.", {})
async def get_workspaces():
    from hypr_env import HYPRLAND_ENV
    out = subprocess.check_output(["hyprctl", "workspaces", "-j"], text=True, env=HYPRLAND_ENV)
    return {"status": "success", "workspaces": json.loads(out)}

@registry.register("switch_workspace", "Switch to workspace number.", {"type":"object", "properties":{"number":{"type":"integer"}}, "required":["number"]})
async def switch_workspace(number: int):
    await _hypr(f"dispatch workspace {number}")
    return {"status": "success"}

@registry.register("move_window_to_workspace", "Move focus window to workspace.", {"type":"object", "properties":{"number":{"type":"integer"}}, "required":["number"]})
async def move_window_to_workspace(number: int):
    await _hypr(f"dispatch movetoworkspacesilent {number}")
    return {"status": "success"}

@registry.register("get_windows", "Get all open windows.", {})
async def get_windows():
    res = await _hypr("clients -j")
    clients = json.loads(res.get("output", "[]"))
    return [{"title": c.get("title"), "class": c.get("class"), "address": c.get("address")} for c in clients]

# --- GUI INTERACTION & OCR ---
@registry.register("click", "Click at (x, y).", {"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"}},"required":["x","y"]})
async def click(x, y):
    await _hypr(f"dispatch movecursor {x} {y}")
    subprocess.run(["ydotool", "click", "1"])
    return {"status": "success"}

@registry.register("type_text", "Type text via virtual keyboard.", {"type":"object","properties":{"text":{"type":"string"}},"required":["text"]})
async def type_text(text):
    subprocess.run(["ydotool", "type", text])
    return {"status": "success"}

# @registry.register("take_screenshot", "Capture screen and return base64.", {})
# async def take_screenshot():
#     path = "/tmp/screenshot.png"
#     subprocess.run(["grim", path], check=True)
#     with open(path, "rb") as f:
#         data = base64.b64encode(f.read()).decode()
#     os.remove(path)
#     return {"status": "success", "image_base64": data}

# @registry.register("find_on_screen", "Find text on screen using OCR.", {"type":"object","properties":{"text":{"type":"string"}},"required":["text"]})
# async def find_on_screen(text: str):
#     # This is a simplified placeholder of the logic in interaction.py
#     # In a real merge, I'd copy the full OCR clustering logic.
#     return {"status": "error", "message": "OCR clustering logic requires full migration from interaction.py (omitted for brevity in template)."}

# --- MEDIA & PERIPHERALS ---
@registry.register("set_volume", "Set volume (0-100).", {"type":"object","properties":{"percent":{"type":"integer"}},"required":["percent"]})
async def set_volume(percent: int):
    await global_shell.execute(f"pamixer --set-volume {percent}")
    return {"status": "success"}

@registry.register("send_notification", "Send desktop notification.", {"type":"object","properties":{"title":{"type":"string"},"message":{"type":"string"}},"required":["title","message"]})
async def send_notification(title, message):
    subprocess.run(["notify-send", title, message])
    return {"status": "success"}

@registry.register("play_audio", "Play audio file.", {"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})
async def play_audio(path: str):
    p = os.path.expanduser(path)
    subprocess.Popen(["mpv", "--no-video", p], start_new_session=True)
    return {"status": "success"}

@registry.register("clipboard", "Get or set clipboard.", {"type":"object","properties":{"text":{"type":"string"}}})
async def clipboard(text: str = None):
    if text:
        process = subprocess.Popen(["wl-copy"], stdin=subprocess.PIPE)
        process.communicate(input=text.encode())
        return {"status": "success", "message": "Set clipboard."}
    else:
        out = subprocess.check_output(["wl-paste"], text=True)
        return {"status": "success", "content": out.strip()}
