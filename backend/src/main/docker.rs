use std::process::Command;
use anyhow::{Result, anyhow};
use std::path::Path;

pub struct DockerManager;

impl DockerManager {
    pub async fn check_searxng(config_path: &Path) -> Result<()> {
        // 1. Check if reachable
        if Self::is_searxng_up().await {
            return Ok(());
        }

        println!("SearXNG not reachable. Attempting auto-start via Docker...");

        // 2. Check if container exists
        let output = Command::new("docker")
            .args(["ps", "-a", "--filter", "name=searxng", "--format", "{{.Names}}"])
            .output()?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let exists = stdout.contains("searxng");

        if exists {
            println!("SearXNG container found. Starting...");
            let status = Command::new("docker").args(["start", "searxng"]).status()?;
            if status.success() {
                // Wait for init
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                if Self::is_searxng_up().await {
                    return Ok(());
                }
            }
            
            println!("SearXNG failed to start or misconfigured. Re-creating...");
            let _ = Command::new("docker").args(["rm", "-f", "searxng"]).status();
        }

        // 3. Create fresh instance
        let searxng_config_dir = config_path.join("searxng");
        let status = Command::new("docker")
            .args([
                "run", "-d", "--name", "searxng",
                "-p", "8888:8080",
                "-v", &format!("{}:/etc/searxng", searxng_config_dir.display()),
                "searxng/searxng"
            ])
            .status()?;
        
        if status.success() {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            if Self::is_searxng_up().await {
                return Ok(());
            }
        }

        Err(anyhow!("Failed to start SearXNG via Docker"))
    }

    pub async fn is_searxng_up() -> bool {
        let client = reqwest::Client::new();
        let res = client.get("http://localhost:8888/search")
            .query(&[("q", "ping"), ("format", "json")])
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;
        
        matches!(res, Ok(r) if r.status().is_success())
    }
}
