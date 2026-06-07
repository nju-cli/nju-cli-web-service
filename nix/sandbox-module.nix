{
  config,
  lib,
  pkgs,
  inputs,
  ...
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
    # NJU CLI Agent Sandbox

    - You are running inside an isolated sandbox for a single web user.
    - Use `nju-cli` for Nanjing University questions and workflows before falling back to generic web search.
    - Inspect `nju-cli --help` and subcommand help when you need exact command syntax.
    - Do not assume the user is logged in to NJU systems unless credentials or cookies are explicitly provided during the session.
  '';
in
{
  system.stateVersion = "25.11";

  networking.hostName = "nju-cli-codex-sandbox";
  networking.firewall.allowedTCPPorts = [ 4500 ];

  users.users.codex = {
    isNormalUser = true;
    home = "/home/codex";
    createHome = true;
    group = "users";
    extraGroups = [ "wheel" ];
  };

  security.sudo.enable = true;
  security.sudo.wheelNeedsPassword = false;

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  environment.systemPackages = [
    pkgs.bashInteractive
    pkgs.cacert
    pkgs.codex
    pkgs.coreutils
    pkgs.curl
    pkgs.git
    pkgs.jq
    njuCli
    pkgs.ripgrep
  ];

  system.activationScripts.codexSandboxHome.text = ''
    install -d -m 0755 -o codex -g users /home/codex
    install -d -m 0700 -o codex -g users /home/codex/.codex
    install -m 0600 -o codex -g users ${codexConfig} /home/codex/.codex/config.toml
    install -d -m 0755 -o codex -g users /home/codex/workspace
    install -m 0644 -o codex -g users ${agents} /home/codex/workspace/AGENTS.md
  '';
}
