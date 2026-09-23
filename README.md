# TARS Cloud shared actions

## Overview

An opinionated set of composite actions bundled for re-use.

Refer to the README.md within each composite actions for example usage.

## Composite Actions

- [setup-nix](composite/setup-nix/README.md): idempotent Nix prerequisite.
- [setup-cache](composite/setup-cache/README.md): detected dependency-download archives and optional Cachix.
- [setup-devenv](composite/setup-devenv/README.md): bootstrap and warm the selected project shell.
- [setup-trivy](composite/setup-trivy/README.md): validate the environment's Trivy package and report its version.
- [free-disk-space](composite/free-disk-space/README.md): explicit hosted SDK cleanup, always skipped on self-hosted
  runners.
- [report-status](composite/report-status/README.md): Bash-only pipeline summaries and optional commit statuses using
  gh.

Each action is independently callable.

Each action owns its runtime scripts and private helper actions under its own `scripts/` directory. Its `action.yml`,
`README.md` and declarative `test.yaml` live beside that directory.

Only setup-cache and setup-devenv compose setup-nix as a shared prerequisite.

Nested `$/` references follow the exact action revision selected by the caller, using
[GitHub's self-repository syntax](https://github.blog/changelog/2026-07-30-reference-same-repository-actions-with-self-repository-syntax/).

GitHub Enterprise Server, Windows, macOS and emulated ARM64 are outside of scope for this MVP.

## Testing

[Testing with Tact](docs/tact.md) covers the Rust runner, per-action `test.yaml` scenarios and integration checks.

## Releases

[Releasing the repository](docs/releases.md) covers the manual Prepare release and Publish release workflows, shared
Cargo version, Conventional Commits and dependency batching.

## Repository layout

- `composite/<action>/`: public actions with their metadata, documentation, declarative tests and owned runtime scripts.
- `crates/`: Tact and release automation.
- `schemas/`: handwritten schemas shared by the declarative test manifests.
- `nix/packages/`: repository tooling package definitions.
- `tests/fixtures/`: shared integration fixtures.
- `docs/`: contributor and release documentation.
- `.github/`: repository workflows and Dependabot configuration.

Consumers reference `tars-cloud/actions/composite/<action>@<reviewed-sha>`. Dependabot updates the flake inputs in
`tests/fixtures/flakes/flake.lock` weekly.
