# Migrating from v3 to v4

Use this guide as the single entrypoint for migrating a repository (or directing an AI coding agent to migrate a repository) from `tars-cloud/actions` `v3` to `v4`.
Replace `@v4` with the published `v4` full 40-character commit SHA and a matching version comment once `v4` is released.
No release or repository migration happens automatically.

## 1. Update Organization or Repository Settings

Configure non-secret S3 routing values as GitHub Actions **Variables** and S3 credentials as GitHub Actions **Secrets**.
Remove deprecated secret aliases once no `v3` branches depend on them.

- Create Actions Variables (Settings, Secrets and variables, Actions, Variables) so reusable workflows inherit them automatically:
  - `TARS_CLOUD_S3_ENDPOINT` (migrated from secret `S3_ENDPOINT`).
  - `TARS_CLOUD_S3_BUCKET` (migrated from secret `S3_BUCKET`).
  - `TARS_CLOUD_S3_REGION` (migrated from secret `S3_REGION`).
- Create Actions Secrets (Settings, Secrets and variables, Actions, Secrets):
  - `TARS_CLOUD_S3_ACCESS_KEY` (migrated from secret `S3_ACCESS_KEY`).
  - `TARS_CLOUD_S3_SECRET_ACCESS_KEY` (migrated from secret `S3_SECRET_ACCESS_KEY`).
  - `TARS_CLOUD_S3_SESSION_TOKEN` (optional; only required when using temporary credentials, migrated from secret `S3_SESSION_TOKEN`).
- Confirm GitHub App secrets use Client IDs rather than numeric App IDs:
  - `CI_APP_CLIENT_ID` and `CI_APP_PRIVATE_KEY` (replaces `CI_APP_ID` / `APP_ID`).
  - `DEPENDENCY_APP_CLIENT_ID` and `DEPENDENCY_APP_PRIVATE_KEY` when accessing private dependency repositories.
- Verify runner variables match [project setup and runners](setup.md): `TARS_CLOUD_RUNNER_TYPE`, `TARS_CLOUD_RUNNER_AMD64` and `TARS_CLOUD_RUNNER_ARM64`.

## 2. Rename Reusable Workflows and Composite Actions

Update every `uses:` reference from `v3` names to `v4` names and target `@v4` (or the pinned `v4` release SHA).

- `.github/workflows/consumer-devenv-ci.yaml` becomes `.github/workflows/devenv-ci.yaml` ([copy-paste example](../workflows/devenv-ci/example.yaml)).
- `.github/workflows/consumer-devenv-update.yaml` becomes `.github/workflows/devenv-update.yaml` ([copy-paste example](../workflows/devenv-update/example.yaml)).
- `.github/workflows/consumer-trivy.yaml` becomes `.github/workflows/trivy.yaml` ([copy-paste example](../workflows/trivy/example.yaml)).
- `.github/workflows/consumer-codeql.yaml` becomes `.github/workflows/codeql.yaml` ([copy-paste example](../workflows/codeql/example.yaml)).
- `.github/workflows/consumer-cargo-crap.yaml` becomes `.github/workflows/cargo-crap.yaml` ([copy-paste example](../workflows/cargo-crap/example.yaml)).
- `.github/workflows/consumer-rust-release-candidate.yaml` becomes `.github/workflows/rust-release-candidate.yaml` ([copy-paste example](../workflows/rust-release-candidate/example.yaml)).
- `.github/workflows/consumer-rust-release-prepare.yaml` becomes `.github/workflows/rust-release-prepare.yaml` ([copy-paste example](../workflows/rust-release-prepare/example.yaml)).
- `.github/workflows/consumer-rust-release-publish.yaml` becomes `.github/workflows/rust-release-publish.yaml` ([copy-paste example](../workflows/rust-release-publish/example.yaml)).
- `composite/setup-consumer` becomes `composite/setup-project` ([copy-paste example](../composite/setup-project/example.yaml)).

All other composite action paths (`setup-nix`, `setup-cache`, `setup-devenv`, `run-devenv`, `setup-trivy`, `cargo-crap`, `release-rust`, `free-disk-space` and `report-status`) keep their directory names.

## 3. Update Secrets, Inputs and Outputs

Apply these interface changes to every caller workflow and direct composite invocation.

- S3 secrets on reusable workflow calls:
  - Remove `S3_ENDPOINT`, `S3_BUCKET` and `S3_REGION` from `secrets:` blocks; reusable workflows now read `vars.TARS_CLOUD_S3_ENDPOINT`, `vars.TARS_CLOUD_S3_BUCKET` and `vars.TARS_CLOUD_S3_REGION` automatically.
  - Rename `S3_ACCESS_KEY` to `TARS_CLOUD_S3_ACCESS_KEY` and `S3_SECRET_ACCESS_KEY` to `TARS_CLOUD_S3_SECRET_ACCESS_KEY` in `secrets:` blocks.
  - Rename `S3_SESSION_TOKEN` to `TARS_CLOUD_S3_SESSION_TOKEN` if used; omit it when using static credentials.
  - On direct `composite/setup-cache` or `composite/setup-project` calls, wire `s3-endpoint`, `s3-bucket` and `s3-region` from `${{ vars.TARS_CLOUD_S3_* }}` and wire `s3-access-key`, `s3-secret-access-key` (and optional `s3-session-token`) from `${{ secrets.TARS_CLOUD_S3_* }}`.
- Removed legacy App ID aliases:
  - Replace `secrets.app-id` with `secrets.client-id` on `devenv-update.yaml`, `cargo-crap.yaml`, `rust-release-prepare.yaml` and `rust-release-publish.yaml`.
  - Replace `secrets.dependency-app-id` with `secrets.dependency-client-id` on all reusable workflows that accept scoped dependency App credentials.
  - Replace `${{ secrets.CI_APP_ID }}` or `${{ secrets.APP_ID }}` with `${{ secrets.CI_APP_CLIENT_ID }}`.
- Cargo CRAP coverage backend:
  - Remove `coverage-tool` from `cargo-crap.yaml` and `composite/cargo-crap` calls; `tarpaulin` support is removed in `v4` and `cargo-llvm-cov` is the sole coverage backend.
  - Ensure `cargo-llvm-cov` (and LLVM tools) are declared in the repository's devenv or flake environment; remove `cargo-tarpaulin` if no longer needed.
- Cargo CRAP inputs and outputs:
  - Rename input `update-records-pr` to `publish-records` (defaults to `true`; omit if keeping the default).
  - Replace references to outputs `records-pr-url` or `pr-url` with `records-branch` and `records-commit`.
- Hosted disk cleanup defaults:
  - `cleanup-android`, `cleanup-dotnet`, `cleanup-haskell`, `cleanup-boost` and `cleanup-swift` now default to `true` across `devenv-ci.yaml`, `devenv-update.yaml`, `trivy.yaml`, `codeql.yaml`, `cargo-crap.yaml` and `composite/free-disk-space`.
  - Remove redundant `cleanup-*: true` overrides from caller workflows.
  - Add an explicit `cleanup-<sdk>: false` override only if a hosted Linux job needs that specific preinstalled SDK.
  - Self-hosted runners still skip cleanup automatically.

## 4. Cut Over Cargo CRAP to the Orphan Branch

`v4` replaces `crap/next` recording PRs with direct commits to an orphan `crap` branch (configurable via `records-branch`).
Complete these steps for every repository that runs `cargo-crap.yaml`.

- Update the caller workflow permissions:
  - Ensure the caller workflow (or job calling `cargo-crap.yaml`) grants `contents: write`, `pull-requests: write` and `actions: read`.
  - Remove `client-id` and `app-private-key` from the `cargo-crap.yaml` `secrets:` block unless repository rules protect the `crap` branch and block `GITHUB_TOKEN` pushes.
  - Use the [Cargo CRAP example](../workflows/cargo-crap/example.yaml) as the baseline caller template.
- Remove legacy committed CRAP files from the default branch:
  - Delete `.github/crap/baseline.json` and `.github/badges/crap-badge.json` (plus any per-`analysis-id` files under `.github/crap/` and `.github/badges/`).
  - Remove the empty `.github/crap/` and `.github/badges/` directories if nothing else uses them.
- Clean up stale `v3` recording PRs and branches:
  - Close any open `crap/next` PR (or `crap/next-<analysis-id>` PRs).
  - Delete the remote `crap/next` branch after closing the PR.
- Update README badge URLs and click-through links:
  - For default `analysis-id` (`default`), point Shields.io at `https://raw.githubusercontent.com/<owner>/<repo>/crap/crap-badge.json` (URL-encoded inside `https://img.shields.io/endpoint?url=...`) and link the badge to `https://github.com/<owner>/<repo>/blob/crap/crap-badge.json`.
  - For a custom `analysis-id` (for example `cli`), point Shields.io at `https://raw.githubusercontent.com/<owner>/<repo>/crap/<analysis-id>/crap-badge.json` and link to `https://github.com/<owner>/<repo>/blob/crap/<analysis-id>/crap-badge.json`.
- First run behavior:
  - On the first push to the default branch after merging the migration PR, the `record` job automatically creates the orphan `crap` branch (`0` parents) with `baseline.json` and `crap-badge.json`.
  - Subsequent default-branch pushes fast-forward the `crap` branch only when measured scores change and preserve files from other `analysis-id`s in the same tree.

## 5. Simplify Caller Workflows

Compare each file under `.github/workflows/` against its copy-paste template in `workflows/<name>/example.yaml`.

- Remove inputs that match `v4` defaults (`cleanup-*`, `publish-records`, `baseline-branch` when tracking the repository default branch).
- Keep branch triggers aligned with the repository's default branch (`trunk`, `main` or `master`).
- Check GitHub branch protection rules or rulesets if you renamed any caller job display names, and update required status check names accordingly.

## 6. Post-Migration Verification Checklist

Run these checks in the target repository before merging the migration PR.

- Search the repository for leftover `v3` identifiers and confirm zero unintended matches:
  - `consumer-devenv-ci`, `consumer-devenv-update`, `consumer-trivy`, `consumer-codeql`, `consumer-cargo-crap`, `consumer-rust-release-`.
  - `setup-consumer`, `app-id`, `dependency-app-id`, `CI_APP_ID`, `APP_ID`.
  - `tarpaulin`, `coverage-tool`, `update-records-pr`, `records-pr-url`, `crap/next`, `.github/crap`, `.github/badges/crap-badge.json`.
  - Bare `S3_ENDPOINT`, `S3_BUCKET`, `S3_REGION`, `S3_ACCESS_KEY`, `S3_SECRET_ACCESS_KEY`, `S3_SESSION_TOKEN`.
  - `@v3`.
- Validate workflow syntax and hooks locally (for example `prek run --all-files`, `actionlint` and `yamllint`).
- Open the migration PR and confirm every workflow job passes, including S3 cache restoration on self-hosted runners if enabled.
- Merge the migration PR and verify on the default-branch push that:
  - `Cargo CRAP` creates or updates the orphan `crap` branch with `baseline.json` and `crap-badge.json`.
  - The README CRAP badge renders from the `crap` branch without returning 404.
  - No `crap/next` PR is created.
