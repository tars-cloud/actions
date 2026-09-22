{
  system ? builtins.currentSystem,
}:
let
  lock = builtins.fromJSON (builtins.readFile ../devenv.lock);
  nixpkgs = builtins.fetchTree lock.nodes.nixpkgs.locked;
  overlay = builtins.fetchTree lock.nodes.rust-overlay.locked;
  pkgs = import nixpkgs {
    inherit system;
    overlays = [ (import overlay) ];
  };
  toolchain = pkgs.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
  platform = pkgs.makeRustPlatform {
    cargo = toolchain;
    rustc = toolchain;
  };
in
platform.buildRustPackage {
  pname = "tact";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../schemas
      ../internal
      ../setup-nix
      ../setup-devenv
      ../setup-trivy
      ../setup-cache
      ../free-disk-space
      ../.github
      ../devenv.nix
      ../devenv.yaml
      ../devenv.lock
    ];
  };
  cargoLock = {
    lockFile = ../Cargo.lock;
  };
  nativeCheckInputs = [
    pkgs.bash
    pkgs.coreutils
    pkgs.nodejs_24
  ];
  meta = {
    description = "Declarative tests for GitHub composite actions";
    mainProgram = "tact";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
