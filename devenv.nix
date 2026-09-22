{
  pkgs,
  config,
  lib,
  ...
}:

{
  packages = with pkgs; [
    git
    trivy
    actionlint
    shellcheck
    shfmt
    yamllint
    markdownlint-cli
    jq
    ripgrep
    (python3.withPackages (ps: [ ps.pyyaml ]))
    bun
    nodejs_24
    prek
  ];

  languages = {
    nix.enable = true;
    rust.enable = true;
  };

  git-hooks.hooks = {
    actionlint.enable = true;
    markdownlint.enable = true;
    nixfmt.enable = true;
    shellcheck.enable = true;
    shfmt.enable = true;
    yamllint.enable = true;
  };

  enterTest = ''
    node --test tests/*.test.cjs
    python3 tests/metadata.py
    bash tests/static.sh
  '';

  tasks."actions:test" = lib.mkIf config.devenv.isTesting {
    description = "Validate shared action behaviour and metadata";
    before = [ "devenv:enterTest" ];
    exec = config.enterTest;
  };
}
