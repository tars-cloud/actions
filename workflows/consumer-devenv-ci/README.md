# Consumer Devenv CI

[Copyable workflow](example.yaml) · [Implementation](../../.github/workflows/consumer-devenv-ci.yaml).

Run lint and tests using the consumer's locked environment with shared checkout, authentication, caching and status reporting.
Defaults use direct devenv, hosted Ubuntu, `prek run --all-files` for lint and the project's configured `devenv --no-tui test` task.
Tests still run after lint fails; either failure makes the aggregate job fail.
Setup and infrastructure failures also fail.

- `lint-command`: Bash source inside the selected shell; empty disables lint.
- `test-command`: Bash source inside the selected shell; empty uses the bootstrap CLI's configured test task in direct mode.
- Flake consumers must supply `test-command`, such as a test script declared by their devShell.
- `runs-on`: JSON label, label array or group/labels object; defaults to `"ubuntu-24.04"`.
- `job-name` and `timeout-minutes`: display name and total job timeout, default `Devenv CI` and 60 minutes.
- `type`, `working-directory`, `flake-shell`, `system` and `devenv-installable`: select the consumer environment.
- `lfs`, `submodules` and `fetch-depth`: checkout options, default false, false and 1.
- `secretspec-profile`: optional profile selection before evaluation and commands.
- `cache-tools` and `cache-exclude`: cache discovery selection and exclusions.
- `cargo-target`, `cargo-build-target`, `cargo-build-variant` and `cargo-environment-key`: optional compiled-cache settings.
- `result`: aggregate setup, lint and test result.

Caller commands are executable Bash source and are passed through environment variables.
Declare all project tools in the selected environment; missing tools do not fall back to runner-installed copies.
Direct configured test mode uses the selected bootstrap devenv CLI outside a nested project shell, so a clean shell need not retain that CLI on its PATH.
The configured test task itself evaluates and executes the project's declared environment.

See [consumer setup and runner selection](../../docs/consumer-setup.md) for optional dependency App authentication, S3 secrets and self-hosted runners.
The workflow requires `contents: read` and `actions: read` for checkout and cache operations.
The caller owns triggers, concurrency and any project-specific application secrets.
