use anyhow::Context;
use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Config {
    #[arg(long, env = "BIND_ADDR", default_value = "127.0.0.1:8080")]
    pub bind_addr: String,

    #[arg(long, env = "PUBLIC_DIR", default_value = "web")]
    pub public_dir: String,

    #[arg(long, env = "SESSION_COOKIE", default_value = "nju_cli_agent_sid")]
    pub session_cookie: String,

    #[arg(long, env = "SESSION_MAX_AGE_SECONDS", default_value_t = 60 * 60 * 24 * 30)]
    pub session_max_age_seconds: u64,

    #[arg(long, env = "SANDBOX_PROVIDER", default_value = "lxc")]
    pub sandbox_provider: String,

    #[arg(long, env = "LXC_BIN", default_value = "lxc")]
    pub lxc_bin: String,

    #[arg(long, env = "LXC_IMAGE", default_value = "nju-cli-codex-lxc")]
    pub lxc_image: String,

    #[arg(long, env = "LXC_PROJECT", default_value = "nju-cli-web")]
    pub lxc_project: String,

    #[arg(long, env = "DOCKER_SOCKET", default_value = "/var/run/docker.sock")]
    pub docker_socket: String,

    #[arg(
        long,
        env = "DOCKER_IMAGE",
        default_value = "nju-cli-codex-docker:latest"
    )]
    pub docker_image: String,

    #[arg(long, env = "DOCKER_HOST_BIND_IP", default_value = "127.0.0.1")]
    pub docker_host_bind_ip: String,

    #[arg(long, env = "SANDBOX_NAME_PREFIX", default_value = "nju-agent")]
    pub sandbox_name_prefix: String,

    #[arg(long, env = "CODEX_APP_PORT", default_value_t = 4500)]
    pub codex_app_port: u16,

    #[arg(long, env = "CODEX_APP_LISTEN", default_value = "0.0.0.0")]
    pub codex_app_listen: String,

    #[arg(
        long,
        env = "CODEX_MODEL",
        default_value = "nvidia/nemotron-3-ultra-550b-a55b:free"
    )]
    pub codex_model: String,

    #[arg(long, env = "OPENROUTER_API_KEY")]
    pub openrouter_api_key: Option<String>,

    #[arg(
        long,
        env = "DEV_LOCAL_STATE_DIR",
        default_value = "/tmp/nju-cli-web-service-sessions"
    )]
    pub dev_local_state_dir: String,
}

impl Config {
    pub fn normalized(mut self) -> anyhow::Result<Self> {
        self.sandbox_provider = self.sandbox_provider.to_ascii_lowercase();
        anyhow::ensure!(
            matches!(
                self.sandbox_provider.as_str(),
                "lxc" | "docker" | "dev-local"
            ),
            "SANDBOX_PROVIDER must be lxc, docker, or dev-local"
        );
        anyhow::ensure!(
            self.codex_model.ends_with(":free"),
            "CODEX_MODEL must be an OpenRouter free model and end with :free"
        );
        self.openrouter_api_key
            .as_ref()
            .context("OPENROUTER_API_KEY is required at runtime")?;
        Ok(self)
    }
}
