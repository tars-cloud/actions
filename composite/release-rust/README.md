# release-rust

Run `candidate`, `prepare` or `publish` inside a project's direct devenv or flake environment.
Use the [reusable workflows](../../workflows/rust-release-candidate/README.md) for the standard release graph.
The [example](example.yaml) shows read-only candidate inspection from a custom job.

The action requires a Linux AMD64 or ARM64 runner, Rust 1.88 or newer, Cargo, Convco, Git, GitHub CLI and `sha256sum` in the project environment.
Declare missing tools in the project environment and update its lockfile.
The action compiles its bundled release utility with the project Rust toolchain and the action's Cargo.lock.
It builds that utility in runner temporary storage, independently of project build output and Cargo configuration.
It never compiles the application or publishes container images.
Projects with `clean.enabled` must retain the [release environment variables](../../docs/clean-environments.md).

## Inputs

- `command` is required and accepts `candidate`, `prepare` or `publish`.
- `commit-sha` is required and must be the full SHA of the triggering default-branch push or manual run.
- `type` defaults to `devenv`; `flakes` selects a flake-integrated environment.
- `system` optionally selects `x86_64-linux` or `aarch64-linux`; foreign execution requires preconfigured emulation.
- `flake-shell` defaults to `.#default`.
- `devenv-installable` optionally pins the native devenv CLI.
- `github-token` defaults to the job token.
  Preparation requires a GitHub App token with Contents and Pull Requests write access.
  Publication requires Contents write and Actions read access.
- `artifact-run-id` defaults to the current Actions run.
- `release-manifest-artifact` defaults to `release-manifest` and must contain `release-manifest.json`.

The checkout must be the repository and Cargo workspace root, match `commit-sha`, include full release history, and have no tracked changes.
Credentials are passed through environment variables and an ephemeral Git credential helper.
The action does not persist Git credentials or configure caches.

## Outputs

- Candidate always returns string booleans `prepare-ready` and `release-ready`.
  At most one can be `true`.
- An approved candidate also returns `version`, `tag`, `commit-sha` and `pr-number`.
- Preparation returns `pr-url`, `pr-number`, `version` and `tag` when it creates or updates a PR.
- Publication returns `release-url`, `version`, `tag` and `commit-sha`.
- `reason` explains a successful no-op.
- `decision` identifies the candidate state without parsing `reason`.
- Pending decisions include `pending-commit-sha`, `pending-version` and `pending-run-url` when the original push run is available.
- `recovery` is `true` when an approved source is selected for publication recovery.

See the [release lifecycle](../../workflows/rust-release-candidate/README.md) and [artifact contract](../../workflows/rust-release-publish/README.md) before composing custom jobs.

## Project Profile and Dependency Access

`secretspec-profile` optionally selects a profile before environment evaluation.
Empty retains the project configuration.
See [setup guide](../../docs/setup.md) for shared checkout and dependency credential handling.
`dependency-token` supplies environment read access separately from the `github-token` publication credential.

## Approved release recovery

`recovery: true` is supported for candidate and publish operations on a manual default-branch dispatch.
It separates the triggering workflow commit from an older approved release source without bypassing source review or immutable tags.
Recovery accepts only the pending merged managed release and artifacts from the current manual run.
See [the candidate workflow contract](../../workflows/rust-release-candidate/README.md#manual-recovery-with-corrected-orchestration).

For application source corrections, candidate and prepare accept `replace-unpublished` with the pending managed source SHA on a manual default-branch dispatch.
This prepares a fresh release PR with Convco's recalculated version and requires a new approval before publication.
Tagged versions, published releases, prereleases and drafts targeting unrelated commits cannot be replaced.
See [unpublished source replacement](../../workflows/rust-release-candidate/README.md#replacing-an-unpublished-source).

`pending-release-policy` defaults to `refresh-unpublished` on both candidate and prepare.
When a merged release has not been tagged or published and its original push workflow is no longer queued or running, newer default-branch pushes automatically recalculate the version with Convco and refresh a replacement PR on `release/next`.
If the passing commit's manifests, lockfile and changelog already match Convco's exact output for the already-approved unpublished version, candidate selects that commit for direct publication without opening a duplicate PR.
Preparation checks drafts using its write credential before changing the release branch; orphaned untagged drafts targeting the superseded pending commit do not block replacement and are cleaned up during publication.
Set `pending-release-policy: wait` on both candidate and prepare to block newer default-branch pushes until the pending release finishes or is manually replaced.
Publication still requires a fresh human merge whenever a replacement PR is prepared.

For recovery without a source SHA, set `recover-pending: true` on candidate, use the triggering `${{ github.sha }}` as `commit-sha`, and dispatch from the default branch.
Candidate selects and verifies the pending approved source, returning its `commit-sha` and `recovery: true`.
Build that source and pass those outputs to publish after every required gate succeeds.
Do not combine `recover-pending` with `recovery` or `replace-unpublished` on candidate.
When no release is pending, candidate succeeds with both ready outputs false.

## Debug Logging

Set `debug: true` to enable verbose tool logs and phase diagnostics.
GitHub's **Re-run with debug logging** enables the same behaviour automatically.
The default is `false`; GitHub debug logging still takes precedence.
See [the debug contract](../../README.md#debug-logging) for details.
