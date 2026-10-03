{ pkgs, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "cargo-crap";
  version = "0.6.1";
  src = pkgs.fetchFromGitHub {
    owner = "minikin";
    repo = "cargo-crap";
    rev = "a6693a1f3040c21c9d51ec2d4a4b40ed328a2c26";
    hash = "sha256-8Ex8m/xdrh4PrWlZoWoBm/K0kXrY1NN0lOb1GsL5OIA=";
  };
  cargoLock = {
    lockFile = ./cargo-crap/Cargo.lock;
  };
  postPatch = ''
    cp ${./cargo-crap/Cargo.lock} Cargo.lock
  '';
  nativeCheckInputs = [ pkgs.git ];
  meta = {
    description = "Change Risk Anti-Patterns metric for Rust";
    mainProgram = "cargo-crap";
    license = pkgs.lib.licenses.mit;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
