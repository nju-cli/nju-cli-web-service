{
  dockerTools,
  inputs,
  lib,
  pkgs,
  writeShellApplication,
}:

let
  system = pkgs.stdenv.hostPlatform.system;
  njuCli = inputs.nju-cli.packages.${system}.default;
  codexConfig = pkgs.writeText "codex-config.toml" ''
    model = "openai/gpt-oss-120b:free"
    model_provider = "openrouter"
    approval_policy = "on-request"
    sandbox_mode = "danger-full-access"
    model_reasoning_effort = "medium"

    [model_providers.openrouter]
    name = "OpenRouter"
    base_url = "https://openrouter.ai/api/v1"
    env_key = "OPENROUTER_API_KEY"
    wire_api = "responses"
  '';
  agents = pkgs.writeText "AGENTS.md" ''
    # NJU CLI Agent Docker Sandbox

    - You are running inside an isolated Docker sandbox for a single web user.
    - Use `nju-cli` for Nanjing University questions and workflows before falling back to generic web search.
    - Inspect `nju-cli --help` and subcommand help when you need exact command syntax.
    - Do not assume the user is logged in to NJU systems unless credentials or cookies are explicitly provided during the session.
  '';
  path = lib.makeBinPath [
    pkgs.bashInteractive
    pkgs.coreutils
    pkgs.curl
    pkgs.git
    pkgs.jq
    pkgs.ripgrep
    pkgs.codex
    njuCli
  ];
  start = writeShellApplication {
    name = "nju-cli-codex-docker-start";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.codex
    ];
    text = ''
      set -eu

      export HOME="''${HOME:-/home/codex}"
      export CODEX_HOME="''${CODEX_HOME:-/home/codex/.codex}"
      export PATH="${path}:$PATH"
      export SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
      export NIX_SSL_CERT_FILE="$SSL_CERT_FILE"

      mkdir -p "$CODEX_HOME" "$HOME/workspace"
      cp ${codexConfig} "$CODEX_HOME/config.toml"
      cp ${agents} "$HOME/workspace/AGENTS.md"
      chmod 700 "$CODEX_HOME"
      chmod 600 "$CODEX_HOME/config.toml"

      listen="ws://''${CODEX_APP_LISTEN:-0.0.0.0}:''${CODEX_APP_PORT:-4500}"
      cd "$HOME/workspace"
      exec codex app-server --listen "$listen"
    '';
  };
in
dockerTools.buildLayeredImage {
  name = "nju-cli-codex-docker";
  tag = "latest";
  contents = [
    pkgs.bashInteractive
    pkgs.cacert
    pkgs.coreutils
    pkgs.curl
    pkgs.git
    pkgs.jq
    pkgs.ripgrep
    pkgs.codex
    njuCli
  ];
  config = {
    Cmd = [ "${lib.getExe start}" ];
    Env = [
      "HOME=/home/codex"
      "CODEX_HOME=/home/codex/.codex"
      "CODEX_APP_LISTEN=0.0.0.0"
      "CODEX_APP_PORT=4500"
      "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
      "NIX_SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
      "PATH=${path}"
    ];
    ExposedPorts = {
      "4500/tcp" = { };
    };
    WorkingDir = "/home/codex/workspace";
  };
}

