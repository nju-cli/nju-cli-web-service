{
  inputs,
  pkgs,
  ...
}:

{
  imports = [ inputs.home-manager.nixosModules.home-manager ];

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
    pkgs.cacert
    pkgs.codex
    pkgs.coreutils
    pkgs.git
    pkgs.procps
  ];
  environment.defaultPackages = [ ];

  home-manager.useGlobalPkgs = true;
  home-manager.useUserPackages = true;
  home-manager.users.codex = {
    home.stateVersion = "25.11";

    home.file.".codex/config.toml".text = ''
      model = "openai/gpt-oss-120b:free"
      model_provider = "openrouter"
      approval_policy = "on-request"
      sandbox_mode = "danger-full-access"
      model_reasoning_effort = "medium"
      model_instructions_file = "custom_instructions.md"

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
    '';

    home.file.".codex/custom_instructions.md".text = ''
      You are ChatNJU, a chat agent for NanJing University students.

      - You are inside codex, you can use tools to help accomplish user requests.
      - Give a direct answer for simple queries
      - For more complex or NJU-specific queries, you can work harder as an agent.
    '';

    home.file."workspace/.keep".text = "";
  };
}
