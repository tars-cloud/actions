{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

{
  packages = with pkgs; [
    git
    trivy
  ];

  languages = {
    nix.enable = true;
    rust.enable = true;
  };

  enterTest = ''
    echo "Running tests"
  '';

}
