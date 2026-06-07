# NJU CLI Web Service

一个面向 `nju-cli` 的 Web agent 服务。浏览器侧直接连接 Codex app-server 的 JSON-RPC/WebSocket 事件流；Rust 服务端只负责匿名 cookie、用户到 sandbox 的映射、sandbox 生命周期，以及 WebSocket 透传。

## 设计

- 不需要登录。首次访问会设置 `nju_cli_agent_sid` cookie。
- 每个 cookie 对应一个 sandbox。默认 provider 是 `lxc`，也支持 `docker` 和 `dev-local`，sandbox 名由 cookie hash 派生。
- sandbox 镜像由 Nix flake 生成，预装 `nju-cli`、`codex`、`git`、`rg`、`curl` 等工具。
- sandbox 内的 Codex 配置固定使用 OpenRouter 免费模型：

  ```toml
  model = "openai/gpt-oss-120b:free"
  model_provider = "openrouter"

  [model_providers.openrouter]
  base_url = "https://openrouter.ai/api/v1"
  env_key = "OPENROUTER_API_KEY"
  wire_api = "responses"
  ```

- OpenRouter key 只从运行时环境变量 `OPENROUTER_API_KEY` 读取，不写进仓库。
- Codex app-server 在 sandbox 内监听 `ws://0.0.0.0:4500`，宿主 Rust 服务把 `/ws/codex` 透传到对应 sandbox。

## 本地开发

```bash
nix develop
cargo run -- --sandbox-provider dev-local --public-dir web
```

`dev-local` 只用于没有 LXC 的机器上调试 Web UI 和 Codex app-server 协议；它不会提供真正的虚拟化隔离。生产默认使用 `lxc`。

## Linux / Docker 运行

Docker backend 直接通过 Docker Engine API 访问 `DOCKER_SOCKET`，不依赖 `docker` CLI。每个 cookie 会创建或复用一个容器，并把容器内 `4500/tcp` 随机映射到宿主 `127.0.0.1` 端口，Rust 服务再代理到这个端口。

准备环境变量：

```bash
export OPENROUTER_API_KEY=...
```

构建并导入 Docker sandbox 镜像。导入脚本同样走 Docker socket 的 `/images/load` API，不调用 `docker` CLI：

```bash
./scripts/load-docker-image.sh
```

在 macOS 上，Docker image 仍然是 Linux image；脚本会按机器架构构建 `.#packages.aarch64-linux.dockerImage` 或 `.#packages.x86_64-linux.dockerImage`。这需要可用的 Linux Nix builder，比如 Orb VM；没有 Linux builder 时请在 Linux 环境里执行。

启动服务：

```bash
DOCKER_IMAGE=nju-cli-codex-docker:latest \
OPENROUTER_API_KEY="$OPENROUTER_API_KEY" \
cargo run -- --sandbox-provider docker --bind-addr 0.0.0.0:8080 --public-dir web
```

## Linux / LXC 运行

准备环境变量：

```bash
export OPENROUTER_API_KEY=...
```

构建并导入 LXC sandbox 镜像：

```bash
nix build .#lxcImage
./scripts/import-lxc-image.sh nju-cli-codex-lxc
```

启动服务：

```bash
LXC_PROJECT=nju-cli-web \
LXC_IMAGE=nju-cli-codex-lxc \
OPENROUTER_API_KEY="$OPENROUTER_API_KEY" \
cargo run -- --bind-addr 0.0.0.0:8080 --public-dir web
```

首次浏览器连接会创建对应 LXC instance，并在其中启动：

```bash
codex app-server --listen ws://0.0.0.0:4500
```

## VM 镜像

同一个 NixOS module 也导出 QCOW 镜像：

```bash
nix build .#vmImage
```

当前 Rust 服务实现默认管理 LXC provider；QCOW 输出用于后续接入 firecracker/cloud-hypervisor/libvirt 之类的轻量 VM runner。sandbox 内系统配置和 Codex/NJU CLI 预装内容与 LXC 镜像一致。

## NixOS host module

flake 导出 `nixosModules.host`：

```nix
{
  imports = [ inputs.nju-cli-web-service.nixosModules.host ];

  services.nju-cli-web-service = {
    enable = true;
    bindAddr = "0.0.0.0:8080";
    sandboxProvider = "docker";
    dockerImage = "nju-cli-codex-docker:latest";
    lxcImage = "nju-cli-codex-lxc";
    lxcProject = "nju-cli-web";
    environmentFile = "/etc/nju-cli-web-service/openrouter.env";
  };
}
```

`/etc/nju-cli-web-service/openrouter.env` 内容：

```bash
OPENROUTER_API_KEY=...
```

不要把这个文件提交到 git。

## 配置

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `BIND_ADDR` | `127.0.0.1:8080` | Rust 服务监听地址 |
| `PUBLIC_DIR` | `web` | 静态前端目录 |
| `SESSION_COOKIE` | `nju_cli_agent_sid` | 匿名 session cookie 名 |
| `SANDBOX_PROVIDER` | `lxc` | `lxc`、`docker` 或 `dev-local` |
| `LXC_BIN` | `lxc` | LXC/Incus CLI |
| `LXC_IMAGE` | `nju-cli-codex-lxc` | sandbox 镜像 alias |
| `LXC_PROJECT` | `nju-cli-web` | LXC project |
| `DOCKER_SOCKET` | `/var/run/docker.sock` | Docker Engine Unix socket |
| `DOCKER_IMAGE` | `nju-cli-codex-docker:latest` | Docker sandbox image |
| `DOCKER_HOST_BIND_IP` | `127.0.0.1` | 容器端口映射到宿主的监听 IP |
| `CODEX_APP_PORT` | `4500` | sandbox 内 Codex app-server 端口 |
| `CODEX_MODEL` | `openai/gpt-oss-120b:free` | 必须是 `:free` 结尾的 OpenRouter 免费模型 |
| `OPENROUTER_API_KEY` | 必填 | 运行时注入，不提交 |
