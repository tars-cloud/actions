{
  system ? builtins.currentSystem,
}:
let
  lock = builtins.fromJSON (builtins.readFile ../../devenv.lock);
  nixpkgs = builtins.fetchTree lock.nodes.nixpkgs.locked;
  overlay = builtins.fetchTree lock.nodes.rust-overlay.locked;
  pkgs = import nixpkgs {
    inherit system;
    overlays = [ (import overlay) ];
  };
  toolchain = pkgs.rust-bin.fromRustupToolchainFile ../../rust-toolchain.toml;
  platform = pkgs.makeRustPlatform {
    cargo = toolchain;
    rustc = toolchain;
  };
in
platform.buildRustPackage {
  pname = "tact";
  version = (builtins.fromTOML (builtins.readFile ../../Cargo.toml)).workspace.package.version;
  src = pkgs.lib.fileset.toSource {
    root = ../../.;
    fileset = pkgs.lib.fileset.unions [
      ../../Cargo.toml
      ../../Cargo.lock
      ../../.convco
      ../../crates
      ../../schemas
      ../../composite
      ../../.github
      ../../devenv.nix
      ../../devenv.yaml
      ../../devenv.lock
    ];
  };
  cargoLock = {
    lockFile = ../../Cargo.lock;
  };
  nativeCheckInputs = [
    pkgs.bash
    pkgs.coreutils
    pkgs.nodejs_24
    pkgs.git
    pkgs.prettier
    (import ./convco.nix {
      inherit pkgs;
      rustPlatform = platform;
    })
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
