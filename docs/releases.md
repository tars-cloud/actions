# Releases

The repository, its composite actions, Tact and the release utilities share `[workspace.package].version` in Cargo.toml.
Release preparation updates the lockfile entries for `tact`, `actions-release` and `actions-crap`.
It compares every workspace member's lockfile version with Cargo metadata before publishing the candidate PR.
Keep `--locked` in CI so an inconsistent manifest and lockfile fail before tools or tests run.
Release preparation writes the exact version returned by `convco version --bump`.
Publication reads that version from the reviewed, merged commit and publishes `v<version>`.
The Nix package reads the same Cargo.toml version.

## Prepare a release

1. Merge the feature, fix and dependency PRs to include.
2. Successful trunk push CI automatically creates or refreshes the single `release/next` PR.
3. Review its Cargo.toml, Cargo.lock and CHANGELOG.md changes.
4. Wait for the refreshed PR's CI and merge using its generated `chore(release): v<version>` title.
5. Successful trunk push CI for that merged commit automatically starts publication.

Normal PRs do not bump Cargo.toml or maintain the changelog.
Leaving the release PR open batches subsequent merges into the same PR, including any change to its proposed version.
Documentation-only and chore merges refresh an existing candidate's base after CI, without requesting another version increment.
With no releasable changes, automation succeeds without opening a PR.
Identical retries preserve the branch commit and avoid unnecessary PR updates or CI runs.
Preparation refuses to proceed if more than one matching release PR exists.

**Repository: Prepare Release** remains available as a manual dispatch from **trunk**.
Both automatic and manual preparation use current trunk and refresh the same release PR.
Automatic preparation waits for successful CI on that exact trunk revision.
If trunk advances during preparation, the run reports that the newer CI completion will handle the refresh.
Wait for a refresh before merging a release PR whose base is behind trunk.
Do not use GitHub's **Update branch** button on a release PR; regeneration keeps its version and changelog consistent.
If an outdated candidate was already merged, the next successful trunk CI or manual **Repository: Prepare Release** rebuilds it from current trunk for review and merge.

Generated changelogs keep Markdown linting enabled and configure MD024 for sibling headings within that file.
Sections such as Features can repeat under different versions, while duplicate sections within one version still fail.
After merging a preparation-tool fix, start a new **Repository: Prepare Release** dispatch from trunk so it uses the corrected tooling.

Repository: Prepare Release uses the organization's CI GitHub App, following the platform repository's credential names. Make
`CI_APP_CLIENT_ID` (or the fallback `CI_APP_ID`) and `CI_APP_PRIVATE_KEY` available as Actions secrets to this
repository. Organization secrets restricted to private repositories are unavailable here because this repository is
public. The App must be installed for this repository with Contents and Pull requests write permissions. Each workflow
mints a token scoped to this repository after environment setup and revokes it at job completion. App-created PRs and
branch updates trigger the normal CI workflows without an additional dispatch.
The App also writes release tags and GitHub Releases.
The organization setting allowing `GITHUB_TOKEN` to create PRs is not required.
API reads use the job's `GITHUB_TOKEN` through `GH_READ_TOKEN`, with Contents, Pull requests and Actions read permissions.
The App does not need Actions permissions.
Both release workflows run in the `enterprise/tars-cloud` runner group and share one concurrency group with `queue: max`.
Pending preparation and publication runs wait instead of replacing one another.

## Automatic release decisions

**Repository: Release Automation** runs after each trunk CI completion and records the triggering run, commit, result and release decision in its job summary.
Cancelled or failed CI produces an explanation and does not set up devenv or mint an App token.
Only successful same-repository trunk push CI can reach automatic preparation or publication.
PR runs and release-branch pushes cannot recursively prepare releases.

- An ordinary merge refreshes the release PR against current trunk once that revision passes CI.
- A merged release PR publishes its tested commit, even when later trunk commits already exist.
- A valid merged release awaiting publication blocks another release PR, including when the tag exists but the GitHub Release or major alias is unfinished.
- Successful publication checks current trunk again and prepares accumulated changes if its CI has passed.
- If newer trunk CI is still running, its eventual completion handles preparation.
- An already published release is a successful no-op on automatic retries, followed by the same current-trunk check.

Trunk CI uses a separate concurrency group for each commit, and nested security tests use the caller's run ID.
A later merge cannot cancel the tests required to publish an earlier release commit.
Superseded PR CI can still be cancelled.
The validator's bundled schema is extended for `concurrency.queue`; Tact checks the release queue and cancellation contracts.

## Publish a release

Review the release diff, upstream immutable pins, action interfaces and compatibility notes before publication. The
required CI matrix covers native AMD64 and ARM64, direct and flake environments, and cold/post-save/warm caches. Record
any live S3 validation separately; fixture tests do not establish live service operation.

Merging the release PR approves publication.
When its **Repository: CI** trunk push run completes successfully, **Repository: Release Automation** uses that run's exact commit SHA.
Publication requires a merged `release/next` PR from this repository into trunk.
Failed, cancelled, fork and PR CI runs cannot trigger publication.
Changing CHANGELOG.md alone does not qualify a commit for release.

The workflow loads its tooling from the default branch and verifies the same-repository merged release PR, its files and
ancestry, the calculated version, the Cargo.lock workspace versions, and successful CI for the exact candidate trunk
commit. Later trunk commits are excluded from the release; their presence does not invalidate an already correctly
merged candidate. Changes merged into trunk between preparation and the release merge do invalidate the candidate.

The workflow creates a fixed `v<version>` tag and GitHub Release, then moves the major alias such as `v1` to that
commit. The initial `0.x` series uses `v0`. Full version tags are never moved, and older releases cannot move an alias
backwards. The release body contains up to five changelog highlights and a link to CHANGELOG.md at the fixed version
tag. It does not copy the entire changelog.

Manual **Repository: Release Automation** dispatch remains available for publication retries and recovery. Select **trunk** and supply the full
40-character merged release commit SHA as `commit`. Use the merged commit, not the release branch head or a later fix
commit. For a publishing-tool fix, start a new dispatch from trunk after the fix merges so the run uses the corrected
tooling. If publication stops after creating a tag or release, retry with the same candidate commit SHA. Existing tags
must point to that exact commit; conflicting tags cause a failure. Published release notes are preserved on retries.
This process does not publish crates or attach compiled binaries.

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
release when Prepare Release is run. Dependabot uses the `build` prefix and dependency scope for both Cargo and GitHub
Actions updates. Review dependency upgrades for consumer-facing breaking changes and mark the PR title with `!` when
required.

With no previous version tag, Convco uses the configured initial version, `0.1.0`. In that first release PR, Cargo.toml
and Cargo.lock may already have the correct version and remain unchanged.
After a release, preparation succeeds without changing anything if there are no releasable changes.

The devenv Convco hook validates local commit messages at the `commit-msg` stage. The **Conventional Commits / Validate PR Title**
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
squash merges, stale candidates, incorrect versions and retry behaviour.
Lifecycle tests use local bare Git remotes and a mocked GitHub API to verify PR reuse, pending publication, out-of-order CI and partial publication recovery.
They do not create real GitHub branches, tags, PRs or releases.

For a local read-only candidate check, with the PR head and merged commit available in Git:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=release-candidate-review \
  devenv --no-tui shell --quiet -- \
  actions-release verify-candidate --commit <merged-sha> --head <pr-head-sha>
```

This local command checks release files and ancestry only. The publish workflow additionally verifies GitHub PR
provenance, trunk membership, CI and remote release state.
