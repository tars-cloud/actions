# TARS Cloud shared actions

## Overview

An opinionated set of composite actions bundled for re-use.

Refer to the README.md within each composite actions for example usage.

## Composite Actions

- [setup-nix](setup-nix/README.md): idempotent Nix prerequisite.
- [setup-cache](setup-cache/README.md): detected dependency-download archives and optional Cachix.
- [setup-devenv](setup-devenv/README.md): bootstrap and warm the selected project shell.
- [setup-trivy](setup-trivy/README.md): validate the environment's Trivy package and report its version.
- [free-disk-space](free-disk-space/README.md): explicit hosted SDK cleanup, always skipped on self-hosted runners.

Each action is independently callable.

Only setup-cache and setup-devenv compose setup-nix as a shared prerequisite.

Nested `$/` references follow the exact action revision selected by the caller, using
[GitHub's self-repository syntax](https://github.blog/changelog/2026-07-30-reference-same-repository-actions-with-self-repository-syntax/).

GitHub Enterprise Server, Windows, macOS and emulated ARM64 are outside of scope for this MVP.

## Testing

[Testing with Tact](docs/tact.md) covers the Rust runner, per-action `test.yaml` scenarios and integration checks.
