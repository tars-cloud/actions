{
  inputs = {
    nixpkgs = {
      url = "github:cachix/devenv-nixpkgs/c2f38fe7f9e04d9aadd354d380f2bd40531d9737";
    };
  };
  outputs =
    { nixpkgs, ... }:
    let
      source = builtins.toPath (builtins.getEnv "TARS_ACTIONS_ROOT");
      lock = builtins.fromJSON (builtins.readFile (source + "/devenv.lock"));
      overlay = builtins.fetchTree lock.nodes.rust-overlay.locked;
    in
    {
      devShells = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import overlay) ];
          };
          rust = pkgs.rust-bin.fromRustupToolchainFile (source + "/rust-toolchain.toml");
        in
        {
          named = pkgs.mkShell {
            packages = [
              rust
              pkgs.cargo-llvm-cov
              pkgs.cargo-tarpaulin
              pkgs.git
              pkgs.coreutils
            ];
          };
        }
      );
    };
}
