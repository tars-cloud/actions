# Shared GitHub Actions

This public repository provides opinionated composite actions for projects that use devenv.
The MVP consists of setup-cache, setup-trivy, setup-devenv, free-disk-space and setup-nix actions for Linux AMD64 and ARM64.
Read the approved implementation handoff in `.tars/scratch/PROMPT.md` when present and preserve its settled decisions.

## Development environment

Run commands from this repository's root, beside devenv.nix, through its devenv shell.
Always use --no-tui and pass the command after --.
For agents and CI, use:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=ai-agent \
  devenv --no-tui shell --quiet -- <command>
```

Run the configured checks with:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=ai-agent \
  devenv --no-tui shell --quiet -- prek run --all-files

CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=ai-agent \
  devenv --no-tui test
```

The development environment supplies actionlint, ShellCheck, shfmt, yamllint, jq, ripgrep, Python, Bun, Node, Rust and Trivy.
Declare additional development tools in devenv.nix and preserve devenv.lock so developers and CI use the same dependency revisions.
Do not install tools globally, use ad hoc downloaded binaries or prepend Nix store paths to PATH.
Use prek for Git hooks; do not bypass hooks or invoke the legacy pre-commit CLI.
No real secrets are required for local static checks or unit tests.

## Consumer runtime contract

All consumer project tools run inside the consumer's devenv environment, either direct (devenv.nix/devenv.yaml/devenv.lock) or flake-integrated (flake.nix/flake.lock).
Direct mode uses devenv --no-tui shell; flake mode uses nix develop --impure with an inner Bash invocation and the selected devShell.
setup-devenv, setup-cache and setup-trivy independently accept type (devenv by default, or flakes) and flake-shell (default .#default); do not auto-detect environment type.
This repository itself uses direct mode; retain the development commands above.
Never use an ambient runner project tool as a substitute or independently download a project tool when it is missing.
Devenv and explicitly enabled Cachix are bootstrap exceptions: reuse preinstalled CLIs or install them through the approved Nix commands when absent.
setup-cache and setup-devenv both invoke the idempotent shared setup-nix action, which reuses Nix or bootstraps it only on GitHub-hosted runners.
setup-devenv may install the devenv CLI when missing.
Use `nix profile add nixpkgs#devenv` without an action-managed devenv version pin; the owner prefers the current command name.
Reuse an installed devenv CLI; install only when required and missing, without automatic upgrades on subsequent jobs.
Honour the runner's nixpkgs registry mapping; do not change its channel or assume that the mapping selects stable.
Self-hosted runners must already provide Nix; do not install or reconfigure it there.
Remaining bootstrap details are specified in the handoff.
setup-trivy must execute Trivy through devenv and fail with instructions to add Trivy to the consumer's devenv packages when it is unavailable.
Project-tool commands run through the selected environment; file-cache restoration itself must not require package managers or entering that environment before setup-devenv.
setup-cache detects manifests and hashes lockfiles without running project tools; use conventional Linux cache paths with explicit overrides for custom locations.
setup-cache must not enter the project environment to query cache paths or tool versions; it must work before devenv setup.
GitHub's action runtime, including the runtime for pinned upstream cache actions, is distinct from consumer project tooling.

Keep all five actions individually callable; setup-cache and setup-devenv share setup-nix as an explicitly authorised prerequisite.
Do not invoke cache setup from setup-devenv or setup-trivy, or disk cleanup from another action.
Nix-cache infrastructure is managed separately; do not add Nix-store archive setup.
Optional Cachix setup belongs exclusively in setup-cache: cachix-name enables reads, cachix-name plus cachix-token enables writes, and no name disables it.
Fork PRs always use GitHub archive storage and disable Cachix writes, regardless of supplied credentials.
Archive saves require job success, with PR/ref scopes and compatible default-branch fallback; never restore PR entries into default-branch jobs.
When enabled, reuse the runner's Cachix CLI or install it with `nix profile add nixpkgs#cachix` if missing; do not require the project shell to supply this bootstrap tool.
Never infer Cachix configuration from the repository owner or add Cachix setup to setup-devenv.
Protect persistent self-hosted runners from hosted-image cleanup.

## Implementation and tests

Pin upstream actions to reviewed full commit SHAs with accurate version comments.
Treat checkout paths and action paths separately: bundled scripts belong to github.action_path, while manifests belong to the consumer checkout.
Pass untrusted inputs through environment variables or argument arrays rather than interpolating them into shell source.
Never log credentials or persist them in cache archives.

Test detection, exclusions, keys, backend selection and error behaviour with fixtures and mocks.
Keep tests independent of real lab endpoints and credentials; use reserved example hostnames for fixtures.
Never test cleanup safety by deleting actual runner tooling.
Add CI that consumes this repository's own actions at the revision under test, once implemented.
Independent-action coverage must include setup-cache before devenv is available, and setup-devenv without setup-cache.
CI must cover Linux AMD64 and ARM64, direct devenv and flake integration, and nested setup-nix calls at the revision under test.
Report unit/static checks separately from live hosted-runner and lab-S3 verification.

## Repository hygiene

Use repository-relative paths and links, or full GitHub links for other repositories.
Never commit machine-specific checkout paths, credentials or local environment artefacts.
Use Markdown lists rather than tables and write one sentence per line.
Keep temporary work in the ignored `.tars/scratch/` directory and remove disposable scripts after use.
Retain PROMPT.md as the requested handoff; it is not a disposable script.
Preserve unrelated user changes and do not publish releases or migrate consumers without authorisation.
