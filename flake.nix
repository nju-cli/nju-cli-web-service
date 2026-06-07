{
  description = "Web agent service for nju-cli backed by per-cookie Codex sandboxes";

  # Keep this in sync with the nju-cli input flake. Nix does not apply
  # nixConfig transitively from flake inputs.
  nixConfig = {
    extra-substituters = [
      "https://nix-binary-cache.ken.com.im/nju-cli"
    ];
    extra-trusted-public-keys = [
      "nju-cli:DRWFBO6JKN1QLv2w+o/BgW42BDnBDzebWTP+cwQh71w="
      "nju-cli-cache-1:qG9SW6IO+FJgaSAZraau16eX5aKE+umrhI9oV+K1aHM="
      "ken.com.im:br/oG6ywHr+tGvmUpZEA5mVYSNZgrNrFflazAEI+AK4="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nixos-generators = {
      url = "github:nix-community/nixos-generators";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nju-cli.url = "github:nju-cli/nju-cli";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-utils,
      nixos-generators,
      ...
    }:
    let
      lib = nixpkgs.lib;
      perSystem = flake-utils.lib.eachDefaultSystem (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          packages = {
            default = pkgs.callPackage ./nix/package.nix { };
            nju-cli-web-service = self.packages.${system}.default;
          }
          // lib.optionalAttrs pkgs.stdenv.isLinux {
            dockerImage = pkgs.callPackage ./nix/docker-image.nix {
              inherit inputs;
            };
          };

          devShells.default = pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.clippy
              pkgs.rustc
              pkgs.rustfmt
              pkgs.pkg-config
              pkgs.nixfmt-rfc-style
              pkgs.codex
            ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              pkgs.lxc
              pkgs.nixos-generators
            ];

            RUST_LOG = "warn,nju_cli_web_service=debug";
          };

          formatter = pkgs.nixfmt-rfc-style;
        }
      );

      imageOutputs =
        let
          mkImages =
            system:
            let
              pkgs = import nixpkgs { inherit system; };
              specialArgs = { inherit inputs; };
              modules = [ self.nixosModules.sandbox ];
            in
            {
              lxcImage = nixos-generators.nixosGenerate {
                inherit system modules specialArgs;
                format = "lxc";
              };
              vmImage = nixos-generators.nixosGenerate {
                inherit system modules specialArgs;
                format = "qcow";
              };
              dockerImage = pkgs.callPackage ./nix/docker-image.nix {
                inherit inputs;
              };
            };
        in
        {
          packages.x86_64-linux = mkImages "x86_64-linux";
          packages.aarch64-linux = mkImages "aarch64-linux";
        };
    in
    lib.recursiveUpdate perSystem imageOutputs
    // {
      nixosModules = {
        sandbox = import ./nix/sandbox-module.nix;
        host = import ./nix/host-module.nix;
      };
    };
}
