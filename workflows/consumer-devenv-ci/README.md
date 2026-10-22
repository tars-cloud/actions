# Consumer Devenv CI

[Copyable workflow](example.yaml) · [Implementation](../../.github/workflows/consumer-devenv-ci.yaml).

Run lint and tests using the consumer's locked environment with shared checkout, authentication, caching and status reporting.
Defaults use direct devenv, hosted Ubuntu, `prek run --all-files` for lint and the project's configured `devenv --no-tui test` task.
Tests still run after lint fails; either failure makes the aggregate job fail.
Setup and infrastructure failures also fail.

- `lint-command`: Bash source inside the selected shell; empty disables lint.
- `test-command`: Bash source inside the selected shell; empty uses the bootstrap CLI's configured test task in direct mode.
- Flake consumers must supply `test-command`, such as a test script declared by their devShell.
- `runs-on`: optional JSON runner label, label array or group/labels object; empty follows the [organisation runner policy](../../docs/consumer-setup.md#runner-selection).
- `job-name` and `timeout-minutes`: display name and total job timeout, default `Devenv CI` and 60 minutes.
- `type`, `working-directory`, `flake-shell`, `system` and `devenv-installable`: select the consumer environment.
- `lfs`, `submodules` and `fetch-depth`: checkout options, default false, false and 1.
- `secretspec-profile`: optional profile selection before evaluation and commands.
- `cache-tools` and `cache-exclude`: cache discovery selection and exclusions.
- `cargo-target`, `cargo-build-target`, `cargo-build-variant` and `cargo-environment-key`: optional compiled-cache settings.
- `result`: aggregate SDK cleanup, setup, lint and test result.

Caller commands are executable Bash source and are passed through environment variables.
Declare all project tools in the selected environment; missing tools do not fall back to runner-installed copies.
Direct configured test mode uses the selected bootstrap devenv CLI outside a nested project shell, so a clean shell need not retain that CLI on its PATH.
The configured test task itself evaluates and executes the project's declared environment.
The workflow sets `PREK_HOME` under the runner's temporary directory so lint also works with a read-only runner home.
Keep `PREK_HOME` in `clean.keep` when enabling a clean direct shell.

See [consumer setup and runner selection](../../docs/consumer-setup.md) for optional dependency App authentication, S3 secrets and self-hosted runners.
The workflow requires `contents: read` and `actions: read` for checkout and cache operations.
The caller owns triggers, concurrency and any project-specific application secrets.

## Hosted Disk Cleanup

SDK cleanup is opt-in and runs before caches, Nix/devenv setup and consumer commands.
Each boolean input defaults to `false`:

- `cleanup-android`: remove the hosted Android SDK.
- `cleanup-dotnet`: remove the hosted .NET SDK.
- `cleanup-haskell`: remove hosted Haskell tools.
- `cleanup-boost`: remove hosted Boost headers.
- `cleanup-swift`: remove the hosted Swift SDK.

Enable the categories your consumer environment does not need to free space before setup and Rust compilation.
The workflow reuses [free-disk-space](../../composite/free-disk-space/README.md) on Linux with every category explicitly forwarded.
Persistent self-hosted runners always retain their SDKs, even when cleanup inputs are enabled.
Cleanup removes only the action's fixed SDK paths and preserves consumer files and devenv data.
Cleanup failures stop subsequent setup and consumer commands and fail the job.

SDK cleanup failures also appear in the aggregate `result` output.

Runner defaults follow the [organisation runner policy](../../docs/consumer-setup.md#runner-selection).
`runner-architecture` accepts `AMD64` (default) or `ARM64`; ARM64 selects `aarch64-linux` unless `system` explicitly overrides it.
Explicit `runs-on` selectors take precedence over organisation defaults.

## Debug Logging

Pass `debug: true` under the calling job's `with:` to enable verbose tool logs and phase diagnostics.
GitHub's **Re-run with debug logging** enables the same behaviour automatically.
See [the debug contract](../../README.md#debug-logging) for details.
