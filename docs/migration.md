# Migrating to the next release

The examples target the upcoming `v3` alias.
After publication, pin all shared action calls to the same reviewed full commit SHA.
Before publication, replace `@v3` with the revision you are testing.
No release or consumer migration happens automatically.

## Update reusable workflow paths

Release `v3` uses `.yaml` workflow filenames and purpose prefixes.
Update consumer `uses` references before pinning the new release:

- `codeql.yml` becomes `consumer-codeql.yaml`.
- `devenv-update.yml` becomes `consumer-devenv-update.yaml`.
- `trivy.yml` becomes `consumer-trivy.yaml`.
- `release-rust-candidate.yml` becomes `consumer-rust-release-candidate.yaml`.
- `release-rust-prepare.yml` becomes `consumer-rust-release-prepare.yaml`.
- `release-rust-publish.yml` becomes `consumer-rust-release-publish.yaml`.

The composite action directory paths stay the same.
Action metadata is now named `action.yaml`.

## Replace local shell wrappers

Use `setup-devenv` to prepare the environment, then `run-devenv` for consumer commands.
Ordinary workflow `run` steps do not automatically enter the prepared shell.
Pass matching `type`, `working-directory`, `flake-shell` and `system` inputs to each action that enters the environment.
Pass the same environment selection and system to `setup-cache` for compatible compiled-cache keys.

The [setup-devenv workflow](../composite/setup-devenv/example.yaml) covers native X64 and ARM64, a pinned CLI, setup outputs, and optional preconfigured emulation.
An explicit `devenv-installable` overrides any installed CLI and applies only to direct mode.
Foreign-system execution requires existing runner emulation and Nix `extra-platforms`; the action validates both configuration and shell execution.
The [run-devenv workflow](../composite/run-devenv/example.yaml) covers a flake environment.
Replace its sample command with your project's command.

For private dependency inputs, pass the same `github-token` to setup-devenv, run-devenv and setup-trivy.
The [Trivy workflow](../composite/setup-trivy/example.yaml) demonstrates this and requires Trivy declared in the selected environment.

## Inspect cache restoration

The [cache workflow](../composite/setup-cache/example.yaml) covers GitHub-hosted runners and an optional self-hosted S3 job.
Supply all required S3 inputs on self-hosted runners; absent configuration uses GitHub storage, while partial configuration fails clearly.
Use the per-tool `*-status` outputs to distinguish exact hits, compatible fallbacks, reported errors and skipped restores.
`miss-or-unavailable` preserves the upstream ambiguity between a cold miss and some recoverable transport failures.
Normal misses are notices; reported failures are warnings.
The log includes the requested key, paths and save policy.
Confirm actual uploads in the backend post-job save log after a successful job that populated the cache paths.

## Use Devenv Toolchains for CodeQL

Set `type: devenv` on the shared CodeQL workflow call, or `type: flakes` with the consumer's `flake-shell` and `working-directory`.
Existing calls default to runner toolchains, so pinning the new release alone does not change their environment.
For Rust analysis, declare the Rust language module and `pkgs.rustup` in the environment, even when using build mode `none`.
The shared workflow exposes those toolchains to CodeQL extraction as well as running setup/manual-build commands in the selected shell.
See the [CodeQL example](../workflows/consumer-codeql/example.yaml) and [environment contract](../workflows/consumer-codeql/README.md#devenv-and-flake-toolchains).

## Keep examples valid

Every action has an `example.yaml`, including private helpers whose examples use their public parent action.
CI validates workflow syntax, action pins, input names, required inputs and referenced shared-action outputs.
Optional self-hosted examples require a repository variable and a manual dispatch; adjust runner labels for your installation.
