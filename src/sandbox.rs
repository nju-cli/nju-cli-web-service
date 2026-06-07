use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use anyhow::{bail, Context};
use bollard::{
    errors::Error as DockerError,
    models::{ContainerCreateBody, HostConfig, PortBinding},
    query_parameters::{
        CreateContainerOptionsBuilder, InspectContainerOptions, StartContainerOptions,
    },
    Docker, API_DEFAULT_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    net::TcpStream,
    process::Command,
    time::{sleep, timeout},
};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    http::{header, HeaderValue, Request},
};
use tracing::{debug, info, warn};

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
            "docker" => self.ensure_docker(session_id).await,
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

        let ws_auth_token = self.codex_ws_auth_token(session_id)?;
        let ip = self.wait_for_lxc_ip(&name).await?;
        self.start_codex_in_lxc(&name, &ws_auth_token).await?;

        let endpoint = format!("ws://{}:{}", ip, self.config.codex_app_port);
        wait_for_websocket(&endpoint, &ws_auth_token, Duration::from_secs(45)).await?;

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

    async fn start_codex_in_lxc(&self, name: &str, ws_auth_token: &str) -> anyhow::Result<()> {
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
umask 077
printf '%s' "$CODEX_WS_AUTH_TOKEN" > /home/codex/.codex/ws-token
chown codex:users /home/codex/.codex/ws-token
chmod 600 /home/codex/.codex/ws-token
if ! pgrep -u codex -f 'codex app-server --listen {listen}' >/dev/null 2>&1; then
  sudo -H -u codex env CODEX_HOME=/home/codex/.codex OPENROUTER_API_KEY="$OPENROUTER_API_KEY" \
    sh -lc 'cd /home/codex/workspace && nohup codex app-server --listen {listen} --ws-auth capability-token --ws-token-file /home/codex/.codex/ws-token > /home/codex/codex-app-server.log 2>&1 &'
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
                .arg("--env")
                .arg(format!("CODEX_WS_AUTH_TOKEN={ws_auth_token}"))
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
        let ws_auth_token = self.codex_ws_auth_token(session_id)?;
        let token_file = codex_home.join("ws-token");
        tokio::fs::write(&token_file, &ws_auth_token).await?;

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
                .arg("--ws-auth")
                .arg("capability-token")
                .arg("--ws-token-file")
                .arg(&token_file)
                .env("CODEX_HOME", &codex_home)
                .env("OPENROUTER_API_KEY", api_key)
                .current_dir(state_dir.join("workspace"))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let _child = cmd
                .spawn()
                .context("failed to spawn local codex app-server")?;
        }
        wait_for_websocket(&endpoint, &ws_auth_token, Duration::from_secs(15)).await?;

        Ok(SandboxRecord {
            provider: "dev-local".to_owned(),
            name,
            codex_ws_url: endpoint,
        })
    }

    async fn ensure_docker(&self, session_id: &str) -> anyhow::Result<SandboxRecord> {
        let name = format!(
            "{}-{}",
            self.config.sandbox_name_prefix,
            short_hash(session_id)
        );
        debug!("Connecting to docker socket");
        let docker =
            Docker::connect_with_unix(&self.config.docker_socket, 120, API_DEFAULT_VERSION)
                .with_context(|| {
                    format!(
                        "failed to connect to Docker socket {}",
                        self.config.docker_socket
                    )
                })?;

        let inspect = match docker
            .inspect_container(&name, None::<InspectContainerOptions>)
            .await
        {
            Ok(inspect) => inspect,
            Err(DockerError::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                info!(%name, image = %self.config.docker_image, "creating Docker sandbox");
                let port = format!("{}/tcp", self.config.codex_app_port);
                let mut exposed_ports = std::collections::HashMap::new();
                exposed_ports.insert(port.clone(), std::collections::HashMap::new());

                let mut port_bindings = std::collections::HashMap::new();
                port_bindings.insert(
                    port.clone(),
                    Some(vec![PortBinding {
                        host_ip: Some(self.config.docker_host_bind_ip.clone()),
                        host_port: Some(String::new()),
                    }]),
                );

                let mut labels = std::collections::HashMap::new();
                labels.insert(
                    "im.ken.nju-cli-web-service.session".to_owned(),
                    short_hash(session_id),
                );

                let api_key = self
                    .config
                    .openrouter_api_key
                    .as_deref()
                    .context("OPENROUTER_API_KEY is required")?;
                let ws_auth_token = self.codex_ws_auth_token(session_id)?;
                docker
                    .create_container(
                        Some(CreateContainerOptionsBuilder::new().name(&name).build()),
                        ContainerCreateBody {
                            image: Some(self.config.docker_image.clone()),
                            env: Some(vec![
                                format!("OPENROUTER_API_KEY={api_key}"),
                                format!("CODEX_WS_AUTH_TOKEN={ws_auth_token}"),
                                "CODEX_HOME=/home/codex/.codex".to_owned(),
                                "HOME=/home/codex".to_owned(),
                                format!("CODEX_APP_PORT={}", self.config.codex_app_port),
                                format!("CODEX_APP_LISTEN={}", self.config.codex_app_listen),
                            ]),
                            exposed_ports: Some(exposed_ports),
                            host_config: Some(HostConfig {
                                port_bindings: Some(port_bindings),
                                ..Default::default()
                            }),
                            labels: Some(labels),
                            ..Default::default()
                        },
                    )
                    .await
                    .with_context(|| format!("failed to create Docker container {name}"))?;
                docker
                    .inspect_container(&name, None::<InspectContainerOptions>)
                    .await
                    .with_context(|| {
                        format!("failed to inspect Docker container {name} after create")
                    })?
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to inspect Docker container {name}"));
            }
        };

        if inspect.state.as_ref().and_then(|state| state.running) != Some(true) {
            let has_ws_auth_env = inspect
                .config
                .as_ref()
                .and_then(|config| config.env.as_ref())
                .is_some_and(|env| {
                    env.iter()
                        .any(|value| value.starts_with("CODEX_WS_AUTH_TOKEN="))
                });
            anyhow::ensure!(
                has_ws_auth_env,
                "Docker sandbox {name} was created before Codex WebSocket auth support; remove it and let nju-cli-web-service recreate it from the updated image"
            );
            info!(%name, "starting Docker sandbox");
            docker
                .start_container(&name, None::<StartContainerOptions>)
                .await
                .with_context(|| format!("failed to start Docker container {name}"))?;
        }

        let endpoint = self.wait_for_docker_endpoint(&docker, &name).await?;
        let ws_auth_token = self.codex_ws_auth_token(session_id)?;
        wait_for_websocket(&endpoint, &ws_auth_token, Duration::from_secs(45)).await?;

        Ok(SandboxRecord {
            provider: "docker".to_owned(),
            name,
            codex_ws_url: endpoint,
        })
    }

    async fn wait_for_docker_endpoint(
        &self,
        docker: &Docker,
        name: &str,
    ) -> anyhow::Result<String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(endpoint) = self.docker_endpoint(docker, name).await? {
                return Ok(endpoint);
            }
            if tokio::time::Instant::now() > deadline {
                bail!("timed out waiting for Docker port binding for {name}");
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    async fn docker_endpoint(&self, docker: &Docker, name: &str) -> anyhow::Result<Option<String>> {
        let port = format!("{}/tcp", self.config.codex_app_port);
        let inspect = docker
            .inspect_container(name, None::<InspectContainerOptions>)
            .await
            .with_context(|| format!("failed to inspect Docker container {name}"))?;
        let Some(ports) = inspect.network_settings.and_then(|settings| settings.ports) else {
            return Ok(None);
        };
        let Some(Some(bindings)) = ports.get(&port) else {
            return Ok(None);
        };
        let Some(binding) = bindings.first() else {
            return Ok(None);
        };
        let Some(host_port) = binding
            .host_port
            .as_deref()
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        let host = binding.host_ip.as_deref().unwrap_or("127.0.0.1");
        let host = match host {
            "" | "0.0.0.0" | "::" => "127.0.0.1",
            other => other,
        };
        Ok(Some(format!("ws://{host}:{host_port}")))
    }

    pub fn codex_ws_auth_token(&self, session_id: &str) -> anyhow::Result<String> {
        let api_key = self
            .config
            .openrouter_api_key
            .as_deref()
            .context("OPENROUTER_API_KEY is required")?;
        let mut hasher = Sha256::new();
        hasher.update(b"nju-cli-web-service codex app-server ws token\0");
        hasher.update(api_key.as_bytes());
        hasher.update(b"\0");
        hasher.update(session_id.as_bytes());
        Ok(format!("{:x}", hasher.finalize()))
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

[marketplaces.nju-cli]
source_type = "git"
source = "https://github.com/nju-cli/codex-marketplace.git"

[plugins."nju-cli@nju-cli"]
enabled = true
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

async fn wait_for_websocket(
    ws_url: &str,
    auth_token: &str,
    timeout_duration: Duration,
) -> anyhow::Result<()> {
    url::Url::parse(ws_url).with_context(|| format!("invalid WebSocket URL {ws_url}"))?;
    let deadline = tokio::time::Instant::now() + timeout_duration;
    let mut last_error = None;

    while tokio::time::Instant::now() <= deadline {
        match timeout(
            Duration::from_secs(3),
            connect_async(ws_request(ws_url, auth_token)?),
        )
        .await
        {
            Ok(Ok((_stream, _response))) => return Ok(()),
            Ok(Err(err)) => {
                warn!(%ws_url, error = %err, "waiting for codex app-server WebSocket handshake");
                last_error = Some(err.to_string());
            }
            Err(_) => {
                warn!(%ws_url, "waiting for codex app-server WebSocket handshake timed out");
                last_error = Some("handshake attempt timed out".to_owned());
            }
        }
        sleep(Duration::from_millis(500)).await;
    }

    bail!(
        "timed out waiting for {ws_url} WebSocket handshake{}",
        last_error
            .map(|err| format!(": last error: {err}"))
            .unwrap_or_default()
    )
}

pub fn ws_request(ws_url: &str, auth_token: &str) -> anyhow::Result<Request<()>> {
    let mut request = ws_url
        .into_client_request()
        .with_context(|| format!("invalid WebSocket URL {ws_url}"))?;
    let value = HeaderValue::from_str(&format!("Bearer {auth_token}"))
        .context("failed to build WebSocket Authorization header")?;
    request.headers_mut().insert(header::AUTHORIZATION, value);
    Ok(request)
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
