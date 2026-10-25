{ pkgs, ... }: {
  cachix = { enable = false; };
  packages = [ pkgs.bash ];
  env = { UPDATE_REVISION = builtins.toString ((builtins.fromJSON (builtins.readFile ./devenv.lock)).nodes.nixpkgs.locked.lastModified); };
  enterTest = ''
    test "$UPDATE_REVISION" != 1
    printf excluded > generated-file
  '';
}
