{ pkgs, ... }:
let
  source = ../../..;
in
{
  languages = {
    rust = {
      enable = true;
      toolchainFile = source + "/rust-toolchain.toml";
    };
  };
  packages = [
    pkgs.cargo-llvm-cov
    pkgs.cargo-tarpaulin
    pkgs.git
    pkgs.coreutils
  ];
}
