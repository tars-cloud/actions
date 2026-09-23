# Validation evidence

The test suite has migrated to Tact. Local migration checks passed on Linux X64 on 2026-09-22:

- All seven action manifests passed schema validation and their 46 scenarios.
- All 19 Rust tests passed, including CLI failure paths, mock forwarding, fixture isolation and CI cache evidence.
- The Nix package built successfully and ran the Rust suite offline during its check phase.
- Real direct, default-flake and named-flake environments accepted their declared Trivy package.
- Real direct and flake environments without Trivy rejected an ambient executable.
- Pinned RunsOn restore/save implementations contacted a disposable local S3 endpoint and handled HTTP 403 denials
  nonfatally; save emitted a warning.
- Pinned Cachix main/post code passed mocked read, write/daemon-drain and fork/no-token lifecycle checks.
- Live lab S3 operation and live Cachix reads/writes remain unverified.

[Testing with Tact](tact.md) documents commands and the manifest contract. The old shell, Node and Python test runners
have been removed. Production action implementations remain the subjects of the tests.

## Release automation

The release implementation adds 13 Rust tests, bringing the workspace total to 32. Local checks on 2026-09-23 passed
`devenv test`, all file hooks, and the Nix package's offline build and test phase. Explicit `commit-msg` checks accepted
a conventional message and rejected a nonconventional message.

Release fixtures verify Convco version calculations, dependency batching, generated Cargo and changelog files,
squash-merge ancestry, stale-candidate rejection, exact-commit CI gates and immutable-tag retries. They use disposable
Git repositories and do not publish GitHub releases. [Release operations](releases.md) describes the manual dispatches
and repository requirements.

## Coverage migration

- Shell fixture tests became per-action YAML scenarios for Nix reuse, bootstrap, shell dispatch, Trivy isolation and
  safe SDK cleanup.
- Cache policy and key tests became shared Rust checks selected by `setup-cache/scripts/cache-plan/test.yaml`.
- Metadata checks became `tact check metadata`, including pins, nested input wiring, post-output safety and native CI
  matrices.
- Static checks use the hooks configured in `devenv.nix`.
- S3 and Cachix integration checks became `tact integration s3` and `tact integration cachix`.
- Real environment checks became `tact integration environments`.
- Cold/warm CI fixture scripts became `tact ci` subcommands.

## Hosted validation

[CI run 35704182195](https://github.com/tars-cloud/actions/actions/runs/35704182195) passed all eleven jobs before the
Tact migration. That run covered native AMD64/ARM64 direct and default/named flake environments, cold/post-save/warm
cache lifecycles, and the enterprise self-hosted runner. The migrated workflow retains those lifecycle checks and
invokes Tact for test execution and fixture management. Local migration checks alone do not establish a passing migrated
hosted run.

## Reviewed upstream revisions

- actions/cache v6.1.0: `55cc8345863c7cc4c66a329aec7e433d2d1c52a9`; Node 24 and success-only post-save.
- runs-on/cache v5.0.7: `88d90644011a3a9957fd141a106f5a94f9794203`; explicit S3 routing, success-only post-save and
  warning-only save failures.
- cachix/install-nix-action v31.11.1: `13d8dd58da0234aa297dedd986986ccb8e7f3e24`; used after hosted missing-Nix
  detection.
- cachix/cachix-action v17: `38b082610b782e7e93e209c35fd730d399dee866`; substituter setup and post-job push integration.
- actions/checkout v7.0.1: `3d3c42e5aac5ba805825da76410c181273ba90b1`; checkout with credential persistence disabled.
