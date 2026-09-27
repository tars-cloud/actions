# TARS Cloud shared actions

## Overview

An opinionated set of composite actions bundled for re-use.

Each action includes a complete, copyable `example.yaml` workflow and a README describing its inputs and outputs.
See the [migration guide](docs/migration.md) for pinned CLI and execution-system changes.

## Composite Actions

- [setup-nix](composite/setup-nix/README.md): idempotent Nix prerequisite.
- [setup-cache](composite/setup-cache/README.md): detected language and tool dependency-download archives.
- [setup-devenv](composite/setup-devenv/README.md): bootstrap and warm the selected project shell.
- [run-devenv](composite/run-devenv/README.md): execute commands in the selected project shell.
- [setup-trivy](composite/setup-trivy/README.md): validate the environment's Trivy package and report its version.
- [free-disk-space](composite/free-disk-space/README.md): explicit hosted SDK cleanup, always skipped on self-hosted
  runners.
- [report-status](composite/report-status/README.md): Bash-only pipeline summaries and optional commit statuses using
  gh.

Each action is independently callable.

Each action owns its runtime scripts and private helper actions under its own `scripts/` directory.
Its `action.yml`, `README.md`, `example.yaml` and declarative `test.yaml` live beside that directory.

setup-cache and setup-devenv compose setup-nix as a shared prerequisite.

Nested `$/` references follow the exact action revision selected by the caller, using [GitHub's self-repository syntax](https://github.blog/changelog/2026-07-30-reference-same-repository-actions-with-self-repository-syntax/).

GitHub Enterprise Server and Windows are outside the supported scope.
The environment composites target Linux; the standalone CodeQL workflow also supports macOS for Swift analysis.
Linux X64 and ARM64 execution is supported natively or with preconfigured runner emulation.
Pass the same `system` to setup-devenv, run-devenv, setup-trivy and setup-cache when selecting a foreign system.
The actions validate execution support but do not install emulation.

## Reusable workflows

- [Trivy](workflows/trivy/README.md): scan using the consumer's devenv Trivy package and tool cache.
- [CodeQL](workflows/codeql/README.md): analyze selected languages using consumer devenv/flake toolchains or existing runner toolchains.

A consumer can call both from one `secops.yml`; the [combined example](workflows/trivy/example.yaml) shows the interface.
Reusable workflows live under `.github/workflows/` and compose this repository's actions with same-revision `$/` references.
The workflow implementations, action implementations and upstream action pins are released together.
Consumers keep scan configuration and triggers in their own repositories.

Dependabot's `github-actions` ecosystem updates SHA-pinned reusable workflow calls.
Use a published release SHA with a matching same-line version comment, and group related action updates into one PR per repository.
Central updates reach SHA-pinned consumers after their update PRs merge; they do not change existing pins automatically.
See [GitHub's Dependabot guidance](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/auto-update-actions).

## Testing

[Testing with Tact](docs/tact.md) covers the Rust runner, per-action `test.yaml` scenarios and integration checks.

## Releases

[Releasing the repository](docs/releases.md) covers manual release preparation, publication after successful trunk CI, the shared Cargo version, Conventional Commits and dependency batching.

## Repository layout

- `composite/<action>/`: public actions with their metadata, documentation, declarative tests and owned runtime scripts.
- `crates/`: Tact and release automation.
- `schemas/`: handwritten schemas shared by the declarative test manifests.
- `nix/packages/`: repository tooling package definitions.
- `tests/fixtures/`: shared integration fixtures.
- `docs/`: contributor and release documentation.
- `.github/`: repository workflows and Dependabot configuration.

Consumers reference `tars-cloud/actions/composite/<action>@<reviewed-sha>`.
Dependabot updates the flake inputs in `tests/fixtures/flakes/flake.lock` weekly.
