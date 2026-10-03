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
- [cargo-crap](composite/cargo-crap/README.md): measure Rust coverage and gate function score regressions.
- [release-rust](composite/release-rust/README.md): prepare, inspect and publish Cargo/Convco releases.
- [free-disk-space](composite/free-disk-space/README.md): explicit hosted SDK cleanup, always skipped on self-hosted
  runners.
- [report-status](composite/report-status/README.md): Bash-only pipeline summaries and optional commit statuses using
  gh.

Each action is independently callable.

Each action owns its runtime scripts and private helper actions under its own `scripts/` directory.
Its `action.yaml`, `README.md`, `example.yaml` and declarative `test.yaml` live beside that directory.

setup-cache and setup-devenv compose setup-nix as a shared prerequisite.

Nested `$/` references follow the exact action revision selected by the caller, using [GitHub's self-repository syntax](https://github.blog/changelog/2026-07-30-reference-same-repository-actions-with-self-repository-syntax/).

GitHub Enterprise Server and Windows are outside the supported scope.
The environment composites target Linux; the standalone CodeQL workflow also supports macOS for Swift analysis.
Linux X64 and ARM64 execution is supported natively or with preconfigured runner emulation.
Pass the same `system` to setup-devenv, run-devenv, setup-trivy and setup-cache when selecting a foreign system.
The actions validate execution support but do not install emulation.

## Reusable workflows

- [Devenv update](workflows/consumer-devenv-update/README.md): validate lockfile updates and maintain a dependency PR using a GitHub App.
- [Trivy](workflows/consumer-trivy/README.md): scan using the consumer's devenv Trivy package and tool cache.
- [CodeQL](workflows/consumer-codeql/README.md): analyze selected languages using consumer devenv/flake toolchains or existing runner toolchains.
- [Cargo CRAP](workflows/consumer-cargo-crap/README.md): compare PRs with the baseline branch and maintain one optional baseline/badge PR.
- [Rust release candidate](workflows/consumer-rust-release-candidate/README.md): identify an ordinary merge or an approved release commit.
- [Rust release preparation](workflows/consumer-rust-release-prepare/README.md): maintain one version and changelog PR with Convco.
- [Rust release publication](workflows/consumer-rust-release-publish/README.md): attach consumer-built artifacts and publish the approved version.

A consumer can call both from one `secops.yaml`; the [combined example](workflows/consumer-trivy/example.yaml) shows the interface.
Reusable workflows live under `.github/workflows/` and compose this repository's actions with same-revision `$/` references.
The workflow implementations, action implementations and upstream action pins are released together.
Consumers keep scan configuration and triggers in their own repositories.

Dependabot's `github-actions` ecosystem updates SHA-pinned reusable workflow calls.
Use a published release SHA with a matching same-line version comment, and group related action updates into one PR per repository.
Central updates reach SHA-pinned consumers after their update PRs merge; they do not change existing pins automatically.
See [GitHub's Dependabot guidance](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/auto-update-actions).

## Testing

[Testing with Tact](docs/tact.md) covers the Rust runner, per-action `test.yaml` scenarios and integration checks.

The [repository Cargo CRAP workflow](.github/workflows/repository-cargo-crap.yaml) uses the shared workflow to compare PRs with `trunk`.
[.cargo-crap.toml](.cargo-crap.toml) defines workspace scoring with threshold 30, regression tolerance 0.01 and the standard weight of 1 for `?`.
Tests, benchmarks and examples are excluded from scoring; coverage still runs the workspace tests.
Completed reports appear in the run summary and artifacts, with a sticky comment on same-repository PRs.
Trunk pushes refresh one `crap/next` PR containing `.github/crap/baseline.json` and `.github/badges/crap-badge.json`, using the existing CI App secrets described in [release setup](docs/releases.md).
Merge that recording PR to publish the JSON; accepting a quality failure through merge makes the updated trunk source authoritative for future comparisons.
The permissive Cargo CRAP integration tests remain separate from this repository quality gate.

For an on-demand local report, run from the repository root:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=local-cargo-crap \
  devenv --no-tui shell --quiet -- crap
```

Coverage output and instrumented builds stay under the ignored `.tars/scratch/cargo-crap/` directory.
Coverage does not run in commit hooks.

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
