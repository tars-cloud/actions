# Rust Release Preparation

Prepare the next Cargo version and CHANGELOG.md with Convco, then create or refresh one `release/next` PR.
Use it when [candidate](../release-rust-candidate/README.md) returns `prepare-ready=true`.
The [example](example.yaml) shows the full lifecycle.

## Configuration

The common inputs are `commit-sha`, `runs-on`, `type`, `flake-shell` and `devenv-installable`.
They have the same meaning as in [candidate](../release-rust-candidate/README.md).

Provide these secrets:

- `app-id`: the GitHub App client ID or App ID.
- `app-private-key`: its private key.

Install the App in the consumer repository with Contents and Pull Requests write permissions.
Preparation mints a repository-scoped token so managed PR changes can run the consumer's normal CI.
The job token only needs Contents, Pull Requests and Actions read permissions.

## Version Policy

Root Cargo.toml is the sole version authority.
Convco calculates the next version from Conventional Commits since the preceding version tag.
The consumer may configure Convco through one `.convco` or `.versionrc` file.
The shared workflow uses SemVer and the `v` tag prefix, independent of ambient `CONVCO_*` variables.
Without a previous version tag, the first release uses the initial version declared in Cargo.toml.

With Convco's defaults, features bump the minor version and fixes bump the patch version.
Chores without a release increment do not create a PR once a release baseline exists.
While a release PR remains open, new merges refresh that PR from the latest eligible base.
Several fixes still produce one patch bump from the previous release.
The proposed version does not increment every time the PR is refreshed.

Preparation updates the root version, internal path dependency requirements and workspace package entries in Cargo.lock.
It checks the resulting lockfile with Cargo's locked metadata command.
It generates CHANGELOG.md with Convco and permits repeated changelog section headings through a file-local markdownlint directive.
The PR contains only derived version manifests, Cargo.lock and CHANGELOG.md.

The release branch is replaced using an explicit force-with-lease check.
An existing unmanaged PR on `release/next` is rejected rather than overwritten.
Default-branch advancement during preparation leaves the newer run responsible for refreshing the PR.
A merged release awaiting publication blocks preparation of another version.

Outputs are `pr-url`, `pr-number`, `version`, `tag` and, for a no-op, `reason`.
Publication starts only when the human merges this PR and its required consumer build jobs succeed.
