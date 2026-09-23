# setup-cache

Restore dependency downloads before constructing the consumer's devenv environment, then save through success-only
post-job hooks. Supports GitHub.com Linux X64 and ARM64 runners 2.336.0+. It calls setup-nix at the same action
revision, but never installs devenv, enters a project shell or runs package managers.

```yaml
- uses: tars-cloud/actions/composite/setup-cache@v1
  with:
    s3-endpoint: ${{ secrets.S3_ENDPOINT }}
    s3-bucket: ${{ vars.S3_BUCKET }}
    s3-region: ${{ vars.S3_REGION }}
    s3-access-key: ${{ secrets.S3_ACCESS_KEY }}
    s3-secret-key: ${{ secrets.S3_SECRET_KEY }}
```

Unset secrets and variables resolve to empty inputs, so the same call works with no S3 configuration. For GitHub storage
only, omit all inputs. The action cannot read secrets from its hosting repository; callers supply their own
configuration.

## Detection and environment selection

- `tools`: defaults to `auto`; alternatively use a comma/space-separated list of `cargo`, `python`, `bun`, `trivy`, or
  `none` to disable dependency archives.
- `working-directory`: defaults to `.`; recursive discovery starts at this consumer environment root.
- `exclude`: additional glob exclusions, one per line, matched against relative paths and entry names.
- `python-manager`: defaults to `auto`; use `uv` or `pip` for ambiguous Python projects.
- `type`: defaults to `devenv`; select `flakes` explicitly for a devenv-integrated flake.
- `flake-shell`: defaults to `.#default`; use the same selector as setup-devenv and setup-trivy.
- `cargo-target`: defaults to `false`; opt into a separate compiled-output archive.
- `cargo-build-target`: optional target-triple key discriminator; otherwise uses visible `CARGO_BUILD_TARGET` or the
  native runner architecture.
- `cargo-build-variant`: optional compiled-output discriminator for features, profiles and matrix variants.

Cargo detection requires Cargo.toml, and hashes discovered Cargo.toml and Cargo.lock files. Bun requires bun.lock,
bun.lockb or `packageManager: bun@...` in package.json; package.json alone emits an override notice. Python uses uv.lock
for uv and requirements*.txt for pip; a pyproject.toml alone emits a manager-selection notice. Mixed nested uv and pip
projects receive separate archives. Trivy requires trivy.yaml or trivy.yml, excluding `.github/workflows`; scan-only
jobs can select `tools: trivy` explicitly. No detection step sources project code or executes a language runtime from
the consumer. Missing tool lockfiles are supported, but the selected environment's lockfile is required.

Discovery ignores symlinks, `.git`, `.devenv`, `.direnv`, `.tars`, scratch, node_modules, target, vendor, vendors,
.venv, venv, **pycache**, dist, build, .cache, .bun, .cargo, .next and coverage trees. Exclusions apply to nested
projects and Trivy configuration detection. Only `.cargo/config` and `.cargo/config.toml` are inspected inside otherwise
excluded `.cargo` directories, to distinguish compiler/target settings; Cargo installation contents remain excluded. Use
an excluded directory name or a subtree glob such as `examples/**` to add exclusions.

```yaml
- uses: tars-cloud/actions/composite/setup-cache@v1
  with:
    tools: cargo,python,bun,trivy
    python-manager: uv
    cargo-target: "true"
    cargo-build-target: aarch64-unknown-linux-gnu
    cargo-build-variant: release-feature-a
    exclude: |
      examples/**
      generated
```

## Cache paths

Each path resolves from its explicit input, then a job-visible tool variable, then the conventional Linux default.
Relative paths resolve from working-directory; `~/` expands to the runner home. Inputs accept literal directories rather
than globs or multiline lists. Selected paths are exported to later steps through the corresponding tool variables.
Values set only inside a later project shell cannot be discovered here; supply matching explicit overrides and avoid
overriding them inconsistently inside the shell.

- `cargo-cache-path`: Cargo home, then `CARGO_HOME`, then `~/.cargo`. Only `registry/index`, `registry/cache` and
  `git/db` are archived; credentials, config, extracted sources and Git checkouts are excluded.
- `cargo-target-path`: then `CARGO_TARGET_DIR`, then `working-directory/target`. Applies only with `cargo-target: true`;
  point nested workspaces at a shared target directory or invoke the action with their environment root.
- `uv-cache-path`: then `UV_CACHE_DIR`, then `${XDG_CACHE_HOME:-~/.cache}/uv`.
- `pip-cache-path`: then `PIP_CACHE_DIR`, then `${XDG_CACHE_HOME:-~/.cache}/pip`.
- `bun-cache-path`: then `BUN_INSTALL_CACHE_DIR`, then `${BUN_INSTALL:-~/.bun}/install/cache`.
- `trivy-cache-path`: then `TRIVY_CACHE_DIR`, then `${XDG_CACHE_HOME:-~/.cache}/trivy`.

No virtual environments, node_modules or Nix-store archives are included by default. Do not override cache paths to
directories containing credentials or unrelated configuration. Missing directories on the first run are normal; upstream
saves warn/skip when no files exist. uv's downloaded wheels are preserved; this action never runs `uv cache prune --ci`.
Trivy keys rotate daily in UTC, with compatible fallback; normal Trivy database freshness checks remain enabled.

```yaml
- uses: tars-cloud/actions/composite/setup-cache@v1
  with:
    tools: cargo,python,bun,trivy
    python-manager: uv
    cargo-cache-path: .cache/cargo
    uv-cache-path: .cache/uv
    bun-cache-path: .cache/bun
    trivy-cache-path: .cache/trivy
```

## Storage and trust policy

Fork PRs always select official GitHub cache storage, ignoring even incomplete S3 inputs and disabling Cachix writes.
Fork identity compares the PR head repository with the consuming repository, including PR-bearing events such as
pull_request_target. This does not make checking out untrusted code in a privileged workflow safe. Consumers must
withhold private S3 and Cachix write credentials from fork jobs.

For other jobs, `runner.environment` selects the backend:

- Self-hosted with every required S3 input: S3-compatible storage through RunsOn cache.
- Self-hosted with all S3 configuration absent: official GitHub cache storage.
- GitHub-hosted: official GitHub cache storage, ignoring S3 inputs.

Required S3 inputs are `s3-endpoint`, `s3-bucket`, `s3-region`, `s3-access-key` and `s3-secret-key`. There is no
implicit signing region; supply the region expected by your endpoint. `s3-session-token` is optional for temporary
credentials. `s3-force-path-style` defaults to `true`; set `false` for virtual-host addressing. Partial self-hosted
configuration fails before restoration, naming missing fields without printing credentials. Selection never reads
ambient AWS or RunsOn configuration. Cache credentials are scoped to S3 transport steps; deployment credentials
elsewhere in the job are preserved.

An S3 miss or transport failure never switches to GitHub storage. Restore failures are nonfatal, with an S3 warning;
installation/build steps continue normally. Save failures warn without changing a successful job to failure. These
exceptions apply only to optional archive transport; invalid inputs and project failures still fail.

The key format is:

```text
<owner>-<repo>-<tool>-<os>-<arch>-v1-<repository hash>-<environment hash>-<ref hash>-<build>-<content hash>
```

Hash fields use the first 24 hexadecimal characters of SHA-256. Owner and repository names come from the consumer's
`github.repository`, normalized to lowercase. For example, `bingamon-lab-lz-cli-cargo-Linux-X64-v1-...` identifies Cargo
downloads for that repository. The repository hash keeps ambiguous hyphen-separated names distinct. `v1` is the
cache-key schema version, not the repository release version. The environment hash covers the environment type, selected
flake shell and environment lockfile fingerprint. The previous `tars-v1` keys are not restored; the first run with the
new format populates new caches. The ref hash identifies the branch, tag or PR merge ref. The build field is `downloads`
for dependency archives, or a compatibility hash for compiled Cargo output. The content hash covers tool-specific
manifest paths and contents; Trivy adds a UTC date suffix for daily refreshes.

Keys include a schema version, repository identity, OS, architecture, environment lock fingerprint, environment type and
selected flake shell. Per-tool fingerprints avoid hashing unrelated language lockfiles. Compiled keys additionally
include toolchain/config files, target/variant discriminators and visible Rust flags. Changing compiler compatibility
prevents incompatible target-cache fallback.

Each PR writes its own PR-number/merge-ref scope, including fork PRs on GitHub storage. Other branches and tags write
their own ref scopes. Restores search the current scope first, then the default branch from GitHub context.
Default-branch jobs never request PR-scoped entries. GitHub additionally enforces its native cache visibility rules;
explicit keys cannot grant broader server permissions. On S3, keys include repository and ref identity, but names are
not authorization. Restrict bucket credentials and ListBucket/GetObject/PutObject/multipart permissions to the intended
`cache/OWNER/REPO/` prefix, or use separate buckets and credentials for stronger isolation. Branch-level permissions are
also needed if untrusted writers share an endpoint.

Exact hits skip saving; changed dependency fingerprints produce new keys. Concurrent jobs sharing a key do not merge
their archives: GitHub accepts the first successful writer, while S3 can replace the same object with the last
successful upload. Only share a dependency key when writers install the same dependency set; serialize a designated
cache-producing job before parallel consumers when their download sets differ. Use cargo-build-variant to separate
different compiled outputs. Successful PR jobs can save and reuse their own caches without waiting for merge. Failures
and cancellations do not save dependency archives.

Configure bucket lifecycle in infrastructure: current-object age expiry, incomplete multipart cleanup and, for versioned
buckets, noncurrent-version expiry. Reading an object does not renew ordinary object-age expiry. These actions never
provision or modify a bucket or the lab's separately managed nix-cache service.

Disable duplicate caches in consumers: setup-python's `cache`, setup-node's `cache`, setup-uv's `enable-cache`, Trivy
action caching, old cache-cargo/cache-bun/cache-trivy calls, and other dependency-archive wrappers. This action cannot
intercept hidden caches inside third-party setup actions.

## Optional Cachix

```yaml
- uses: tars-cloud/actions/composite/setup-cache@v1
  with:
    cachix-name: ${{ vars.CACHIX_CACHE_NAME }}
    cachix-token: ${{ secrets.CACHIX_TOKEN }}
```

- `cachix-name`: empty by default; no name disables Cachix even if a token is supplied.
- `cachix-token`: empty by default; a named cache without a token is read-only, while a token enables writes except on
  fork PRs.

Cachix selection is independent of detected languages and archive backend. The bootstrap step reuses an installed CLI or
installs with `nix profile add nixpkgs#cachix` when missing, then passes its resolved binary to the pinned integration.
It adds the named substituter while preserving other Nix substituters, and uses its daemon/post-job integration for
pushes. Private caches require appropriate authentication; token-free fork reads are intended for public caches. No name
is inferred from the repository owner. Cachix's upstream daemon mode may fall back to scanning newly created store paths
when the runner lacks daemon support or trusted-user permissions; write-enabled runners must be dedicated to the
intended trust domain. Cachix push behaviour is separate from success-only dependency archives.

## Outputs

- `tools`: JSON array of selected cache names (`cargo`, `cargo-target`, `uv`, `pip`, `bun`, `trivy`).
- `backend`: `github` or `s3`.
- `cachix-mode`: `disabled`, `read` or `write`.
- `reasons`: JSON array of detection/override explanations.
- `cargo-hit`, `cargo-target-hit`, `uv-hit`, `pip-hit`, `bun-hit`, `trivy-hit`: `true` only for an exact key match;
  `false` for a miss/fallback, empty for an inactive tool.
