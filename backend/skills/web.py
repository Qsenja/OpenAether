import requests
import json
import os
import re
import socket
import asyncio
import shutil
from bs4 import BeautifulSoup
from registry import registry
from logger import global_logger
from shell_manager import global_shell
import webbrowser
from logger import tlog

# --- WEB SEARCH & BROWSING ---
def get_searxng_url():
    """Read searxng_url from config or fallback to localhost."""
    try:
        config_path = os.path.expanduser("~/.config/openaether/config.json")
        if os.path.exists(config_path):
            with open(config_path, 'r') as f:
                return json.load(f).get("searxng_url", "http://localhost:8888")
    except:
        pass
    return "http://localhost:8888"

@registry.register(
    "aether_search",
    "Search the internet for external facts, news, or weather. DO NOT use this for local system information (installed packages, software versions) if local tools like 'get_software_version' are available.",
    {"type":"object", "properties":{"query":{"type":"string"}}, "required":["query"]}
)
async def aether_search(query: str):
    base_url = get_searxng_url()
    try:
        r = requests.get(f"{base_url}/search", params={"q": query, "format": "json"}, timeout=10)
        results = r.json().get("results", [])[:3]
        
        combined = []
        for res in results:
            url = res.get("url")
            combined.append(f"--- {res.get('title')} ({url}) ---\n{res.get('content')}")
        return {"status": "success", "content": "\n\n".join(combined)}
    except Exception as e: return {"status": "error", "message": str(e)}

@registry.register("fetch_url", "Fetch text from a URL.", {"type":"object", "properties":{"url":{"type":"string"}}, "required":["url"]})
def fetch_url(url: str):
    # JIT Guard: Detect local paths and redirect AI
    if url.startswith("/") or url.startswith("./") or url.startswith("~/"):
        return {
            "status": "error",
            "message": (
                f"ERROR: '{url}' appears to be a local filesystem path. "
                "fetch_url is ONLY for web URLs (http/https). "
                "For local files, you MUST use 'read_file' or call discover_tools('file') to find the correct tool."
            )
        }
    
    try:
        r = requests.get(url, timeout=10)
        soup = BeautifulSoup(r.text, "html.parser")
        for s in soup(["script", "style"]): s.decompose()
        return soup.get_text()[:5000]
    except Exception as e: return str(e)

# --- NETWORKING ---
@registry.register("scan_network", "Scan local network for active devices.", {})
async def scan_network():
    if not shutil.which("nmap"): return {"status": "error", "message": "nmap missing"}
    res = await global_shell.execute("nmap -sn 192.168.1.0/24")
    return {"status": "success", "output": res.get("output")}

@registry.register("get_wifi_info", "Get current SSID and signal strength.", {})
async def get_wifi_info():
    if shutil.which("nmcli"):
        res = await global_shell.execute("nmcli -t -f active,ssid,signal dev wifi")
        for line in res.get("output", "").splitlines():
            if line.startswith("yes:"): return {"status": "success", "info": line}
    return {"status": "error", "message": "No active WiFi detected."}

@registry.register("check_port", "Check if a TCP port is open.", {"type":"object", "properties":{"host":{"type":"string"},"port":{"type":"integer"}}, "required":["host","port"]})
async def check_port(host, port):
    try:
        with socket.create_connection((host, port), timeout=3):
            return {"status": "success", "open": True}
    except: return {"status": "success", "open": False}

@registry.register("ssh_command", "Run a command on a remote host via SSH.", {"type":"object", "properties":{"host":{"type":"string"},"user":{"type":"string"},"command":{"type":"string"}}, "required":["host","user","command"]})
async def ssh_command(host, user, command):
    cmd = f"ssh -o BatchMode=yes {user}@{host} {command}"
    return await global_shell.execute(cmd)

@registry.register("get_device_info", "Deep scan a specific IP (ports/OS).", {"type":"object", "properties":{"ip":{"type":"string"}}, "required":["ip"]})
async def get_device_info(ip: str):
    return await global_shell.execute(f"pkexec nmap -sV -O {ip}")

@registry.register(
    "open_website",
    "Open a specific URL in the system's default web browser.",
    {"type":"object", "properties":{"url":{"type":"string"}}, "required":["url"]}
)
async def open_website(url: str):
    """Opens a website using the system's default browser."""
    try:
        # Prepend https:// if missing schema
        if not re.match(r'^[a-z]+://', url):
            url = "https://" + url
        
        # open() is non-blocking on most platforms as it spawns a process
        # We wrap in a thread just to be safe if the OS handler is slow.
        await asyncio.to_thread(webbrowser.open, url)
        tlog(f"Opening website Action: {url}")
        return {"status": "success", "message": f"Opening {url} in system browser."}
    except Exception as e:
        return {"status": "error", "message": str(e)}
