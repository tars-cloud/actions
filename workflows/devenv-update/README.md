# Devenv Update

[Copyable example](example.yaml) · [Workflow](../../.github/workflows/devenv-update.yaml)

Update a project environment lockfile, validate the updated environment, and maintain one dependency pull request.
The caller owns the schedule, manual trigger, concurrency and review policy.
The workflow composes setup-devenv and run-devenv at the caller-selected revision.

The example targets the `v4` release alias.
For production, pin a published release's full commit SHA with a matching version comment.

## Behaviour

- Direct mode runs `devenv --no-tui update` and commits only `devenv.lock`.
- Flake mode runs `nix flake update` and commits only `flake.lock`.
- Both modes update inside the project environment and enter a fresh environment for validation.
- Direct validation defaults to `devenv --no-tui test`.
- Flake validation requires an explicit command and a local devShell selector such as `.#default`.
- Validation failure stops publication and leaves an existing PR unchanged.
- Changes to other tracked files, commits created by validation, and validation changes to the updated lockfile fail the job.
- Untracked generated files are excluded from the commit.
- Repeated runs refresh the same dedicated branch and PR.
- No changes produce no new PR; an obsolete PR can be closed when its changes are already present in the base branch.
- The update branch is cleaned up on a later successful run when it is no longer needed.
- Nothing is merged automatically.

Commit the environment manifests and lockfile before using this workflow.
All project commands run in the declared environment; missing tools must be added there.
The environment must support unattended execution with `CI=true`, `SECRETSPEC_PROVIDER=env` and `SECRETSPEC_REASON=devenv-update`.
Caller workflow-level environment variables are not inherited by reusable workflows.
Configure unfree packages and other project policy in the project's environment definition.
Validation must run without production credentials.

## Authentication

Install a GitHub App on the repository with Contents and Pull Requests write permissions.
Pass its client ID and private key as `client-id` and `app-private-key` secrets.
The workflow mints a repository-scoped token only after validation passes and uses it for signed commits and PR publication.
The ordinary workflow token needs only `contents: read`.
The caller chooses its trusted publication triggers; pull request and merge queue triggers support dry-run mode only.

There is no fallback to `GITHUB_TOKEN` for publication.
App-created PRs can trigger the repository's normal PR checks, whereas PRs created with `GITHUB_TOKEN` do not trigger the usual `push` and `pull_request` workflows.
See the [upstream token documentation](https://github.com/peter-evans/create-pull-request#token).
An optional `dependency-token` secret supplies read access to private GitHub Nix inputs; otherwise dependency access uses `github.token`.
Dependency credentials are separate from the PR write token.

## Inputs

- `job-name`: display name for the update job, default `Update Devenv Dependencies`; set it when calling the workflow for multiple environments or architectures.
- `runs-on`: optional JSON runner label, label array or group/labels object; empty follows the [organisation runner policy](../../docs/setup.md#runner-selection).
- `timeout-minutes`: job timeout; default `60`.
- `type`: `devenv` by default, or `flakes`.
- `working-directory`: environment directory relative to the checkout; default `.`.
- `flake-shell`: local devShell selector; default `.#default`.
- `devenv-installable`: optional pinned native CLI installable for direct mode.
- `validation-command`: Bash commands in a fresh updated environment; empty defaults to `devenv test` in direct mode and fails in flake mode.
- `base-branch`: PR target and checkout branch; empty selects the repository default branch.
- `branch`: dedicated PR branch; default `update-devenv-lock`.
- `title`: PR title and commit message; default `chore: Update Environment Dependencies`.
- `labels`, `reviewers`, `assignees`: optional comma or newline separated values; empty by default.
- `submodules`: recursive checkout when true; default `false`.
- `dry-run`: update and validate the triggering commit without minting an App token or modifying GitHub; default `false`.

Paths must stay within the checkout and contain only letters, digits, spaces, dots, underscores, hyphens and directory separators.
The selected lockfile must be tracked and cannot be a symlink.
Use a dedicated update branch that nobody edits manually.
For multiple environments, call the workflow separately with distinct directories, branches and concurrency groups.
Use the same concurrency group for every caller that writes the same update branch, with `cancel-in-progress: false`.
Labels must already exist in the repository.

For flake integration, add these inputs to the example's update job:

```yaml
---
with:
  type: flakes
  working-directory: .
  flake-shell: .#default
  validation-command: devenv-test
```

Replace `devenv-test` with the validation command exposed by your devShell.
Dry runs require no App secrets and always check out the triggering commit rather than `base-branch`.

## Hosted Disk Cleanup

SDK cleanup is enabled by default on GitHub-hosted Linux runners and runs before Nix/devenv setup, dependency updates and validation.
Each boolean input defaults to `true` and can be set to `false` individually to retain a needed SDK:

- `cleanup-android`: remove the hosted Android SDK.
- `cleanup-dotnet`: remove the hosted .NET SDK.
- `cleanup-haskell`: remove hosted Haskell tools.
- `cleanup-boost`: remove hosted Boost headers.
- `cleanup-swift`: remove the hosted Swift SDK.

Set any category your project needs to `false` to keep it on the runner.
The workflow reuses [free-disk-space](../../composite/free-disk-space/README.md) on Linux with every category explicitly forwarded.
Persistent self-hosted runners always retain their SDKs, even when cleanup inputs are enabled.
Cleanup removes only the action's fixed SDK paths and preserves project files and devenv data.
Cleanup failures stop subsequent setup and project commands and fail the job.

An SDK cleanup failure also prevents update PR publication.

## Outputs

- `pr-url` and `pr-number`: the managed PR when present; empty in dry-run mode.
- `operation`: `created`, `updated`, `closed`, `none` or `dry-run`.
- `changed`: `true` or `false`, describing the validated lockfile relative to the checkout.

The `changed` output does not describe whether an existing PR branch changed.
A successful run can report `changed: true` and `operation: none` when the PR already contains the same update.

## Tests

Tact checks the workflow interface, executes its scripts in isolated fixtures and verifies lockfile commit restrictions with real Git repositories.
CI calls this workflow in dry-run mode on Linux AMD64 and ARM64 for direct and flake environments.
The [publication lifecycle test](../../.github/workflows/test-devenv-update-lifecycle.yaml) uses the repository's CI App on pushes affecting the updater and on manual dispatch.
It creates a disposable repository fixture with a stale lockfile, checks that validation observes the updated lockfile, verifies one signed PR across repeated runs, observes downstream PR checks, and tests closure after the base already contains the update.
Cleanup closes remaining test PRs and removes only branches named for that run and attempt.
The test uses `CI_APP_CLIENT_ID` and `CI_APP_PRIVATE_KEY`, which must be available as repository or organisation secrets.

## Common Project Setup

See [shared setup and runner selection](../../docs/setup.md) for LFS checkout, SecretSpec profiles and optional read-only dependency App authentication.

Runner defaults follow the [organisation runner policy](../../docs/setup.md#runner-selection).
`runner-architecture` accepts `AMD64` (default) or `ARM64`; ARM64 selects `aarch64-linux` unless `system` explicitly overrides it.
Explicit `runs-on` selectors take precedence over organisation defaults.

## Debug Logging

Pass `debug: true` under the calling job's `with:` to enable verbose tool logs and phase diagnostics.
GitHub's **Re-run with debug logging** enables the same behaviour automatically.
See [the debug contract](../../README.md#debug-logging) for details.
