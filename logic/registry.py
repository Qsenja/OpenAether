import asyncio
import difflib
import os
import yaml
import json
import importlib
import glob
import json5

# Global loop holder for cross-thread sync-to-async bridging
_MAIN_LOOP = None

def set_main_loop(loop):
    global _MAIN_LOOP
    _MAIN_LOOP = loop

try:
    from qwen_agent.tools.base import BaseTool, register_tool, TOOL_REGISTRY
except ImportError:
    # Define a dummy BaseTool if the library isn't installed yet
    class BaseTool:
        def __init__(self, cfg=None):
            self.cfg = cfg or {}
        def call(self, params: str, **kwargs) -> str:
            raise NotImplementedError
    
    def register_tool(name):
        def decorator(cls):
            return cls
        return decorator
    
    TOOL_REGISTRY = {}

class ToolRegistry:
    def __init__(self):
        self.tools = {}      # dict[str, callable | BaseTool]
        self.schemas = []    # list[dict]
        self.tool_instances = {} # dict[str, BaseTool] for official Qwen tools
        self.knowledge = {}  # Static documentation/tips
        # Aliases: common names the model might hallucinate -> real tool names
        self.aliases = self._load_aliases()

    def _load_aliases(self) -> dict:
        """Load tool aliases from config/aliases.yaml."""
        alias_path = os.path.join(os.path.dirname(__file__), "config", "aliases.yaml")
        try:
            with open(alias_path, "r", encoding="utf-8") as f:
                return yaml.safe_load(f) or {}
        except Exception as e:
            # Fallback for critical aliases if loading fails
            self.log_message("error", f"[registry] Failed to load aliases.yaml: {e}")
            return {
                "google_search": "web_search",
                "search": "web_search",
                "bash": "run_command",
                "shell": "run_command"
            }

    def register(self, name_or_class, description=None, parameters=None):
        """
        Decorator or method to register a tool.
        Supports both functions (legacy) and Qwen-Agent BaseTool classes.
        """
        if isinstance(name_or_class, type) and issubclass(name_or_class, BaseTool):
            # Registering a class
            cls = name_or_class
            tool_name = getattr(cls, 'name', None)
            if not tool_name:
                # Convert CamelCase to snake_case
                import re
                name = cls.__name__
                tool_name = re.sub(r'(?<!^)(?=[A-Z])', '_', name).lower()
                
            self.tools[tool_name] = cls
            
            # Also register with Qwen-Agent if available
            if tool_name not in TOOL_REGISTRY:
                register_tool(tool_name)(cls)

            # We don't instantiate immediately; will be done during execution or export
            desc = getattr(cls, 'description', 'No description.')
            params = getattr(cls, 'parameters', {})
            self.schemas.append({
                'type': 'function',
                'function': {
                    'name': tool_name,
                    'description': desc,
                    'parameters': params
                }
            })
            return cls

        # Registering a function (legacy decorator usage)
        name = name_or_class
        def decorator(func):
            self.tools[name] = func
            
            # BRIDGE: Wrap function in a BaseTool class for Qwen-Agent compatibility
            class WrappedTool(BaseTool):
                def __init__(self, cfg=None):
                    super().__init__(cfg)
                    # Store references for use in call()
                    self.func = func # Capture the actual function
                    self.func_name = name
                
                def call(self, params: str, **kwargs) -> str:
                    # Assistant calls this with a JSON string of arguments
                    try:
                        args = json5.loads(params)
                    except Exception:
                        # Fallback: try cleaning common LLM hallucination garbage (trailing braces, etc)
                        import re
                        try:
                            # Extract everything between the first { and the last }
                            match = re.search(r'(\{.*\})', params, re.DOTALL)
                            if match:
                                args = json5.loads(match.group(1))
                            else:
                                raise ValueError("No valid JSON found in params")
                        except Exception as e:
                            return json.dumps({"status": "error", "message": f"Invalid tool arguments: {str(e)}"})

                    # Resolve naming coercion if needed
                    coerced = registry._coerce_args(self.func_name, args)
                    
                    try:
                        if asyncio.iscoroutinefunction(self.func):
                            # BRIDGE: Run async skill from sync context safely
                            loop = _MAIN_LOOP or asyncio.get_event_loop()
                            if loop.is_running():
                                # Run in the main event loop thread
                                future = asyncio.run_coroutine_threadsafe(self.func(**coerced), loop)
                                res = future.result() # Blocks caller thread until async work is done
                            else:
                                # Loop not running (rare/cli mode), run normally
                                res = asyncio.run(self.func(**coerced))
                        else:
                            res = self.func(**coerced)
                        return json.dumps(res, ensure_ascii=False)
                    except Exception as e:
                        return json.dumps({"status": "error", "message": f"Bridge error: {str(e)}"})

            # Pre-validate/default parameters to valid OpenAI schema
            valid_params = parameters if parameters else {"type": "object", "properties": {}}

            # Setting metadata on the bridge class
            WrappedTool.name = name
            WrappedTool.description = description
            WrappedTool.parameters = valid_params

            # Register the bridge with Qwen-Agent
            if name not in TOOL_REGISTRY:
                register_tool(name)(WrappedTool)

            self.schemas.append({
                'type': 'function',
                'function': {
                    'name': name,
                    'description': description,
                    'parameters': valid_params
                }
            })
            return func
        return decorator

    def load_skills(self):
        """Mechanically import all skill modules to trigger registration."""
        skills_dir = os.path.join(os.path.dirname(__file__), "skills")
        if not os.path.exists(skills_dir):
            return
            
        skill_files = glob.glob(os.path.join(skills_dir, "*.py"))
        for f in skill_files:
            module_name = os.path.basename(f)[:-3]
            if module_name != "__init__":
                try:
                    # Import as skills.filename
                    importlib.import_module(f"skills.{module_name}")
                    self.log_message("info", f"[registry] Loaded skill module: {module_name}")
                except Exception as e:
                    self.log_message("error", f"[registry] Failed to load skill {module_name}: {e}")

    def get_tool_schema(self, name: str) -> dict:
        """Get the JSON schema for a specific tool."""
        resolved = self.resolve_name(name)
        for s in self.schemas:
            if s['function']['name'] == resolved:
                return s
        return None

    def register_knowledge(self, name: str, content: str = None):
        """Register static documentation or tips. Can be used as a decorator or direct call."""
        if content is not None:
            self.knowledge[name] = content
            return
            
        def decorator(func):
            # Use the docstring if available, otherwise call the function
            val = func.__doc__ if func.__doc__ else func()
            self.knowledge[name] = val.strip() if val else "No content."
            return func
        return decorator

    def resolve_name(self, name: str) -> str:
        """Resolve a tool name, checking aliases and fuzzy matching."""
        # Exact match
        if name in self.tools:
            return name
            
        # Dynamic fallback for prefix hallucinations (e.g. "install_discord")
        if name.startswith("install_") and name != "install_software":
            return "install_software"
        if name.startswith("uninstall_") and name != "uninstall_software":
            return "uninstall_software"
        if name.startswith("open_") and name != "open_app":
            return "open_app"
        # Map tool to function for legacy support if needed
        if name == "tool": return "function"
        # Alias match
        if name in self.aliases and self.aliases[name] in self.tools:
            print(f"[ALIAS] '{name}' -> '{self.aliases[name]}'")
            return self.aliases[name]
        # Fuzzy match: find closest known tool name
        all_names = list(self.tools.keys()) + list(self.aliases.keys())
        matches = difflib.get_close_matches(name, all_names, n=1, cutoff=0.6)
        if matches:
            resolved = self.resolve_name(matches[0])
            print(f"[FUZZY] '{name}' -> '{resolved}'")
            return resolved
        return name  # Return original, will fail with proper error

    def get_schemas(self):
        return self.schemas

    def search_tools(self, query: str, limit: int = 5) -> dict:
        """Search for tools and knowledge based on keywords."""
        import re
        # Split by any non-alphanumeric character (handles commas, etc.)
        query_words = set(w for w in re.split(r'[^a-zA-Z0-9]+', query.lower()) if w)
        
        scored_schemas = []
        scored_knowledge = []
        
        # Search Tools
        for schema in self.schemas:
            name = schema['function']['name'].lower()
            desc = schema['function']['description'].lower()
            
            score = 0
            for word in query_words:
                # Direct or substring match
                if word in name or name in word:
                    score += 5 if word == name else 3
                if word in desc or desc in word:
                    score += 1
                # Special keyword boost
                if word in ["install", "uninstall", "remove"] and ("install" in name or "remove" in name):
                    score += 10
            
            if score > 0:
                scored_schemas.append((score, schema))
        
        # Search Knowledge
        for name, content in self.knowledge.items():
            name_low = name.lower()
            cont_low = content.lower()
            score = 0
            for word in query_words:
                if word in name_low:
                    score += 5 if word == name_low else 3
                if word in cont_low:
                    score += 1
            if score > 0:
                scored_knowledge.append((score, name, content))
        
        # Sort and return
        scored_schemas.sort(key=lambda x: x[0], reverse=True)
        scored_knowledge.sort(key=lambda x: x[0], reverse=True)
        
        return {
            "tools": [s[1] for s in scored_schemas[:limit]],
            "knowledge": [{"name": k[1], "content": k[2]} for k in scored_knowledge[:2]]
        }

    def _coerce_args(self, resolved_name: str, args: dict) -> dict:
        """Map hallucinated argument names to actual schema-defined names."""
        if not isinstance(args, dict):
            return {}
            
        schema = next((s for s in self.schemas if s['function']['name'] == resolved_name), None)
        if not schema:
            return args
            
        props = schema['function']['parameters'].get('properties', {})
        expected_keys = list(props.keys())
        required_keys = schema['function']['parameters'].get('required', [])
        
        if not expected_keys:
            return {}  # Function expects no arguments
            
        # 1. Single argument exact coercion (highly reliable)
        if len(expected_keys) == 1 and len(args) == 1:
            return {expected_keys[0]: list(args.values())[0]}
            
        new_args = {}
        # 2. Fuzzy map incoming keys to expected keys
        import difflib
        for k, v in args.items():
            if k in expected_keys:
                new_args[k] = v
            else:
                matches = difflib.get_close_matches(k, expected_keys, n=1, cutoff=0.3)
                if matches and matches[0] not in new_args:
                    new_args[matches[0]] = v
                else:
                    # Dump unmapped stuff into generic catch-all keys if possible
                    if "details" in expected_keys:
                        new_args["details"] = f"{new_args.get('details', '')} {k}: {v}".strip()
                    elif "query" in expected_keys:
                        new_args["query"] = f"{new_args.get('query', '')} {k}: {v}".strip()

        # 3. Fill missing required parameters to prevent TypeError crashes
        for req in required_keys:
            if req not in new_args:
                new_args[req] = f"AUTO-FILLED (Missing {req})"
                
        # 4. Filter out any remaining spurious keys that Python would choke on
        return {k: v for k, v in new_args.items() if k in expected_keys}

    async def execute(self, name, args):
        resolved = self.resolve_name(name)
        
        # Dynamic argument extraction from hallucinated tool names
        if not args or len(args) == 0:
            if resolved == "install_software" and name.startswith("install_") and name != "install_software":
                args = {"name": name.replace("install_", "", 1)}
            elif resolved == "uninstall_software" and name.startswith("uninstall_") and name != "uninstall_software":
                args = {"name": name.replace("uninstall_", "", 1)}
            elif resolved == "open_app" and name.startswith("open_") and name != "open_app":
                args = {"command": name.replace("open_", "", 1)}
                
        if resolved in self.tools:
            try:
                coerced_args = self._coerce_args(resolved, args)
                func_or_cls = self.tools[resolved]
                
                if isinstance(func_or_cls, type) and issubclass(func_or_cls, BaseTool):
                    # Instantiate on demand (singleton-ish within the registry session if needed, 
                    # but for now we just run it).
                    if resolved not in self.tool_instances:
                        self.tool_instances[resolved] = func_or_cls()
                    
                    tool_inst = self.tool_instances[resolved]
                    # Qwen BaseTool.call expects JSON string or dict? Usually a string for the agent.
                    # Registry expects to return a Python object.
                    if asyncio.iscoroutinefunction(tool_inst.call):
                        result = await tool_inst.call(json.dumps(coerced_args))
                    else:
                        result = tool_inst.call(json.dumps(coerced_args))
                    
                    # result is usually a JSON string from BaseTool.call
                    try:
                        return json.loads(result)
                    except:
                        return result

                # Legacy function-based execution
                if asyncio.iscoroutinefunction(func_or_cls):
                    return await func_or_cls(**coerced_args)
                return func_or_cls(**coerced_args)
            except Exception as e:
                return {"status": "error", "message": f"Error executing tool {resolved}: {str(e)}"}
        return {"status": "error", "message": f"Tool '{name}' not found. To look for new capabilities, use 'discover_tools(query)'. Available tools: {', '.join(list(self.tools.keys())[:5])}..."}

    def log_event(self, event_type: str, data: dict):
        """Send a structured event log to the Rust backend."""
        print(json.dumps({
            "type": "log",
            "event_type": event_type,
            "data": data
        }), flush=True)

    def log_message(self, level: str, message: str, tag: str = "SKILL"):
        """Send a simple message log to the Rust backend."""
        self.log_event("message", {
            "level": level,
            "message": message,
            "tag": tag
        })

registry = ToolRegistry()
