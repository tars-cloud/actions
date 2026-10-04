# release-rust

Run `candidate`, `prepare` or `publish` inside a consumer's direct devenv or flake environment.
Use the [reusable workflows](../../workflows/consumer-rust-release-candidate/README.md) for the standard release graph.
The [example](example.yaml) shows read-only candidate inspection from a custom job.

The action requires a Linux AMD64 or ARM64 runner, native Rust 1.88 or newer, Cargo, Convco, Git, GitHub CLI and `sha256sum` in the consumer environment.
Declare missing tools in the consumer environment and update its lockfile.
The action compiles its bundled release utility with the consumer Rust toolchain and the action's Cargo.lock.
It builds that utility in runner temporary storage, independently of consumer build output and Cargo configuration.
It never compiles the consumer application or publishes container images.
Consumers with `clean.enabled` must retain the [release environment variables](../../docs/clean-environments.md).

## Inputs

- `command` is required and accepts `candidate`, `prepare` or `publish`.
- `commit-sha` is required and must be the full SHA of the triggering default-branch push or manual run.
- `type` defaults to `devenv`; `flakes` selects a flake-integrated environment.
- `flake-shell` defaults to `.#default`.
- `devenv-installable` optionally pins the native devenv CLI.
- `github-token` defaults to the job token.
  Preparation requires a GitHub App token with Contents and Pull Requests write access.
  Publication requires Contents write and Actions read access.
- `artifact-run-id` defaults to the current Actions run.
- `release-manifest-artifact` defaults to `release-manifest` and must contain `release-manifest.json`.

The consumer checkout must be the repository and Cargo workspace root, match `commit-sha`, include full release history, and have no tracked changes.
Credentials are passed through environment variables and an ephemeral Git credential helper.
The action does not persist Git credentials or configure caches.

## Outputs

- Candidate always returns string booleans `prepare-ready` and `release-ready`.
  At most one can be `true`.
- An approved candidate also returns `version`, `tag`, `commit-sha` and `pr-number`.
- Preparation returns `pr-url`, `pr-number`, `version` and `tag` when it creates or updates a PR.
- Publication returns `release-url`, `version`, `tag` and `commit-sha`.
- `reason` explains a successful no-op.

See the [release lifecycle](../../workflows/consumer-rust-release-candidate/README.md) and [artifact contract](../../workflows/consumer-rust-release-publish/README.md) before composing custom jobs.

## Consumer Profile and Dependency Access

`secretspec-profile` optionally selects a profile before environment evaluation.
Empty retains the consumer configuration.
See [consumer setup](../../docs/consumer-setup.md) for shared checkout and dependency credential handling.
`dependency-token` supplies environment read access separately from the `github-token` publication credential.
