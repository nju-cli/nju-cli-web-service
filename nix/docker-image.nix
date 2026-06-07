{
  dockerTools,
  inputs,
  lib,
  pkgs,
  writeShellApplication,
}:

let
  system = pkgs.stdenv.hostPlatform.system;
  njuCli = inputs.nju-cli.packages.${system}.nju-cli-static;
  codexShared = import ./codex-shared.nix { inherit pkgs; };
  path = lib.makeBinPath [
    pkgs.bashInteractive
    pkgs.coreutils
    pkgs.curl
    pkgs.git
    pkgs.jq
    pkgs.poppler-utils
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

      if [ -z "''${CODEX_WS_AUTH_TOKEN:-}" ]; then
        echo "CODEX_WS_AUTH_TOKEN is required" >&2
        exit 1
      fi

      mkdir -p "$CODEX_HOME" "$HOME/workspace"
      cp ${codexShared.codexConfigFile} "$CODEX_HOME/config.toml"
      cp ${codexShared.customInstructionsFile} "$CODEX_HOME/custom_instructions.md"
      printf '%s' "$CODEX_WS_AUTH_TOKEN" > "$CODEX_HOME/ws-token"
      chmod 700 "$CODEX_HOME"
      chmod 600 "$CODEX_HOME/config.toml"
      chmod 600 "$CODEX_HOME/custom_instructions.md"
      chmod 600 "$CODEX_HOME/ws-token"

      listen="ws://''${CODEX_APP_LISTEN:-0.0.0.0}:''${CODEX_APP_PORT:-4500}"
      cd "$HOME/workspace"
      exec codex app-server --listen "$listen" --ws-auth capability-token --ws-token-file "$CODEX_HOME/ws-token"
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
    pkgs.poppler-utils
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
