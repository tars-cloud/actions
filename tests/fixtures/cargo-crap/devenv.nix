{ pkgs, ... }:
let
  source = ../../..;
in
{
  env = pkgs.lib.optionalAttrs (builtins.getEnv "CRAP_FIXTURE_LLVM_PROFDATA" != "") {
    LLVM_PROFDATA = builtins.getEnv "CRAP_FIXTURE_LLVM_PROFDATA";
  };
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
