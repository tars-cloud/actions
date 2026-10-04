# setup-consumer

[Copyable workflow](example.yaml).

Check out the consumer, restore selected language caches and prepare its direct devenv environment or flake devShell.
All nested actions use the caller-selected revision.
The action inherits its caller job's runner and supports Linux AMD64 and ARM64 on hosted and self-hosted runners.

Checkout inputs are `ref`, `fetch-depth`, `lfs` and `submodules` (`false`, `true` or `recursive`).
Git credentials are not retained after checkout.
Environment inputs match [setup-devenv](../setup-devenv/README.md), including `type`, `working-directory`, `flake-shell`, `system`, `devenv-installable` and `warmup`.
Cache inputs match [setup-cache](../setup-cache/README.md), including `tools`, exclusions, compiled Cargo cache discriminators and optional S3 credentials.

`secretspec-profile` selects a profile before environment evaluation and subsequent commands.
Empty retains the consumer's configured profile.
Keep `SECRETSPEC_PROFILE` and any command-specific variables in a clean shell's `clean.keep`.
The provider remains `env` for unattended execution.
This input selects a profile; it does not provide the consumer's application secrets.

## Dependency Authentication

Existing `dependency-token` credentials remain supported.
Alternatively supply `app-id`, `app-private-key`, `dependency-owner` and a nonempty JSON `dependency-repositories` array.
Repository entries are names within one owner, such as `["private-nix-input", "shared-crate"]`.
The owner defaults to the consumer repository owner.
Tokens are requested with Contents read permission and are revoked by the upstream App action after the job.
Checkout normally uses the repository token.
Explicitly include the consumer repository in the dependency list under the same owner to use the App token for private LFS files and submodules.
Dependencies under another installation owner receive their own token; consumer checkout still uses its repository token.
Cross-owner private submodules require a credential that can read both repositories, supplied through `dependency-token`.
Token and App authentication are mutually exclusive.

The `dependency-token` output is a masked step credential for subsequent environment actions in the same job.
Pass it to `run-devenv` through `github-token`; do not forward it as a workflow output or place it in cache files.
Dependency access uses the existing GitHub Nix input token support.
Project package managers retain their own registry authentication configuration.
Publication tokens belong to their respective release or badge jobs and are separate from dependency read tokens.

Fork PRs, Dependabot runs and `pull_request_target` runs receive neither supplied dependency credentials nor S3 cache credentials.
The public checkout and default repository token remain available.
Unavailable private inputs fail normally; the action does not expand App scope or execute untrusted code to retrieve credentials.

Existing reusable workflows compose the owned checkout helper while keeping their specialized cache and environment steps.
New consumer CI composes the complete setup action.
