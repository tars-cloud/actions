# GitHub Actions

This public repository provides opinionated composite actions for projects that use devenv.

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

The development environment supplies actionlint, action-validator, ShellCheck, shfmt, yamllint, jq, ripgrep, Python,
Bun, Node, Rust and Trivy.

Declare additional development tools in devenv.nix and preserve devenv.lock so developers and CI use the same dependency
revisions.

Do not install tools globally, use ad hoc downloaded binaries or prepend Nix store paths to PATH.

Use prek for Git hooks; do not bypass hooks or invoke the legacy pre-commit CLI.

No real secrets are required for local static checks or unit tests.

## Consumer runtime contract

All consumer project tools run inside the consumer's devenv environment, either direct
(devenv.nix/devenv.yaml/devenv.lock) or flake-integrated (flake.nix/flake.lock).

- Direct mode uses devenv --no-tui shell;
- Flake mode uses nix develop --impure with an inner Bash invocation and the selected devShell.

This repository itself uses direct mode; retain the development commands above.

Never use an ambient runner project tool as a substitute or independently download a project tool when it is missing.

Devenv and explicitly enabled Cachix are bootstrap exceptions: reuse preinstalled CLIs or install them through the
approved Nix commands when absent.

## Implementation and tests

Pin upstream actions to reviewed full commit SHAs with accurate version comments.

Treat checkout paths and action paths separately: bundled scripts belong to github.action_path, while manifests belong
to the consumer checkout.

Pass untrusted inputs through environment variables or argument arrays rather than interpolating them into shell source.

Never log credentials or persist them in cache archives.

Test detection, exclusions, keys, backend selection and error behaviour with fixtures and mocks.

Keep tests independent of real lab endpoints and credentials; use reserved example hostnames for fixtures.

Never test cleanup safety by deleting actual runner tooling.

Add CI that consumes this repository's own actions at the revision under test, once implemented.

CI must cover Linux AMD64 and ARM64, direct devenv and flake integration, and nested setup-nix calls at the revision
under test.

## Repository hygiene

Start every YAML file with `---`.

Start every workflow and composite action step with `id`, followed by a descriptive `name`, and leave one blank line
between steps.

Put a comment header before every workflow job:

```yaml
---
# -------------------------------------------------
# Job Name
# -------------------------------------------------
```

Use expanded nested attribute sets in Nix instead of dotted assignments.

Keep linter settings in `devenv.nix` where supported, including the path to `.prettierrc.yaml`.

Use repository-relative paths and links, or full GitHub links for other repositories.

Never commit machine-specific checkout paths, credentials or local environment artefacts.

Use Markdown lists rather than tables and write one sentence per line.

Keep temporary work in the ignored `.tars/scratch/` directory and remove disposable scripts after use.

Preserve unrelated user changes and do not publish releases or migrate consumers without authorisation.

Please remove all mannered prose.
