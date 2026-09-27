# Rust Release Publication

Publish the approved Cargo version after consumer-owned build jobs succeed.
The shared workflow validates and attaches files, renders release notes from the reviewed changelog, and creates the version tag at the exact approved commit.
It never builds the application or publishes containers.
The [example](example.yaml) shows the complete lifecycle with an explicit empty asset set.

## Inputs and Permissions

The common inputs are `commit-sha`, `runs-on`, `type`, `flake-shell` and `devenv-installable`.
They have the same meaning as in [candidate](../release-rust-candidate/README.md).

- `artifact-run-id` defaults to the current run.
- `release-manifest-artifact` defaults to `release-manifest`.
  That artifact must contain a file named `release-manifest.json`.

Grant the calling job Contents write, Actions read and Pull Requests read permissions.
The publisher uses the job token and does not need registry credentials.
It returns `release-url`, `tag` and `version`.

## Artifact Manifest

Consumer jobs upload finished files through Actions artifacts.
Paths alone do not transfer files between jobs.
Declare every required file rather than discovering whatever outputs happen to exist.
Upload distinct artifact names for matrix members and wait for the entire matrix before publication.

```json
{
  "schema": 1,
  "version": "1.2.3",
  "tag": "v1.2.3",
  "commit_sha": "FULL_40_CHARACTER_RELEASE_SHA",
  "assets": [
    {
      "artifact": "release-linux-amd64",
      "files": ["app-linux-amd64.tar.gz", "app-linux-amd64.tar.gz.sha256"]
    },
    {
      "artifact": "release-linux-arm64",
      "files": ["app-linux-arm64.tar.gz", "app-linux-arm64.tar.gz.sha256"]
    }
  ],
  "reference_artifacts": [{ "artifact": "release-image-references", "file": "references.json" }]
}
```

Use `assets: []` for an intentionally asset-free release and `reference_artifacts: []` when no external publications are required.
Populate the version, tag and commit from candidate outputs, not duplicated version constants.
Artifact names and file path components accept letters, digits, hyphens, underscores and dots.
Paths must be relative and cannot traverse directories or use symlinks.
Files are attached using their basename; duplicate basenames and empty files fail publication.
Files ending in `.sha256`, and `SHA256SUMS`, use standard SHA256-plus-filename records and are verified against declared files.
Package executable permissions inside an archive when they must survive Actions artifact transport.

The source run must belong to the consumer repository and match the candidate SHA, default branch and a push or manual event.
Another run must have completed successfully; the current run can still be in progress while its publication job executes.
Missing, expired or duplicate artifact names fail publication.
API failures are errors and are never treated as absent releases or assets.

## Container Images and Other External Publications

Consumer jobs receive the version before the Git tag exists.
They publish images or other external deliverables and upload a reference document only after verifying their complete required output set.
For example, `release-image-references` contains:

```json
{
  "schema": 1,
  "version": "1.2.3",
  "commit_sha": "FULL_40_CHARACTER_RELEASE_SHA",
  "references": [
    {
      "name": "Container Image",
      "url": "https://github.com/example/app/pkgs/container/app",
      "reference": "ghcr.io/example/app:v1.2.3",
      "digest": "sha256:FULL_64_CHARACTER_HEX_DIGEST"
    }
  ]
}
```

Each reference requires a name and HTTPS URL.
`reference` and SHA256 `digest` are optional for plain links.
The publisher verifies document identity and adds the references to release notes.
The consumer verifies registry contents and component completeness.
Publishing images and publishing a GitHub release are separate operations; consumer image jobs must tolerate retries after partial failure.

## Publication and Recovery

The publisher revalidates the merged managed PR and approved files before mutation.
It creates or resumes a draft, uploads and verifies the declared asset set, and only then publishes the release.
The release creation specifies the exact candidate commit so later default-branch merges cannot enter the release.
No separate manual tag push is required.

Rerun the original release workflow to recover a failed build or upload, preserving its source SHA.
Temporary Actions artifacts can be replaced by their producer job within that same run.
Matching draft assets are reused; differing draft assets are replaced with the validated input.
An existing version tag pointing elsewhere fails immediately.
Published assets are verified and never replaced or supplemented, so this flow supports immutable releases.
An already-published candidate skips the consumer build path on a full workflow rerun.

Publication uses `GITHUB_TOKEN`; do not rely on its release or tag events to start other workflows.
Place required jobs before publication in the caller graph.
Use an explicit downstream dispatch if a separate post-release automation is needed.
