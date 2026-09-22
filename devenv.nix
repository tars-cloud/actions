{
  config,
  lib,
  pkgs,
  ...
}:

{

  name = "actions";

  env = {
    PROJECT = config.name;
  };

  cachix = {
    pull = [
      "tars-cloud"
    ];
    push = "tars-cloud";
  };

  packages = with pkgs; [
    git
    curl
    gnutar
    gzip
    zstd
    trivy
    actionlint
    action-validator
    shellcheck
    shfmt
    yamllint
    markdownlint-cli
    jq
    ripgrep
    bun
    nodejs_24
    prek
  ];

  languages = {
    nix = {
      enable = true;
    };
    rust = {
      enable = true;
      toolchainFile = ./rust-toolchain.toml;
    };
    shell = {
      enable = true;
    };
  };

  git-hooks = {
    hooks = {
      cargo-check = {
        enable = true;
        package = config.languages.rust.toolchainPackage;
        args = [ "--locked" ];
      };
      clippy = {
        enable = true;
        package = config.languages.rust.toolchainPackage;
        settings = {
          denyWarnings = true;
          allFeatures = true;
          extraArgs = "--workspace --all-targets --locked";
        };
      };
      rustfmt = {
        enable = true;
        package = config.languages.rust.toolchainPackage;
      };
      actionlint = {
        enable = true;
      };
      action-validator = {
        enable = true;
        files = "(^|/)action\\.ya?ml$|^\\.github/workflows/.*\\.ya?ml$";
      };
      markdownlint = {
        enable = true;
        settings = {
          configuration = {
            MD013 = false;
          };
        };
      };
      nixfmt = {
        enable = true;
      };
      shellcheck = {
        enable = true;
        args = [
          "--external-sources"
          "--source-path=SCRIPTDIR"
        ];
      };
      shfmt = {
        enable = true;
      };
      yamllint = {
        enable = true;
        settings = {
          configuration = ''
            ---
            extends: default
            rules:
              line-length: disable
              truthy:
                allowed-values: ["true", "false", "on"]
              indentation:
                indent-sequences: consistent
              comments:
                min-spaces-from-content: 1
              document-start:
                level: error
          '';
        };
      };
      prettier = {
        enable = true;
        settings = {
          configPath = ".prettierrc.yaml";
        };
      };
    };
  };

  enterTest = ''
    set -euo pipefail
    cargo test --workspace --all-targets --locked
    cargo run --locked --quiet -p tact -- run
    cargo run --locked --quiet -p tact -- check metadata
    prek run --all-files
  '';

  scripts = {
    tact = {
      description = "Run declarative action tests (tact list, validate, or run)";
      exec = ''
        exec cargo run --locked --quiet -p tact -- "$@"
      '';
    };
  };

  tasks = {
    "actions:test" = lib.mkIf config.devenv.isTesting {
      description = "Validate shared action behaviour and metadata";
      before = [ "devenv:enterTest" ];
      exec = config.enterTest;
    };
  };
}
