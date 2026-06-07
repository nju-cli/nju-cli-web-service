{
  description = "Web agent service for nju-cli backed by per-cookie Codex sandboxes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nixos-generators.url = "github:nix-community/nixos-generators";
    nju-cli.url = "https://github.com/nju-cli/nju-cli/archive/refs/heads/main.tar.gz";
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
