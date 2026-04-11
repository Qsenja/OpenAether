import os
import glob

def get_hyprland_env():
    """Robustly recovers the Hyprland signature or environment (Modern XDG Path)."""
    env = os.environ.copy()
    
    # Ensure DISPLAY and WAYLAND_DISPLAY have defaults
    if "DISPLAY" not in env: env["DISPLAY"] = ":0"
    if "WAYLAND_DISPLAY" not in env: env["WAYLAND_DISPLAY"] = "wayland-1"
    if "XDG_RUNTIME_DIR" not in env:
        env["XDG_RUNTIME_DIR"] = f"/run/user/{os.getuid()}"
    
    # Recover HYPRLAND_INSTANCE_SIGNATURE if missing or potentially stale
    # We follow the 'Deepsearch' recommendation: prioritize youngest session in XDG_RUNTIME_DIR
    try:
        runtime_dir = env["XDG_RUNTIME_DIR"]
        hypr_runtime = os.path.join(runtime_dir, "hypr")
        
        # Collect all signature directories
        if os.path.exists(hypr_runtime):
            dirs = [os.path.join(hypr_runtime, d) for d in os.listdir(hypr_runtime) 
                    if os.path.isdir(os.path.join(hypr_runtime, d))]
            
            if dirs:
                # Sort by modification time (youngest first)
                dirs.sort(key=os.path.getmtime, reverse=True)
                latest_dir = dirs[0]
                sig = os.path.basename(latest_dir)
                env["HYPRLAND_INSTANCE_SIGNATURE"] = sig
    except Exception as e:
        # Fallback to legacy /tmp/hypr/ if XDG path fails
        try:
            sockets = glob.glob("/tmp/hypr/*/.socket.sock")
            if sockets:
                sockets.sort(key=os.path.getmtime, reverse=True)
                sig = sockets[0].split("/tmp/hypr/")[1].split("/")[0]
                env["HYPRLAND_INSTANCE_SIGNATURE"] = sig
        except:
            pass
            
    return env

HYPRLAND_ENV = get_hyprland_env()
