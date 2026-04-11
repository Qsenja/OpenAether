import asyncio
import difflib
import os
import yaml
from logger import global_logger

class ToolRegistry:
    def __init__(self):
        self.tools = {}
        self.schemas = []
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
            global_logger.log_message("system", f"[registry] Failed to load aliases.yaml: {e}")
            return {
                "google_search": "web_search",
                "search": "web_search",
                "bash": "run_command",
                "shell": "run_command"
            }

    def register(self, name, description, parameters):
        def decorator(func):
            self.tools[name] = func
            self.schemas.append({
                'type': 'function',
                'function': {
                    'name': name,
                    'description': description,
                    'parameters': parameters
                }
            })
            return func
        return decorator

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
                func = self.tools[resolved]
                if asyncio.iscoroutinefunction(func):
                    return await func(**coerced_args)
                return func(**coerced_args)
            except Exception as e:
                return {"status": "error", "message": f"Error executing tool {resolved}: {str(e)}"}
        return {"status": "error", "message": f"Tool '{name}' not found. Available tools: {', '.join(list(self.tools.keys())[:10])}..."}

registry = ToolRegistry()
