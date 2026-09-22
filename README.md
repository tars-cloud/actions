# TARS Cloud shared actions

Five composite actions for projects using direct devenv or devenv-integrated Nix flakes.
Supported platforms are Linux X64 and native ARM64 on GitHub.com, with runner **2.336.0 or newer**.
Self-hosted runners must already provide working Nix.

- [setup-nix](setup-nix/README.md): idempotent Nix prerequisite.
- [setup-cache](setup-cache/README.md): detected dependency-download archives and optional Cachix.
- [setup-devenv](setup-devenv/README.md): bootstrap and warm the selected project shell.
- [setup-trivy](setup-trivy/README.md): validate the environment's Trivy package and report its version.
- [free-disk-space](free-disk-space/README.md): explicit hosted SDK cleanup, always skipped on self-hosted runners.

Each action is independently callable.
Only setup-cache and setup-devenv compose setup-nix as a shared prerequisite.
Nested `$/` references follow the exact action revision selected by the caller, using [GitHub's self-repository syntax](https://github.blog/changelog/2026-07-30-reference-same-repository-actions-with-self-repository-syntax/).
GitHub Enterprise Server, Windows, macOS and emulated ARM64 are outside this MVP.

## Direct devenv consumer

The example `v1` references require a release to be published first.
For controlled updates, replace them with a reviewed release commit SHA.
The repository's [CI](.github/workflows/ci.yml) instead checks out and invokes the revision under test.

```yaml
jobs:
  build:
    runs-on: ubuntu-24.04
    env:
      CI: 'true'
      SECRETSPEC_PROVIDER: env
      SECRETSPEC_REASON: github-actions
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - uses: tars-cloud/actions/free-disk-space@v1
      - uses: tars-cloud/actions/setup-cache@v1
      - uses: tars-cloud/actions/setup-devenv@v1
      - uses: tars-cloud/actions/setup-trivy@v1
      - name: Install dependencies and run project checks
        shell: devenv --no-tui shell --quiet -- bash --noprofile --norc -e -o pipefail {0}
        run: |
          cargo fetch --locked
          cargo test --locked
          trivy fs .
```

Declare Cargo and Trivy in the consumer's devenv configuration before using this example.
Use the install/build commands appropriate to your project; cache hits do not install dependencies.
A setup action cannot change subsequent steps' shells, so specify the shell explicitly.
Optional cleanup runs before setup and can remove SDKs that some projects need; review its categories first.
Archive saves run in post-job hooks only after a successful job.

## Flake-integrated consumer

Select `type: flakes` explicitly on each action that accesses the environment.
The default shell selector is `.#default`; this example chooses `.#ci`.
No standalone devenv CLI or devenv.yaml is required.
The devShell must integrate devenv; [the fixture](tests/fixtures/flakes/flake.nix) demonstrates default and named shells for both architectures.

```yaml
steps:
  - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
  - uses: tars-cloud/actions/setup-cache@v1
    with:
      type: flakes
      flake-shell: .#ci
  - uses: tars-cloud/actions/setup-devenv@v1
    with:
      type: flakes
      flake-shell: .#ci
  - uses: tars-cloud/actions/setup-trivy@v1
    with:
      type: flakes
      flake-shell: .#ci
  - name: Check the flake environment
    shell: nix develop --impure .#ci --command bash --noprofile --norc -e -o pipefail {0}
    env:
      CI: 'true'
      SECRETSPEC_PROVIDER: env
      SECRETSPEC_REASON: github-actions
    run: devenv-flake-test
```

## Runtime requirements

Runners need Bash, standard Linux utilities and working GitHub Actions JavaScript runtimes.
Archive transports require GNU tar and gzip; zstd is recommended for cache compression.
Hosted Nix bootstrap also requires the prerequisites of the pinned cachix/install-nix-action.
File discovery uses a dependency-free internal Node 24 action, executed by the runner; it never installs Node, Python or a package manager.
Self-hosted runners must support Node 24 actions (including compatible glibc and runner runtime configuration).

Project tools execute through the selected environment and follow devenv.lock or flake.lock.
The bootstrap exceptions are Nix on missing hosted runners, devenv in direct mode, and Cachix when explicitly enabled.
Existing bootstrap CLIs are reused without upgrades.
Missing devenv and Cachix use `nix profile add nixpkgs#devenv` and `nix profile add nixpkgs#cachix`, respecting the runner's nixpkgs registry mapping.
That mapping does not necessarily select stable or the latest upstream release.
No action installs secretspec or obtains deployment secrets.
Shell preparation uses the noninteractive environment secrets provider, preserves profile and credential values, and permits normal consumer-defined shell hooks to execute.

## Development and validation

Run from this repository's root:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=validation \
  devenv --no-tui shell --quiet -- node --test tests/*.test.cjs
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=validation \
  devenv --no-tui shell --quiet -- prek run --all-files
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=validation \
  devenv --no-tui test
```

Unit tests exercise policy, detection, paths, compatibility, trust scopes, shell dispatch, missing tools and mocked cleanup.
CI exercises independent setup-devenv, cache-only cold/post-save/warm jobs, repeated setup-nix, real Trivy, and default/named flake shells on AMD64 and ARM64.
CI never selects persistent lab runners for cleanup.
See [validation evidence](docs/validation.md) for executed checks and remaining live-service coverage.
See [release instructions](docs/releases.md) for the single repository-wide version policy.
