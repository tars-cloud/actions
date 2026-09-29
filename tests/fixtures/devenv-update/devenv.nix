{ pkgs, ... }:
{
  cachix = {
    enable = false;
  };
  packages = [
    pkgs.bash
  ];
  env = {
    UPDATE_FIXTURE = "direct";
  };
  enterTest = ''
    test "$UPDATE_FIXTURE" = direct
  '';
}
