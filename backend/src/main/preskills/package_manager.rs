use rig::{
    completion::ToolDefinition,
    tool::{ToolDyn, ToolError},
};
use rig::wasm_compat::WasmBoxedFuture;
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Command;
use std::sync::Arc;
use crate::logic::logger::Logger;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Pacman,
    Apt,
    Dnf,
    Zypper,
    Apk,
    Npm, // Fallback
}

impl PackageManager {
    fn detect() -> Option<Self> {
        if Command::new("pacman").arg("--version").output().is_ok() {
            Some(Self::Pacman)
        } else if Command::new("apt-get").arg("--version").output().is_ok() {
            Some(Self::Apt)
        } else if Command::new("dnf").arg("--version").output().is_ok() {
            Some(Self::Dnf)
        } else if Command::new("zypper").arg("--version").output().is_ok() {
            Some(Self::Zypper)
        } else if Command::new("apk").arg("--version").output().is_ok() {
            Some(Self::Apk)
        } else if Command::new("npm").arg("--version").output().is_ok() {
            Some(Self::Npm)
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Zypper => "zypper",
            Self::Apk => "apk",
            Self::Npm => "npm",
        }
    }
}

pub struct PackageManagerTool {
    pub logger: Arc<Logger>,
}

#[derive(Deserialize)]
struct PackageManagerArgs {
    action: String,
    packages: Option<Value>,
    query: Option<String>,
}

impl ToolDyn for PackageManagerTool {
    fn name(&self) -> String {
        "package_manager".to_string()
    }

    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        let def = ToolDefinition {
            name: "package_manager".to_string(),
            description: "Manage system packages (install, remove, search, status). Automatically detects distribution-specific tools (pacman, apt, dnf, etc.).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["install", "remove", "reinstall", "search", "status", "cleanup_orphans", "clean_cache", "purge_data"],
                        "description": "The action to perform."
                    },
                    "packages": {
                        "oneOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ],
                        "description": "Package or list of packages to act upon."
                    },
                    "query": {
                        "type": "string",
                        "description": "Search query if action is 'search'."
                    }
                },
                "required": ["action"]
            }),
        };
        Box::pin(async move { def })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        let logger = self.logger.clone();
        Box::pin(async move {
            let json_args: PackageManagerArgs = serde_json::from_str(&args)
                .map_err(|e| ToolError::ToolCallError(format!("Deserialization failed: {}. Args: {}", e, args).into()))?;

            let pm = PackageManager::detect().ok_or_else(|| {
                ToolError::ToolCallError("No supported package manager detected on this system.".into())
            })?;

            let pkgs: Vec<String> = match json_args.packages {
                Some(Value::String(s)) => vec![s],
                Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
                _ => Vec::new(),
            };

            logger.log("AGENT", &format!("Package Manager: Action={}, Distro={}, Packages={:?}", json_args.action, pm.name(), pkgs));

            match json_args.action.as_str() {
                "install" => {
                    if pkgs.is_empty() { return Err(ToolError::ToolCallError("No packages specified.".into())); }
                    let logger_clone = logger.clone();
                    tokio::task::spawn_blocking(move || Self::do_install(&logger_clone, pm, &pkgs)).await.map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                }
                "remove" => {
                    if pkgs.is_empty() { return Err(ToolError::ToolCallError("No packages specified.".into())); }
                    let logger_clone = logger.clone();
                    tokio::task::spawn_blocking(move || Self::do_remove(&logger_clone, pm, &pkgs)).await.map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                }
                "reinstall" => {
                    if pkgs.is_empty() { return Err(ToolError::ToolCallError("No packages specified.".into())); }
                    let logger_clone = logger.clone();
                    tokio::task::spawn_blocking(move || {
                         Self::do_remove(&logger_clone, pm, &pkgs)?;
                         Self::do_install(&logger_clone, pm, &pkgs)
                    }).await.map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                }
                "search" => {
                    let query = json_args.query.ok_or_else(|| ToolError::ToolCallError("No search query specified.".into()))?;
                    let query_clone = query.clone();
                    let logger_clone = logger.clone();
                    tokio::task::spawn_blocking(move || Self::do_search(&logger_clone, pm, &query_clone)).await.map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                }
                "status" => {
                    if pkgs.is_empty() { return Err(ToolError::ToolCallError("No packages specified.".into())); }
                    let logger_clone = logger.clone();
                    tokio::task::spawn_blocking(move || Self::do_status(&logger_clone, pm, &pkgs)).await.map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                }
                "cleanup_orphans" => {
                     let logger_clone = logger.clone();
                     tokio::task::spawn_blocking(move || Self::do_cleanup_orphans(&logger_clone, pm)).await.map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                }
                "clean_cache" => {
                     let logger_clone = logger.clone();
                     tokio::task::spawn_blocking(move || Self::do_clean_cache(&logger_clone, pm)).await.map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                }
                "purge_data" => {
                    if pkgs.is_empty() { return Err(ToolError::ToolCallError("No packages specified.".into())); }
                    let logger_clone = logger.clone();
                    tokio::task::spawn_blocking(move || Self::do_purge_data(&logger_clone, &pkgs)).await.map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                }
                _ => Err(ToolError::ToolCallError(format!("Invalid action: {}", json_args.action).into())),
            }
        })
    }
}

impl PackageManagerTool {
    fn run_filtered(action: &str, pkgs: &[String], cmd: &mut Command) -> Result<String, ToolError> {
        let output = cmd.output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
        if output.status.success() {
            Ok(format!("Success: {} completed for {:?}", action, pkgs))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let last_lines = stderr.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            Err(ToolError::ToolCallError(format!("Failure: {} error:\n{}", action, last_lines).into()))
        }
    }

    fn do_install(logger: &Arc<Logger>, pm: PackageManager, pkgs: &[String]) -> Result<String, ToolError> {
        logger.log("SYSTEM", &format!("Installing packages via {}: {:?}", pm.name(), pkgs));
        match pm {
            PackageManager::Pacman => {
                let mut cmd = Command::new("pkexec");
                cmd.args(&["pacman", "-S", "--noconfirm"]).args(pkgs);
                let output = cmd.output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                
                if !output.status.success() {
                    if Command::new("yay").arg("--version").output().is_ok() {
                        logger.log("SYSTEM", "Official repo fail. Trying yay (AUR)...");
                        let mut yay = Command::new("yay");
                        yay.args(&["-S", "--noconfirm", "--sudo", "pkexec"]).args(pkgs);
                        return Self::run_filtered("install", pkgs, &mut yay);
                    }
                }
                if output.status.success() {
                    Ok(format!("Success: Installed {:?}", pkgs))
                } else {
                    Err(ToolError::ToolCallError(String::from_utf8_lossy(&output.stderr).into()))
                }
            }
            PackageManager::Apt => Self::run_filtered("install", pkgs, Command::new("pkexec").arg("apt-get").arg("install").arg("-y").args(pkgs)),
            PackageManager::Dnf => Self::run_filtered("install", pkgs, Command::new("pkexec").arg("dnf").arg("install").arg("-y").args(pkgs)),
            PackageManager::Zypper => Self::run_filtered("install", pkgs, Command::new("pkexec").arg("zypper").arg("install").arg("-y").args(pkgs)),
            PackageManager::Apk => Self::run_filtered("install", pkgs, Command::new("pkexec").arg("apk").arg("add").args(pkgs)),
            PackageManager::Npm => Self::run_filtered("install", pkgs, Command::new("npm").arg("install").arg("-g").args(pkgs)),
        }
    }

    fn do_remove(logger: &Arc<Logger>, pm: PackageManager, pkgs: &[String]) -> Result<String, ToolError> {
        logger.log("SYSTEM", &format!("Removing packages via {}: {:?}", pm.name(), pkgs));
        match pm {
            PackageManager::Pacman => {
                let mut cmd = Command::new("pkexec");
                cmd.args(&["pacman", "-Rns", "--noconfirm"]).args(pkgs);
                let output = cmd.output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                if !output.status.success() {
                    if Command::new("yay").arg("--version").output().is_ok() {
                        let mut yay = Command::new("yay");
                        yay.args(&["-Rns", "--noconfirm", "--sudo", "pkexec"]).args(pkgs);
                        return Self::run_filtered("remove", pkgs, &mut yay);
                    }
                }
                if output.status.success() {
                    Ok(format!("Success: Removed {:?}", pkgs))
                } else {
                    Err(ToolError::ToolCallError(String::from_utf8_lossy(&output.stderr).into()))
                }
            }
            PackageManager::Apt => Self::run_filtered("remove", pkgs, Command::new("pkexec").arg("apt-get").arg("purge").arg("-y").args(pkgs)),
            PackageManager::Dnf => Self::run_filtered("remove", pkgs, Command::new("pkexec").arg("dnf").arg("remove").arg("-y").args(pkgs)),
            PackageManager::Zypper => Self::run_filtered("remove", pkgs, Command::new("pkexec").arg("zypper").arg("remove").arg("-y").args(pkgs)),
            PackageManager::Apk => Self::run_filtered("remove", pkgs, Command::new("pkexec").arg("apk").arg("del").args(pkgs)),
            PackageManager::Npm => Self::run_filtered("remove", pkgs, Command::new("npm").arg("uninstall").arg("-g").args(pkgs)),
        }
    }

    fn process_search_output(raw_output: String, limit: usize) -> String {
        let mut results = Vec::new();
        let lines: Vec<&str> = raw_output.lines().collect();
        let mut i = 0;
        while i < lines.len() && results.len() < limit {
            let line = lines[i].trim();
            if !line.is_empty() && !line.starts_with(' ') {
                // Headline (Name, Version, Repo)
                let name_part = line;
                let desc_part = if i + 1 < lines.len() && lines[i+1].starts_with(' ') {
                    lines[i+1].trim()
                } else {
                    "No description available"
                };
                results.push(format!("- {}: {}", name_part, desc_part));
                i += 1;
            }
            i += 1;
        }
        if results.is_empty() {
             "No matches found.".to_string()
        } else {
             format!("Found {} results:\n{}", results.len(), results.join("\n"))
        }
    }

    fn do_search(_logger: &Arc<Logger>, pm: PackageManager, query: &str) -> Result<String, ToolError> {
        let limit = 10;
        match pm {
            PackageManager::Pacman => {
                let official = Command::new("pacman").arg("-Ss").arg(query).output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                let mut results = Self::process_search_output(String::from_utf8_lossy(&official.stdout).to_string(), limit / 2);
                
                if Command::new("yay").arg("--version").output().is_ok() {
                    let aur = Command::new("yay").arg("-Ss").arg(query).output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                    let aur_results = Self::process_search_output(String::from_utf8_lossy(&aur.stdout).to_string(), limit / 2);
                    results = format!("Official:\n{}\nAUR:\n{}", results, aur_results);
                }
                Ok(results)
            }
            PackageManager::Apt => {
                let out = Command::new("apt-cache").arg("search").arg(query).output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let lines: Vec<&str> = stdout.lines().take(limit).collect();
                Ok(lines.join("\n"))
            }
            PackageManager::Dnf => {
                let out = Command::new("dnf").arg("search").arg(query).output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                Ok(Self::process_search_output(String::from_utf8_lossy(&out.stdout).to_string(), limit))
            }
            _ => {
                let out = Command::new(pm.name()).arg("search").arg(query).output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let lines: Vec<&str> = stdout.lines().take(limit).collect();
                Ok(lines.join("\n"))
            }
        }
    }

    fn process_qi_output(raw: String) -> String {
        let mut version = "unknown";
        let mut desc = "unknown";
        for line in raw.lines() {
            if line.starts_with("Version") {
                version = line.split(':').nth(1).unwrap_or("unknown").trim();
            } else if line.starts_with("Description") {
                desc = line.split(':').nth(1).unwrap_or("unknown").trim();
            }
        }
        format!("Version: {}, Description: {}", version, desc)
    }

    fn do_status(_logger: &Arc<Logger>, pm: PackageManager, pkgs: &[String]) -> Result<String, ToolError> {
        let mut results = Vec::new();
        for pkg in pkgs {
            match pm {
                PackageManager::Pacman => {
                    let output = Command::new("pacman").arg("-Qi").arg(pkg).output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                    if output.status.success() {
                        results.push(format!("{}: {}", pkg, Self::process_qi_output(String::from_utf8_lossy(&output.stdout).to_string())));
                    } else if Command::new("yay").arg("--version").output().is_ok() {
                         let aur = Command::new("yay").arg("-Qi").arg(pkg).output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                         if aur.status.success() {
                              results.push(format!("{} (AUR): {}", pkg, Self::process_qi_output(String::from_utf8_lossy(&aur.stdout).to_string())));
                         } else {
                             results.push(format!("{}: NOT_FOUND", pkg));
                         }
                    } else {
                        results.push(format!("{}: NOT_FOUND", pkg));
                    }
                }
                PackageManager::Apt => {
                     let out = Command::new("dpkg-query").arg("-W").arg("-f=${Version} | ${Description}").arg(pkg).output().ok();
                     if let Some(o) = out {
                         if o.status.success() {
                             results.push(format!("{}: {}", pkg, String::from_utf8_lossy(&o.stdout).trim()));
                         } else {
                             results.push(format!("{}: NOT_FOUND", pkg));
                         }
                     }
                }
                PackageManager::Dnf => {
                     let out = Command::new("rpm").arg("-q").arg("--qf").arg("%{VERSION} | %{SUMMARY}").arg(pkg).output().ok();
                     if let Some(o) = out {
                         if o.status.success() {
                             results.push(format!("{}: {}", pkg, String::from_utf8_lossy(&o.stdout).trim()));
                         } else {
                             results.push(format!("{}: NOT_FOUND", pkg));
                         }
                     }
                }
                _ => {
                    results.push(format!("{}: Status check not customized for this PM.", pkg));
                }
            }
        }
        Ok(results.join("\n"))
    }

    fn do_cleanup_orphans(logger: &Arc<Logger>, pm: PackageManager) -> Result<String, ToolError> {
        logger.log("SYSTEM", &format!("Cleaning up orphans via {}", pm.name()));
        match pm {
            PackageManager::Pacman => {
                let orphans = Command::new("pacman").args(&["-Qtdq"]).output().map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
                let list = String::from_utf8_lossy(&orphans.stdout).trim().to_string();
                if list.is_empty() {
                    Ok("No orphaned packages found.".to_string())
                } else {
                    let pkgs: Vec<&str> = list.split_whitespace().collect();
                    Self::run_filtered("cleanup", &pkgs.iter().map(|s| s.to_string()).collect::<Vec<_>>(), Command::new("pkexec").arg("pacman").arg("-Rns").arg("--noconfirm").args(pkgs))
                }
            }
            PackageManager::Apt => Ok(Self::run_filtered("cleanup", &[], Command::new("pkexec").arg("apt-get").arg("autoremove").arg("-y"))?),
            PackageManager::Dnf => Ok(Self::run_filtered("cleanup", &[], Command::new("pkexec").arg("dnf").arg("autoremove").arg("-y"))?),
            _ => Ok("Cleanup orphans not supported or needed for this package manager.".to_string()),
        }
    }

    fn do_clean_cache(logger: &Arc<Logger>, pm: PackageManager) -> Result<String, ToolError> {
        logger.log("SYSTEM", &format!("Cleaning cache via {}", pm.name()));
        match pm {
            PackageManager::Pacman => {
                 if Command::new("paccache").arg("--version").output().is_ok() {
                     Self::run_filtered("cache clean", &[], Command::new("pkexec").arg("paccache").arg("-r").arg("-k").arg("2"))
                 } else {
                     Self::run_filtered("cache clean", &[], Command::new("pkexec").arg("pacman").arg("-Sc").arg("--noconfirm"))
                 }
            }
            _ => {
                let mut cmd = Command::new("pkexec");
                cmd.arg(pm.name()).arg("clean");
                Self::run_filtered("cache clean", &[], &mut cmd)
            }
        }
    }

    fn do_purge_data(logger: &Arc<Logger>, pkgs: &[String]) -> Result<String, ToolError> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
        let mut purged = Vec::new();
        for pkg in pkgs {
            logger.log("SYSTEM", &format!("Purging data for {}", pkg));
            let paths = [
                format!("{}/.config/{}", home, pkg),
                format!("{}/.local/share/{}", home, pkg),
                format!("{}/.cache/{}", home, pkg),
            ];
            for path in &paths {
                if std::path::Path::new(path).exists() {
                    let _ = std::fs::remove_dir_all(path);
                    purged.push(path.clone());
                }
            }
        }
        if purged.is_empty() {
             Ok("No custom user data found to purge.".to_string())
        } else {
             Ok(format!("Successfully purged user data paths: {:?}", purged))
        }
    }
}
