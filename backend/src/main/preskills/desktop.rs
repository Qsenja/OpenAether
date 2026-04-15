use anyhow::Result;
use rig::{
    completion::ToolDefinition,
    tool::{ToolDyn, ToolError},
};
use rig::wasm_compat::WasmBoxedFuture;
use serde::Deserialize;
use serde_json::json;
use std::process::Command;
use std::sync::Arc;
use crate::logic::logger::Logger;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesktopEnvironment {
    Hyprland,
    Sway,
    I3,
    Bspwm,
    Gnome,
    Kde,
    Xfce,
    River,
    Awesome,
    GenericWayland,
    GenericX11,
    Unknown,
}

impl DesktopEnvironment {
    fn detect() -> Self {
        let xdg = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
        let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        
        if xdg.contains("hyprland") {
            Self::Hyprland
        } else if xdg.contains("sway") {
            Self::Sway
        } else if xdg.contains("i3") {
            Self::I3
        } else if xdg.contains("gnome") {
            Self::Gnome
        } else if xdg.contains("kde") || xdg.contains("plasma") {
            Self::Kde
        } else if xdg.contains("xfce") {
            Self::Xfce
        } else if xdg.contains("bspwm") {
            Self::Bspwm
        } else if xdg.contains("river") {
            Self::River
        } else if xdg.contains("awesome") {
            Self::Awesome
        } else if wayland {
            // Check for Sway/Hyprland if xdg is ambiguous
            if Command::new("swaymsg").arg("--version").output().is_ok() {
                Self::Sway
            } else if Command::new("hyprctl").arg("--version").output().is_ok() {
                Self::Hyprland
            } else {
                Self::GenericWayland
            }
        } else if std::env::var("DISPLAY").is_ok() {
             if Command::new("i3-msg").arg("--version").output().is_ok() {
                Self::I3
            } else {
                Self::GenericX11
            }
        } else {
            Self::Unknown
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Hyprland => "hyprland",
            Self::Sway => "sway",
            Self::I3 => "i3",
            Self::Bspwm => "bspwm",
            Self::Gnome => "gnome",
            Self::Kde => "kde",
            Self::Xfce => "xfce",
            Self::River => "river",
            Self::Awesome => "awesome",
            Self::GenericWayland => "wayland",
            Self::GenericX11 => "x11",
            Self::Unknown => "unknown",
        }
    }
}

pub struct DesktopTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct DesktopArgs {
    action: String,
    #[serde(default)]
    number: Option<i32>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
    #[serde(default)]
    path: Option<String>,
}

impl ToolDyn for DesktopTool {
    fn name(&self) -> String {
        "desktop".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "desktop".to_string(),
            description: "Control your desktop environment (windows, workspaces, audio, notifications, keyboard/mouse). Supports Hyprland, Sway, i3, BSPWM, River, Awesome, GNOME, KDE, and X11.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "get_workspaces", "switch_workspace", "move_window_to_workspace",
                            "get_windows", "click", "type_text", "set_volume",
                            "send_notification", "play_audio", "clipboard"
                        ],
                        "description": "The action to perform."
                    },
                    "number": { "type": "integer", "description": "Workspace/Tag/Desktop number or volume level (0-100)." },
                    "text": { "type": "string", "description": "Text to type or set in clipboard." },
                    "title": { "type": "string", "description": "Notification title." },
                    "message": { "type": "string", "description": "Notification message." },
                    "x": { "type": "integer", "description": "Mouse X coordinate." },
                    "y": { "type": "integer", "description": "Mouse Y coordinate." },
                    "path": { "type": "string", "description": "Path to audio file." }
                },
                "required": ["action"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: DesktopArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(format!("Deserialization failed: {}. Args: {}", e, args).into()))?;

            let de = DesktopEnvironment::detect();
            logger.log("AGENT", &format!("Desktop Tool: Action={}, Environment={}", json_args.action, de.name()));

            match json_args.action.as_str() {
                "get_workspaces" => Self::do_get_workspaces(de),
                "switch_workspace" => {
                    let num = json_args.number.ok_or_else(|| ToolError::ToolCallError("Workspace 'number' is required.".into()))?;
                    Self::do_switch_workspace(de, num)
                }
                "move_window_to_workspace" => {
                    let num = json_args.number.ok_or_else(|| ToolError::ToolCallError("Target workspace 'number' is required.".into()))?;
                    Self::do_move_window(de, num)
                }
                "get_windows" => Self::do_get_windows(de),
                "click" => {
                    let x = json_args.x.ok_or_else(|| ToolError::ToolCallError("'x' coordinate required.".into()))?;
                    let y = json_args.y.ok_or_else(|| ToolError::ToolCallError("'y' coordinate required.".into()))?;
                    Self::do_click(de, x, y)
                }
                "type_text" => {
                    let text = json_args.text.ok_or_else(|| ToolError::ToolCallError("'text' to type is required.".into()))?;
                    Self::do_type_text(de, &text)
                }
                "set_volume" => {
                    let level = json_args.number.ok_or_else(|| ToolError::ToolCallError("Volume 'number' (0-100) is required.".into()))?;
                    Self::do_set_volume(level)
                }
                "send_notification" => {
                    let title = json_args.title.ok_or_else(|| ToolError::ToolCallError("'title' required.".into()))?;
                    let msg = json_args.message.ok_or_else(|| ToolError::ToolCallError("'message' required.".into()))?;
                    Self::do_notify(&title, &msg)
                }
                "play_audio" => {
                    let path = json_args.path.ok_or_else(|| ToolError::ToolCallError("'path' required.".into()))?;
                    Self::do_play_audio(&path)
                }
                "clipboard" => {
                    Self::do_clipboard(json_args.text)
                }
                _ => Err(ToolError::ToolCallError(format!("Invalid action: {}", json_args.action).into())),
            }
        })
    }
}

impl DesktopTool {
    fn run_cmd(cmd: &mut Command) -> Result<String, ToolError> {
        let out = cmd.output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        } else {
            Err(ToolError::ToolCallError(String::from_utf8_lossy(&out.stderr).to_string().into()))
        }
    }

    fn do_get_workspaces(de: DesktopEnvironment) -> Result<String, ToolError> {
        match de {
            DesktopEnvironment::Hyprland => Self::run_cmd(Command::new("hyprctl").arg("workspaces").arg("-j")),
            DesktopEnvironment::Sway => Self::run_cmd(Command::new("swaymsg").args(&["-t", "get_workspaces"])),
            DesktopEnvironment::I3 => Self::run_cmd(Command::new("i3-msg").args(&["-t", "get_workspaces"])),
            DesktopEnvironment::Bspwm => Self::run_cmd(Command::new("bspc").args(&["query", "-D", "--names"])),
            DesktopEnvironment::Kde => Self::run_cmd(Command::new("qdbus").args(&["org.kde.KWin", "/KWin", "org.kde.KWin.desktops"])),
            _ => {
                let mut cmd = Command::new("wmctrl");
                cmd.arg("-d");
                if cmd.output().is_ok() {
                    Self::run_cmd(&mut cmd)
                } else {
                    Err(ToolError::ToolCallError("Workspace enumeration not supported on this environment.".into()))
                }
            }
        }
    }

    fn do_switch_workspace(de: DesktopEnvironment, num: i32) -> Result<String, ToolError> {
        match de {
            DesktopEnvironment::Hyprland => Self::run_cmd(Command::new("hyprctl").args(&["dispatch", "workspace", &num.to_string()])),
            DesktopEnvironment::Sway => Self::run_cmd(Command::new("swaymsg").args(&["workspace", &num.to_string()])),
            DesktopEnvironment::I3 => Self::run_cmd(Command::new("i3-msg").args(&["workspace", &num.to_string()])),
            DesktopEnvironment::Bspwm => Self::run_cmd(Command::new("bspc").args(&["desktop", "-f", &num.to_string()])),
            DesktopEnvironment::River => Self::run_cmd(Command::new("riverctl").args(&["set-focused-tags", &(1 << (num - 1)).to_string()])),
            DesktopEnvironment::Awesome => Self::run_cmd(Command::new("awesome-client").arg(&format!("mouse.screen.tags[{}]:view_only()", num))),
            DesktopEnvironment::Kde => Self::run_cmd(Command::new("qdbus").args(&["org.kde.KWin", "/KWin", "org.kde.KWin.setCurrentDesktop", &num.to_string()])),
            _ => {
                Self::run_cmd(Command::new("wmctrl").args(&["-s", &((num - 1).to_string())])) // wmctrl is 0-indexed
            }
        }
    }

    fn do_move_window(de: DesktopEnvironment, num: i32) -> Result<String, ToolError> {
        match de {
            DesktopEnvironment::Hyprland => Self::run_cmd(Command::new("hyprctl").args(&["dispatch", "movetoworkspacesilent", &num.to_string()])),
            DesktopEnvironment::Sway => Self::run_cmd(Command::new("swaymsg").args(&["move", "container", "to", "workspace", &num.to_string()])),
            DesktopEnvironment::I3 => Self::run_cmd(Command::new("i3-msg").args(&["move", "container", "to", "workspace", &num.to_string()])),
            DesktopEnvironment::Bspwm => Self::run_cmd(Command::new("bspc").args(&["node", "-d", &num.to_string()])),
            DesktopEnvironment::River => Self::run_cmd(Command::new("riverctl").args(&["set-view-tags", &(1 << (num - 1)).to_string()])),
            DesktopEnvironment::Awesome => Self::run_cmd(Command::new("awesome-client").arg(&format!("client.focus:move_to_tag(mouse.screen.tags[{}])", num))),
            _ => {
                Self::run_cmd(Command::new("wmctrl").args(&["-r", ":ACTIVE:", "-t", &((num - 1).to_string())]))
            }
        }
    }

    fn do_get_windows(de: DesktopEnvironment) -> Result<String, ToolError> {
        match de {
            DesktopEnvironment::Hyprland => Self::run_cmd(Command::new("hyprctl").args(&["clients", "-j"])),
            DesktopEnvironment::Sway => Self::run_cmd(Command::new("swaymsg").args(&["-t", "get_tree"])),
            DesktopEnvironment::I3 => Self::run_cmd(Command::new("i3-msg").args(&["-t", "get_tree"])),
            DesktopEnvironment::Bspwm => Self::run_cmd(Command::new("bspc").args(&["query", "-N", "-n", ".window"])),
            _ => {
                Self::run_cmd(Command::new("wmctrl").arg("-l"))
            }
        }
    }

    fn do_click(de: DesktopEnvironment, x: i32, y: i32) -> Result<String, ToolError> {
        if de == DesktopEnvironment::Hyprland {
             let _ = Command::new("hyprctl").args(&["dispatch", "movecursor", &x.to_string(), &y.to_string()]).output();
        } else if de == DesktopEnvironment::Sway {
             let _ = Command::new("swaymsg").arg(&format!("seat - cursor set {} {}", x, y)).output();
        }
        
        // ydotool works on most Wayland compositors (with daemon)
        let mut ydotool = Command::new("ydotool");
        if ydotool.arg("--version").output().is_ok() {
            let mut cmd = Command::new("ydotool");
            cmd.args(&["click", "1"]);
            Self::run_cmd(&mut cmd)
        } else {
            let mut xdotool = Command::new("xdotool");
            if xdotool.arg("--version").output().is_ok() {
                let mut cmd = Command::new("xdotool");
                cmd.args(&["mousemove", &x.to_string(), &y.to_string(), "click", "1"]);
                Self::run_cmd(&mut cmd)
            } else {
                Err(ToolError::ToolCallError("No mouse simulation tool found (ydotool/xdotool).".into()))
            }
        }
    }

    fn do_type_text(_de: DesktopEnvironment, text: &str) -> Result<String, ToolError> {
        let mut ydotool = Command::new("ydotool");
        if ydotool.arg("--version").output().is_ok() {
            let mut cmd = Command::new("ydotool");
            cmd.args(&["type", text]);
            Self::run_cmd(&mut cmd)
        } else {
            let mut xdotool = Command::new("xdotool");
            if xdotool.arg("--version").output().is_ok() {
                let mut cmd = Command::new("xdotool");
                cmd.args(&["type", text]);
                Self::run_cmd(&mut cmd)
            } else {
                Err(ToolError::ToolCallError("No keyboard simulation tool found (ydotool/xdotool).".into()))
            }
        }
    }

    fn do_set_volume(level: i32) -> Result<String, ToolError> {
        // Try multiple tools
        let mut pamixer = Command::new("pamixer");
        if let Ok(out) = pamixer.args(&["--set-volume", &level.to_string()]).output() {
            if out.status.success() { return Ok("Volume set via pamixer.".into()); }
        }
        
        let mut wpctl = Command::new("wpctl");
        if let Ok(out) = wpctl.args(&["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{}%", level)]).output() {
            if out.status.success() { return Ok("Volume set via wpctl.".into()); }
        }

        let mut amixer = Command::new("amixer");
        if let Ok(out) = amixer.args(&["set", "Master", &format!("{}%", level)]).output() {
            if out.status.success() { return Ok("Volume set via amixer.".into()); }
        }

        Err(ToolError::ToolCallError("No supported volume control tool found (pamixer/wpctl/amixer).".into()))
    }

    fn do_notify(title: &str, msg: &str) -> Result<String, ToolError> {
        Self::run_cmd(Command::new("notify-send").args(&[title, msg]))
    }

    fn do_play_audio(path: &str) -> Result<String, ToolError> {
        let p = if path.starts_with('~') {
             path.replace('~', &std::env::var("HOME").unwrap_or_default())
        } else {
            path.to_string()
        };
        
        Command::new("mpv")
            .args(&["--no-video", &p])
            .spawn()
            .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        Ok(format!("Successfully started audio playback of: {}", p))
    }

    fn do_clipboard(text: Option<String>) -> Result<String, ToolError> {
        if let Some(t) = text {
            // Set clipboard
            if std::env::var("WAYLAND_DISPLAY").is_ok() {
                let mut child = Command::new("wl-copy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                
                use std::io::Write;
                let mut stdin = child.stdin.take().ok_or_else(|| ToolError::ToolCallError("Failed to open wl-copy stdin".into()))?;
                stdin.write_all(t.as_bytes()).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                drop(stdin); // Flush and close
                return Ok("Set clipboard via wl-copy.".into());
            } else {
                let mut child = Command::new("xclip")
                    .args(&["-selection", "clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                
                use std::io::Write;
                let mut stdin = child.stdin.take().ok_or_else(|| ToolError::ToolCallError("Failed to open xclip stdin".into()))?;
                stdin.write_all(t.as_bytes()).map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                drop(stdin);
                return Ok("Set clipboard via xclip.".into());
            }
        } else {
            // Get clipboard
            if std::env::var("WAYLAND_DISPLAY").is_ok() {
                let mut cmd = Command::new("wl-paste");
                Self::run_cmd(cmd.arg("--trim-newline"))
            } else {
                let mut cmd = Command::new("xclip");
                Self::run_cmd(cmd.args(&["-selection", "clipboard", "-o"]))
            }
        }
    }
}
