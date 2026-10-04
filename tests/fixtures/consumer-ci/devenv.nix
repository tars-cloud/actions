{
  config,
  lib,
  pkgs,
  ...
}:
{
  cachix = {
    enable = false;
  };
  packages = [
    pkgs.bash
    pkgs.git
    pkgs.prek
  ];
  enterTest = ''
    set -euo pipefail
    test "''${SECRETSPEC_PROVIDER:-}" = env || { echo 'Missing environment SecretSpec provider'; exit 1; }
    test "''${SECRETSPEC_PROFILE:-}" = ci || { echo 'Missing selected SecretSpec profile'; exit 1; }
  '';
  tasks = {
    "consumer:test" = lib.mkIf config.devenv.isTesting {
      before = [ "devenv:enterTest" ];
      exec = config.enterTest;
    };
  };
}
