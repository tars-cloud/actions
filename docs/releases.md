# Releases

The repository, its composite actions, Tact and the release utility share `[workspace.package].version` in Cargo.toml.
Prepare release writes the exact version returned by `convco version --bump`. Publish release reads that version from
the reviewed, merged commit and publishes `v<version>`. The Nix package reads the same Cargo.toml version.

## Prepare a release

1. Merge the feature, fix and dependency PRs to include.
2. Run **Prepare release** from **trunk** in GitHub Actions.
3. Review the single `release/next` PR, containing Cargo.toml, Cargo.lock and CHANGELOG.md changes.
4. Wait for CI and merge the PR using its generated `chore(release): v<version>` title.

Normal PRs do not bump Cargo.toml or maintain the changelog. Running Prepare release again rebuilds the same release
branch from trunk and refreshes its PR. If trunk advances before the release PR merges, rerun Prepare release before
merging. If an outdated release PR was already merged, prepare and merge a fresh release PR before publishing. Do not
use GitHub's **Update branch** button on a release PR; regeneration keeps its version and changelog consistent.

Prepare release uses the organization's CI GitHub App, following the platform repository's credential names. Make
`CI_APP_CLIENT_ID` (or the fallback `CI_APP_ID`) and `CI_APP_PRIVATE_KEY` available as Actions secrets to this
repository. Organization secrets restricted to private repositories are unavailable here because this repository is
public. The App must be installed for this repository with Contents and Pull requests write permissions. The workflow
mints a token scoped to this repository after environment setup and revokes it at job completion. App-created PRs and
branch updates trigger the normal CI workflows without an additional dispatch. The organization setting allowing
`GITHUB_TOKEN` to create PRs is not required. Publish release uses its job's `GITHUB_TOKEN` with Contents write, Pull
requests read and Actions read permissions. Both release workflows run in the `enterprise/tars-cloud` runner group and
share one concurrency group.

## Publish a release

Review the release diff, upstream immutable pins, action interfaces and compatibility notes before publication. The
required CI matrix covers native AMD64 and ARM64, direct and flake environments, and cold/post-save/warm caches. Record
any live S3 or Cachix validation separately; fixture tests do not establish live service operation.

1. Wait for the **CI** push run on the merged release commit in trunk to succeed.
2. Copy the full 40-character merged commit SHA, not the release branch SHA.
3. Run **Publish release** from **trunk**, supplying that SHA as `commit`.

Publication is manual even after the release PR has been approved and merged. The workflow verifies the same-repository
merged release PR, its files and ancestry, the calculated version, the Cargo.lock workspace versions, and successful CI
for the exact supplied trunk commit. Later trunk commits are excluded from the release; their presence does not
invalidate an already correctly merged candidate. Changes merged into trunk between preparation and the release merge do
invalidate the candidate.

The workflow creates a fixed `v<version>` tag and GitHub Release, then moves the major alias such as `v1` to that
commit. The initial `0.x` series uses `v0`. Full version tags are never moved, and older releases cannot move an alias
backwards. The release body contains up to five changelog highlights and a link to CHANGELOG.md at the fixed version
tag. It does not copy the entire changelog.

If publication stops after creating a tag or release, rerun it with the same commit SHA. Existing tags must point to
that exact commit; conflicting tags cause a failure. Published release notes are preserved on retries. This process does
not publish crates or attach compiled binaries.

## Conventional commits and version policy

Consumers using a major tag such as `tars-cloud/actions/composite/setup-cache@v1` receive compatible updates as the
alias moves. Consumers using a reviewed full commit SHA remain pinned until a reviewed update, for example from
Dependabot. All public actions share this policy; there are no action-specific versions.

Convco is pinned in `nix/packages/convco.nix` and configured in `.convco`. The newer pin supports
`treatMajorZeroAsStable`, so the same rules apply before and after `1.0.0`:

- `feat(scope): ...` requests a minor release.
- `fix(scope): ...` and `security(scope): ...` request a patch release.
- `build(deps): ...` requests a patch release and appears under Dependencies.
- A breaking `!` or `BREAKING CHANGE:` footer requests a major release.
- Documentation, chores and the other configured non-release types do not request a bump.

The largest requested increment since the previous release wins. Several Dependabot merges therefore produce one patch
release when Prepare release is run. Dependabot uses the `build` prefix and dependency scope for both Cargo and GitHub
Actions updates. Review dependency upgrades for consumer-facing breaking changes and mark the PR title with `!` when
required.

With no previous version tag, Convco uses the configured initial version, `0.1.0`. In that first release PR, Cargo.toml
and Cargo.lock may already have the correct version and remain unchanged. After a release, Prepare release refuses to
reuse an existing version if there are no releasable changes.

The devenv Convco hook validates local commit messages at the `commit-msg` stage. The **Conventional commits / title**
job validates PR titles, including title edits, for squash merges. Keep the validated title when merging; use `!` in the
title for a breaking squash release. Make this job a required branch check alongside CI to enforce it at merge time. The
workflow uses repository-owned configuration and does not check out fork code on the enterprise runner.

## Local verification

Run the workspace tests and hooks through devenv:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=release-validation \
  devenv --no-tui test
```

The Rust release tests use disposable local Git repositories to check actual Convco calculations, generated changelogs,
squash merges, stale candidates, incorrect versions and retry behaviour. They do not create remote branches, tags or
releases.

For a local read-only candidate check, with the PR head and merged commit available in Git:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=release-candidate-review \
  devenv --no-tui shell --quiet -- \
  actions-release verify-candidate --commit <merged-sha> --head <pr-head-sha>
```

This local command checks release files and ancestry only. The publish workflow additionally verifies GitHub PR
provenance, trunk membership, CI and remote release state.
