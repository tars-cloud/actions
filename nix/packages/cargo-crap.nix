{ pkgs, rustPlatform }:
let
  lock = builtins.fromTOML (builtins.readFile ../../Cargo.lock);
  tools = builtins.filter (
    package:
    package.name == "cargo-crap"
    && (package.source or "") == "registry+https://github.com/rust-lang/crates.io-index"
  ) lock.package;
  tool =
    if builtins.length tools == 1 then
      builtins.head tools
    else
      throw "Root Cargo.lock must resolve exactly one crates.io cargo-crap version.";
  archive = pkgs.fetchCrate {
    pname = "cargo-crap";
    inherit (tool) version;
    sha256 = tool.checksum;
    unpack = false;
  };
  source =
    pkgs.runCommand "cargo-crap-${tool.version}-source"
      {
        nativeBuildInputs = [
          pkgs.gnutar
          pkgs.gzip
        ];
      }
      ''
        mkdir -p "$out"
        tar -xzf "${archive}" --strip-components=1 -C "$out"
      '';
in
rustPlatform.buildRustPackage {
  pname = "cargo-crap";
  inherit (tool) version;
  src = source;
  cargoLock = {
    lockFile = "${source}/Cargo.lock";
  };
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
