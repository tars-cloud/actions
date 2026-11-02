{
  pkgs,
  rustPlatform,
  toolchain,
}:

let
  target = pkgs.stdenv.hostPlatform.rust.rustcTargetSpec;
in
(pkgs.cargo-llvm-cov.override { inherit rustPlatform; }).overrideAttrs (_: {
  env = {
    LLVM_COV = "${toolchain}/lib/rustlib/${target}/bin/llvm-cov";
    LLVM_PROFDATA = "${toolchain}/lib/rustlib/${target}/bin/llvm-profdata";
  };
})
