import re
import yaml
import os
from logger import global_logger

# List of words that should be ignored when counting 'semantic' tokens
FILLER_WORDS = {
    "bitte", "einmal", "mal", "jetzt", "schnell", "kurz", "gerade", "einfach",
    "please", "quickly", "briefly", "just", "open", "launch", "starte", "öffne",
    "in", "auf", "an", "nach", "zu", "für", "mit", "von", "aus", "bei",
    "a", "the", "an", "and", "und", "oder", "or"
}

# Regex to detect complexity markers that should force AI reasoning
# Matches:
# - Workspace switching: "in wkX", "auf workspace X"
# - Sequential tasks: "and then", "und dann"
# - Temporal markers (Scheduling): "in 5 min", "nach 10 sekunden", "um 12 uhr"
COMPLEXITY_REGEX = re.compile(
    r'\b(in|auf|zu|nach)\s+(wk|workspace|arbeitsbereich)\b|'
    r'\b(and\s+then|und\s+dann|danach)\b|'
    r'\b(in|nach|after|um|at)\s+\d+\s*(m|min|minuten|minutes|s|sec|sekunden|seconds|h|stunden|hours)\b', 
    re.IGNORECASE
)

_config = None

def load():
    """Public interface to trigger config loading (called by main.py)."""
    return _load_config()

def _load_config():
    global _config
    cfg_path = os.path.join(os.path.dirname(__file__), "config", "commands.yaml")
    try:
        with open(cfg_path, 'r') as f:
            _config = yaml.safe_load(f)
    except Exception as e:
        print(f"Error loading Spark config: {e}")
        _config = {"fixed_commands": [], "patterns": {}}
    return _config

def _get_pre_msg(tool, args):
    """Generates a friendly user message before execution."""
    if tool == "open_app":
        return f"Starting {args.get('command')}..."
    if tool == "kill_process":
        return f"Terminating {args.get('target')}..."
    if tool == "switch_workspace":
        return f"Switching to Workspace {args.get('number')}..."
    if tool == "web_search":
        return f"Searching for '{args.get('query')}' on the web..."
    return "I understand, I'm on it..."

def _resolve_target(value, context):
    """Simple context resolution (e.g. 'it' -> last app)"""
    if str(value).lower() in ("it", "das", "die", "app"):
        for item in reversed(context):
            if item.get("type") == "app":
                return item.get("name")
    return value

async def dispatch(message: str, registry, context: list = None) -> dict | None:
    """
    Identifies if a command can be handled by Spark.
    Returns tool name, args, and pre-execution message.
    """
    cfg = _config or _load_config()
    msg_raw = message.strip()
    
    # SYSTEM GUARD: Ignore any message starting with '(' or containing 'System Note'
    # This prevents Spark from accidentally triggering on internal nudge/handover logic.
    if msg_raw.startswith("(") or "System Note" in msg_raw:
        return None

    # Aggressive stripping: remove all trailing/leading punctuation AND whitespace
    msg = msg_raw.lower().strip(".,!? ").strip()
    context = context or []
    
    # -- PRIORITY 1: FIXED COMMANDS (Instant Answers/Tools) --
    # Check these first so that greetings are intercepted even if they have question marks.
    for fixed in cfg.get("fixed_commands", []):
        for kw in fixed.get("keywords", []):
            if msg == kw or msg.startswith(kw + " "):
                # Support direct text response (e.g. for greetings)
                if "response" in fixed:
                    global_logger.log_message("system", f"[quick_dispatch] Match (Greeting): {kw}")
                    return {"handled": True, "response": fixed["response"]}
                
                tool = fixed["tool"]
                args = fixed.get("args", {})
                pre_msg = _get_pre_msg(tool, args)
                global_logger.log_message("system", f"[quick_dispatch] Match (Fixed): {kw} -> {tool}")
                return {"handled": True, "tool": tool, "args": args, "pre_msg": pre_msg}

    # -- PRIORITY 2: CONVERSATIONAL HAND-OFF --
    CONVERSATIONAL_WORDS = {
        "test", "hi", "hello", "hallo", "hey", "ok", "okay", "ja", "nein", "yes", "no",
        "danke", "thanks", "bitte", "please", "stop", "quit", "check", "ping"
    }
    
    # Hand off deep questions ("who", "why", "how") to AI Core
    # But ONLY if they didn't match a fixed greeting above.
    QUESTION_WORDS = {"wie", "was", "wo", "warum", "wann", "welche", "welcher", "welches", "who", "what", "where", "why", "when", "which", "how"}
    
    if "?" in msg_raw or msg in CONVERSATIONAL_WORDS or any(w in msg.split() for w in QUESTION_WORDS):
        global_logger.log_message("system", f"[quick_dispatch] Hand-off: Conversational or Question detected ('{msg}')")
        return None

    # 2. Complexity Check (Hardened)
    tokens = [t.strip() for t in msg.split() if t.strip()]
    
    # Hand off extremely long sentences or complex structures
    if len(tokens) > 12 or COMPLEXITY_REGEX.search(msg):
        global_logger.log_message("system", f"[quick_dispatch] Hand-off: Complexity/Length detected")
        return None

    # 4. Pattern Detection (Flexible Search)
    patterns = cfg.get("patterns", {})
    
    for pat_name, pat in patterns.items():
        triggers = pat.get("triggers", [])
        
        found_trigger = None
        for trigger in triggers:
            # Match trigger only as whole word
            if re.search(rf'\b{re.escape(trigger)}\b', msg):
                found_trigger = trigger
                break
        
        if found_trigger:
            # Strategy: Strip the trigger and all common fillers to find the core 'target'
            # Use regex for case-insensitive, whole-word filler stripping
            clean_msg = msg
            # Remove the found trigger first
            clean_msg = re.sub(rf'\b{re.escape(found_trigger)}\b', '', clean_msg, count=1).strip()
            
            # Remove all filler words
            for filler in FILLER_WORDS:
                clean_msg = re.sub(rf'\b{re.escape(filler)}\b', '', clean_msg, flags=re.IGNORECASE).strip()
            
            # Remove punctuation
            clean_msg = re.sub(r'[.,!?]', '', clean_msg).strip()
            
            if not clean_msg:
                global_logger.log_message("system", f"[quick_dispatch] Abort: Trigger '{found_trigger}' found but no content remains.")
                continue

            # -- SLACK CHECK / OVERFLOW --
            # We allow multiple words now if they look like a single entity (e.g. "Google Chrome")
            # But we hand off if there are genuine 'junction' words like "and", "then", or punctuation separation.
            if any(j in clean_msg for j in [" and ", " und ", " then ", " dann ", " showing ", " zeige "]):
                 global_logger.log_message("system", f"[quick_dispatch] Hand-off: Secondary junction found in content '{clean_msg}'.")
                 return None

            try:
                tool = pat.get("tool")
                arg_cfg = pat.get("arg")
                
                if arg_cfg.get("type") == "integer":
                    num_match = re.search(r'\d+', clean_msg)
                    if not num_match: 
                        global_logger.log_message("system", f"[quick_dispatch] Abort: Pattern '{pat_name}' needs an integer but none found.")
                        continue
                    value = int(num_match.group(0))
                elif arg_cfg.get("type") == "word":
                    # Take only the first word/token for 'word' type (safety against sentence-swallowing)
                    value = clean_msg.split()[0] if clean_msg.split() else ""
                else:
                    value = clean_msg # Use the whole cleaned string for other types (allows spaces if requested)

                if not value:
                    global_logger.log_message("system", f"[quick_dispatch] Abort: Pattern '{pat_name}' result is empty.")
                    continue

                args = {arg_cfg["name"]: value}

                # Context Resolution
                if arg_cfg["name"] in ("target", "command"):
                    args[arg_cfg["name"]] = _resolve_target(value, context)
                
                pre_msg = _get_pre_msg(tool, args)
                global_logger.log_message("system", f"[quick_dispatch] Match (Pattern): '{found_trigger}' -> {tool}({args})")
                return {"handled": True, "tool": tool, "args": args, "pre_msg": pre_msg}
            except Exception as e:
                global_logger.log_message("system", f"[quick_dispatch] Error in pattern '{pat_name}': {e}")
                continue

    return None
