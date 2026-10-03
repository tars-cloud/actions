{ config, pkgs, ... }:
let
  source = ../../..;
  rustPlatform = pkgs.makeRustPlatform {
    cargo = config.languages.rust.toolchainPackage;
    rustc = config.languages.rust.toolchainPackage;
  };
in
{
  languages = {
    rust = {
      enable = true;
      toolchainFile = source + "/rust-toolchain.toml";
    };
  };
  packages = [
    (import (source + "/nix/packages/cargo-crap.nix") { inherit pkgs rustPlatform; })
    pkgs.cargo-llvm-cov
    pkgs.cargo-tarpaulin
    pkgs.git
    pkgs.coreutils
  ];
}
