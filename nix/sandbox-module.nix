{
  inputs,
  pkgs,
  ...
}:

let
  codexShared = import ./codex-shared.nix { inherit pkgs; };
in
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
    pkgs.poppler-utils
    pkgs.procps
  ];
  environment.defaultPackages = [ ];

  home-manager.useGlobalPkgs = true;
  home-manager.useUserPackages = true;
  home-manager.users.codex = {
    home.stateVersion = "25.11";

    home.file.".codex/config.toml".text = codexShared.codexConfigText;

    home.file.".codex/model-catalog.json".text = codexShared.modelCatalogText;

    home.file.".codex/custom_instructions.md".text = codexShared.customInstructionsText;

    home.file.".codex/agents/image-understanding.toml".text = codexShared.imageUnderstandingAgentText;

    home.file."workspace/.keep".text = "";
  };
}
