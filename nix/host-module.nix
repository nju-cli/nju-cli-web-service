{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.nju-cli-web-service;
in
{
  options.services.nju-cli-web-service = {
    enable = lib.mkEnableOption "NJU CLI web service";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      description = "Package providing the nju-cli-web-service binary.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "nju-cli-web";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "nju-cli-web";
    };

    bindAddr = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:8080";
    };

    publicDir = lib.mkOption {
      type = lib.types.path;
      default = ../web;
    };

    sandboxProvider = lib.mkOption {
      type = lib.types.enum [
        "lxc"
        "docker"
        "dev-local"
      ];
      default = "lxc";
    };

    lxcImage = lib.mkOption {
      type = lib.types.str;
      default = "nju-cli-codex-lxc";
    };

    lxcProject = lib.mkOption {
      type = lib.types.str;
      default = "nju-cli-web";
    };

    dockerSocket = lib.mkOption {
      type = lib.types.str;
      default = "/var/run/docker.sock";
    };

    dockerImage = lib.mkOption {
      type = lib.types.str;
      default = "nju-cli-codex-docker:latest";
    };

    dockerHostBindIp = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
    };

    environmentFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = "/etc/nju-cli-web-service/openrouter.env";
      description = "Environment file containing OPENROUTER_API_KEY=...";
    };
  };

  config = lib.mkIf cfg.enable {
    users.groups.docker = { };
    users.groups.lxd = { };
    users.groups.${cfg.group} = { };
    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      extraGroups = [
        "docker"
        "lxd"
      ];
    };

    environment.systemPackages = [
      cfg.package
      pkgs.lxc
    ];

    systemd.services.nju-cli-web-service = {
      wantedBy = [ "multi-user.target" ];
      after = [
        "network-online.target"
        "docker.service"
        "lxd.service"
      ];
      wants = [ "network-online.target" ];
      serviceConfig = {
        User = cfg.user;
        Group = cfg.group;
        DynamicUser = false;
        Restart = "on-failure";
        RestartSec = "3s";
        Environment = [
          "BIND_ADDR=${cfg.bindAddr}"
          "PUBLIC_DIR=${cfg.publicDir}"
          "SANDBOX_PROVIDER=${cfg.sandboxProvider}"
          "LXC_IMAGE=${cfg.lxcImage}"
          "LXC_PROJECT=${cfg.lxcProject}"
          "DOCKER_SOCKET=${cfg.dockerSocket}"
          "DOCKER_IMAGE=${cfg.dockerImage}"
          "DOCKER_HOST_BIND_IP=${cfg.dockerHostBindIp}"
          "RUST_LOG=info,nju_cli_web_service=debug"
        ];
        ExecStart = "${lib.getExe cfg.package}";
      } // lib.optionalAttrs (cfg.environmentFile != null) {
        EnvironmentFile = cfg.environmentFile;
      };
    };
  };
}
