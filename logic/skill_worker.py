import sys
import os
import json
import asyncio
import traceback

# Add the current directory to sys.path to ensure imports work
sys.path.insert(0, os.path.dirname(__file__))

try:
    from registry import registry, set_main_loop
except ImportError as e:
    print(json.dumps({"type": "error", "message": f"Initialization failed: {e}"}))
    sys.exit(1)

async def skill_worker():
    # Setup the event loop for registry bridging
    set_main_loop(asyncio.get_running_loop())
    
    # Load skills
    registry.load_skills()
    
    confirm_msg = {"type": "ready", "tools": list(registry.tools.keys())}
    print(json.dumps(confirm_msg), flush=True)

    while True:
        try:
            line = await asyncio.get_event_loop().run_in_executor(None, sys.stdin.readline)
            if not line:
                break
                
            data = json.loads(line)
            req_type = data.get("type")
            
            if req_type == "execute":
                name = data.get("name")
                args = data.get("args", {})
                
                try:
                    res = await registry.execute(name, args)
                    print(json.dumps({
                        "id": data.get("id"),
                        "type": "result",
                        "status": "success",
                        "result": res
                    }), flush=True)
                except Exception as e:
                    print(json.dumps({
                        "id": data.get("id"),
                        "type": "result",
                        "status": "error",
                        "message": str(e),
                        "traceback": traceback.format_exc()
                    }), flush=True)
            
            elif req_type == "get_schemas":
                print(json.dumps({
                    "id": data.get("id"),
                    "type": "schemas",
                    "schemas": registry.get_schemas()
                }), flush=True)

        except EOFError:
            break
        except Exception as e:
            print(json.dumps({"type": "error", "message": f"Worker loop error: {e}"}), flush=True)

if __name__ == "__main__":
    try:
        asyncio.run(skill_worker())
    except KeyboardInterrupt:
        pass
