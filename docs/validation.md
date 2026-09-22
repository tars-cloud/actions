# Validation evidence

Local validation passed on Linux X64 on 2026-09-22. The first hosted run verified same-revision setup-nix composition
and actual nested cache saves, and exposed two lifecycle bugs now covered by regressions: relative working-directory
re-entry and composite post-output evaluation.
[CI run 35704182195](https://github.com/tars-cloud/actions/actions/runs/35704182195) passed all eleven jobs on the
corrected implementation. This includes hosted AMD64/ARM64 direct and default/named flake environments,
cold/post-save/warm cache lifecycles, and the `enterprise/tars-cloud` self-hosted job on
`github-runner-04-enterprise-mahdtech`. The final follow-up adds Cargo configuration-file compatibility coverage,
explicit credential masking and the mocked Cachix lifecycle test to hosted CI.

- All 21 Node unit/fixture tests passed.
- Metadata checks and all eight configured prek hooks passed after the workflow styling and linter configuration update.
- `devenv --no-tui test` executed the actions:test task successfully.
- Real direct, default-flake and named-flake environments reported Trivy 0.74.0.
- Real direct and flake environments without Trivy rejected an ambient runner binary.
- The pinned RunsOn restore/save implementations contacted a disposable local S3 endpoint with fake credentials and
  handled HTTP 403 denials nonfatally; save emitted a warning.
- The pinned Cachix main/post implementation passed mocked read-only, write/daemon-drain and fork/no-token lifecycle
  checks.
- Live lab S3 operation and live Cachix reads/writes remain unverified.

The local S3 check downloads the reviewed upstream bundles, then runs against localhost only:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=transport-test \
  devenv --no-tui shell --quiet -- node tests/transport.cjs
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=cachix-test \
  devenv --no-tui shell --quiet -- node tests/cachix.cjs
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=environment-test \
  devenv --no-tui shell --quiet -- bash tests/real-environments.sh
```

## Test scope

- Node fixture tests cover detection, exclusions, explicit selection, paths, compatibility keys, PR/default scopes, fork
  policy and backend selection.
- Shell tests use fake executables for installation, shell dispatch and cleanup; they never delete actual runner SDKs.
- Real local checks validate Trivy from this repository's declared devenv package and the flake fixture.
- Metadata checks validate public action composition, input wiring, immutable upstream pins and CI coverage.
- Static checks run action-validator, actionlint, markdownlint, nixfmt, Prettier, ShellCheck, shfmt and yamllint through
  devenv.
- `tests/static.sh` uses the same prek hooks and configuration as `devenv test`.
- Hosted CI cold and warm jobs test real nested composite post-job saves and subsequent restoration.
- Hosted direct and flake jobs test both native runner architectures, default/named shells and missing environment Trivy
  with an ambient trap.

## Upstream review

- actions/cache v6.1.0: `55cc8345863c7cc4c66a329aec7e433d2d1c52a9`; Node 24, success-only post-save.
- runs-on/cache v5.0.7: `88d90644011a3a9957fd141a106f5a94f9794203`; explicit S3 environment selection,
  repository/version object prefix, success-only post-save, warning-only save exceptions.
- cachix/install-nix-action v31.11.1: `13d8dd58da0234aa297dedd986986ccb8e7f3e24`; used only after hosted missing-Nix
  detection.
- cachix/cachix-action v17: `38b082610b782e7e93e209c35fd730d399dee866`; existing binary detection, explicit install
  command, named substituter and post-job push integration.
- actions/checkout v7.0.1: `3d3c42e5aac5ba805825da76410c181273ba90b1`; CI checkout with credential persistence disabled.

Upstream source review is evidence about implementation contracts, not proof of live lab service behaviour. Lab S3
credentials, bucket permissions, lifecycle configuration and Cachix writes require separate live verification. No
releases or consumer migrations are performed here.
