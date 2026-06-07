{
  lib,
  rustPlatform,
}:

rustPlatform.buildRustPackage {
  pname = "nju-cli-web-service";
  version = "0.1.0";

  src = lib.cleanSourceWith {
    src = ../.;
    filter =
      path: type:
      let
        base = baseNameOf path;
      in
      !(lib.elem base [
        ".git"
        "target"
        "result"
        "result-lxc"
        "result-vm"
      ]);
  };

  cargoLock.lockFile = ../Cargo.lock;
}

