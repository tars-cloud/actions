{
  config,
  lib,
  pkgs,
  ...
}:
let
  actionValidator = import ./nix/packages/action-validator.nix { inherit pkgs; };
  convco = import ./nix/packages/convco.nix {
    inherit pkgs;
    rustPlatform = pkgs.makeRustPlatform {
      cargo = config.languages.rust.toolchainPackage;
      rustc = config.languages.rust.toolchainPackage;
    };
  };
  cargoCrap = import ./nix/packages/cargo-crap.nix {
    inherit pkgs;
    rustPlatform = pkgs.makeRustPlatform {
      cargo = config.languages.rust.toolchainPackage;
      rustc = config.languages.rust.toolchainPackage;
    };
  };
in
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
    actionValidator
    actionlint
    bun
    cargo-audit
    cargo-edit
    cargoCrap
    cargo-llvm-cov
    cargo-tarpaulin
    curl
    gh
    git
    gnutar
    gzip
    jq
    markdownlint-cli
    nodejs_24
    prek
    ripgrep
    shellcheck
    shfmt
    trivy
    yamllint
    zstd
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
    excludes = [
      "CHANGELOG.md"
    ];
    hooks = {
      convco = {
        enable = true;
        package = convco;
      };
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
        files = "^\\.github/workflows/.*\\.ya?ml$|^(composite|workflows)/.*/example\\.yaml$";
        # Tact checks self-repository references and release queue policy unsupported by actionlint 1.7.12.
        args = [
          "-ignore"
          ''^specifying action "\$/composite/[a-z-]+(/scripts/[a-z-]+)?" in invalid format because ref is missing\.''
          "-ignore"
          ''^unexpected key "queue" for "concurrency" section\. expected one of "cancel-in-progress", "group"$''
        ];
      };
      action-validator = {
        enable = true;
        package = actionValidator;
        files = "(^|/)action\\.ya?ml$|^\\.github/workflows/.*\\.ya?ml$|^(composite|workflows)/.*/example\\.yaml$";
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
              brackets:
                forbid: non-empty
              truthy:
                allowed-values:
                  - "true"
                  - "false"
                  - "on"
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
    cargo run --locked --quiet -p tact -- check workflows
    prek run --all-files
  '';

  scripts = {
    crap = {
      description = "Measure Rust Workspace Coverage and CRAP Scores";
      exec = ''
        set -euo pipefail
        report_directory="$DEVENV_ROOT/.tars/scratch/cargo-crap"
        mkdir -p "$report_directory"
        export CARGO_TARGET_DIR="$report_directory/target"
        cargo llvm-cov --workspace --locked --lcov --output-path "$report_directory/lcov.info"
        cargo crap --workspace --lcov "$report_directory/lcov.info"
      '';
    };
    actions-release = {
      description = "Prepare a release PR or publish a tested release";
      exec = ''
        exec cargo run --locked --quiet -p actions-release -- "$@"
      '';
    };
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
