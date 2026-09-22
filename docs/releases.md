# Releasing all shared actions

All five public actions share one repository-wide semantic version. Use a release such as `v1.0.0` and maintain the
moving major tag `v1` for compatible updates. Do not create separate action-specific tags or versions.

Before a release:

1. Review the complete diff, upstream immutable pins and each action's public interface. Update the reviewed pin
   assertions and optional upstream integration tests together with any dependency change.
2. Run local unit, metadata, static and devenv checks.
3. Require the self-consuming CI matrix to pass on AMD64 and ARM64, including cold/post-save/warm cache verification and
   direct/default/named environment coverage.
4. Review fork credentials, S3 isolation and self-hosted cleanup protection.
5. Record which live S3 and Cachix paths have actually been verified; do not infer lab operation from unit tests.
6. Update release notes and compatibility documentation, then obtain separate owner authorization to publish.

After authorization, tag the reviewed commit with the semantic version, publish its release notes, and move the shared
major tag to that same commit. Never move an immutable semantic-version tag. Breaking changes require a new major
version. Implementation of this MVP does not authorize creating releases or moving tags.

Consumers choosing `uses: tars-cloud/actions/setup-cache@v1` receive compatible updates automatically when v1 moves.
Consumers choosing `uses: tars-cloud/actions/setup-cache@<reviewed-release-commit>` receive no changes until their pin
is updated. Use Dependabot or another reviewed update-PR workflow for immutable consumer pins. Apply the same policy to
all five actions. The repository's Dependabot configuration updates pinned upstream actions in workflows and composite
action directories.
