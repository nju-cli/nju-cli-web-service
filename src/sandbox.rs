use std::{net::SocketAddr, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{net::TcpStream, process::Command, time::sleep};
use tracing::{info, warn};

use crate::config::Config;

#[derive(Debug, Clone, Serialize)]
pub struct SandboxRecord {
    pub provider: String,
    pub name: String,
    pub codex_ws_url: String,
}

pub struct SandboxManager {
    config: Arc<Config>,
}

impl SandboxManager {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    pub async fn ensure(&self, session_id: &str) -> anyhow::Result<SandboxRecord> {
        match self.config.sandbox_provider.as_str() {
            "lxc" => self.ensure_lxc(session_id).await,
            "dev-local" => self.ensure_dev_local(session_id).await,
            other => bail!("unsupported sandbox provider {other}"),
        }
    }

    async fn ensure_lxc(&self, session_id: &str) -> anyhow::Result<SandboxRecord> {
        let name = format!(
            "{}-{}",
            self.config.sandbox_name_prefix,
            short_hash(session_id)
        );

        if !self.lxc_exists(&name).await? {
            info!(%name, image = %self.config.lxc_image, "creating LXC sandbox");
            command_ok(
                Command::new(&self.config.lxc_bin)
                    .arg("launch")
                    .arg(&self.config.lxc_image)
                    .arg(&name)
                    .arg("--project")
                    .arg(&self.config.lxc_project),
            )
            .await
            .with_context(|| format!("failed to launch LXC instance {name}"))?;
        } else {
            let status = self.lxc_status(&name).await?;
            if status != "Running" {
                info!(%name, %status, "starting existing LXC sandbox");
                command_ok(
                    Command::new(&self.config.lxc_bin)
                        .arg("start")
                        .arg(&name)
                        .arg("--project")
                        .arg(&self.config.lxc_project),
                )
                .await
                .with_context(|| format!("failed to start LXC instance {name}"))?;
            }
        }

        let ip = self.wait_for_lxc_ip(&name).await?;
        self.start_codex_in_lxc(&name).await?;

        let endpoint = format!("ws://{}:{}", ip, self.config.codex_app_port);
        wait_for_tcp(&endpoint, Duration::from_secs(45)).await?;

        Ok(SandboxRecord {
            provider: "lxc".to_owned(),
            name,
            codex_ws_url: endpoint,
        })
    }

    async fn lxc_exists(&self, name: &str) -> anyhow::Result<bool> {
        let status = Command::new(&self.config.lxc_bin)
            .arg("info")
            .arg(name)
            .arg("--project")
            .arg(&self.config.lxc_project)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;
        Ok(status.success())
    }

    async fn lxc_status(&self, name: &str) -> anyhow::Result<String> {
        let output = Command::new(&self.config.lxc_bin)
            .arg("list")
            .arg(name)
            .arg("--project")
            .arg(&self.config.lxc_project)
            .arg("--format")
            .arg("json")
            .output()
            .await?;
        anyhow::ensure!(
            output.status.success(),
            "lxc list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let instances: Vec<LxcInstance> = serde_json::from_slice(&output.stdout)?;
        Ok(instances
            .first()
            .and_then(|instance| instance.status.clone())
            .unwrap_or_else(|| "Unknown".to_owned()))
    }

    async fn wait_for_lxc_ip(&self, name: &str) -> anyhow::Result<String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(ip) = self.lxc_ip(name).await? {
                return Ok(ip);
            }
            if tokio::time::Instant::now() > deadline {
                bail!("timed out waiting for IP address for LXC instance {name}");
            }
            sleep(Duration::from_millis(750)).await;
        }
    }

    async fn lxc_ip(&self, name: &str) -> anyhow::Result<Option<String>> {
        let output = Command::new(&self.config.lxc_bin)
            .arg("list")
            .arg(name)
            .arg("--project")
            .arg(&self.config.lxc_project)
            .arg("--format")
            .arg("json")
            .output()
            .await?;
        anyhow::ensure!(
            output.status.success(),
            "lxc list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let instances: Vec<LxcInstance> = serde_json::from_slice(&output.stdout)?;
        Ok(instances
            .first()
            .and_then(|instance| instance.network.as_ref())
            .and_then(|networks| {
                networks.values().find_map(|network| {
                    network.addresses.iter().find_map(|address| {
                        (address.family == "inet" && address.scope.as_deref() == Some("global"))
                            .then(|| address.address.clone())
                    })
                })
            }))
    }

    async fn start_codex_in_lxc(&self, name: &str) -> anyhow::Result<()> {
        let api_key = self
            .config
            .openrouter_api_key
            .as_deref()
            .context("OPENROUTER_API_KEY is required")?;
        let listen = format!(
            "ws://{}:{}",
            self.config.codex_app_listen, self.config.codex_app_port
        );
        let shell = format!(
            r#"set -eu
mkdir -p /home/codex/workspace /home/codex/.codex
chown -R codex:users /home/codex
if ! pgrep -u codex -f 'codex app-server --listen {listen}' >/dev/null 2>&1; then
  sudo -H -u codex env CODEX_HOME=/home/codex/.codex OPENROUTER_API_KEY="$OPENROUTER_API_KEY" \
    sh -lc 'cd /home/codex/workspace && nohup codex app-server --listen {listen} > /home/codex/codex-app-server.log 2>&1 &'
fi
"#
        );
        command_ok(
            Command::new(&self.config.lxc_bin)
                .arg("exec")
                .arg(name)
                .arg("--project")
                .arg(&self.config.lxc_project)
                .arg("--env")
                .arg(format!("OPENROUTER_API_KEY={api_key}"))
                .arg("--")
                .arg("sh")
                .arg("-lc")
                .arg(shell),
        )
        .await
        .with_context(|| format!("failed to start codex app-server in {name}"))
    }

    async fn ensure_dev_local(&self, session_id: &str) -> anyhow::Result<SandboxRecord> {
        let suffix = short_hash(session_id);
        let name = format!("dev-local-{suffix}");
        let offset = u16::from_str_radix(&suffix[..4], 16).unwrap_or(0) % 1000;
        let port = self.config.codex_app_port.saturating_add(offset);
        let state_dir = PathBuf::from(&self.config.dev_local_state_dir).join(&name);
        let codex_home = state_dir.join(".codex");
        tokio::fs::create_dir_all(&codex_home).await?;
        tokio::fs::create_dir_all(state_dir.join("workspace")).await?;
        tokio::fs::write(
            codex_home.join("config.toml"),
            codex_config(&self.config.codex_model),
        )
        .await?;

        let endpoint = format!("ws://127.0.0.1:{port}");
        if TcpStream::connect(("127.0.0.1", port)).await.is_err() {
            let api_key = self
                .config
                .openrouter_api_key
                .as_deref()
                .context("OPENROUTER_API_KEY is required")?;
            info!(%endpoint, "starting dev-local codex app-server");
            let mut cmd = Command::new("codex");
            cmd.arg("app-server")
                .arg("--listen")
                .arg(&endpoint)
                .env("CODEX_HOME", &codex_home)
                .env("OPENROUTER_API_KEY", api_key)
                .current_dir(state_dir.join("workspace"))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let _child = cmd
                .spawn()
                .context("failed to spawn local codex app-server")?;
            wait_for_tcp(&endpoint, Duration::from_secs(15)).await?;
        }

        Ok(SandboxRecord {
            provider: "dev-local".to_owned(),
            name,
            codex_ws_url: endpoint,
        })
    }
}

fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

pub fn codex_config(model: &str) -> String {
    format!(
        r#"model = "{model}"
model_provider = "openrouter"
approval_policy = "on-request"
sandbox_mode = "danger-full-access"
model_reasoning_effort = "medium"

[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "responses"
"#
    )
}

async fn command_ok(command: &mut Command) -> anyhow::Result<()> {
    let output = command.output().await?;
    if output.status.success() {
        return Ok(());
    }

    bail!(
        "command failed with status {}: stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

async fn wait_for_tcp(ws_url: &str, timeout: Duration) -> anyhow::Result<()> {
    let url = url::Url::parse(ws_url)?;
    let host = url.host_str().context("missing host in WebSocket URL")?;
    let port = url
        .port_or_known_default()
        .context("missing port in WebSocket URL")?;
    let addr = format!("{host}:{port}")
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid upstream address {host}:{port}"))?;
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(err) if tokio::time::Instant::now() <= deadline => {
                warn!(%ws_url, error = %err, "waiting for codex app-server");
                sleep(Duration::from_millis(500)).await;
            }
            Err(err) => return Err(err).with_context(|| format!("timed out waiting for {ws_url}")),
        }
    }
}

#[derive(Debug, Deserialize)]
struct LxcInstance {
    status: Option<String>,
    network: Option<std::collections::HashMap<String, LxcNetwork>>,
}

#[derive(Debug, Deserialize)]
struct LxcNetwork {
    addresses: Vec<LxcAddress>,
}

#[derive(Debug, Deserialize)]
struct LxcAddress {
    family: String,
    address: String,
    scope: Option<String>,
}
