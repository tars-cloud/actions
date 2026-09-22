#!/usr/bin/env bash
set -euo pipefail
root=.tars/scratch/ci-cache
mkdir -p "$root"
cp devenv.nix devenv.yaml devenv.lock "$root/"
# Unique content makes this workflow run cold without adding a production key input.
printf '[package]\nname = "cache-fixture"\nversion = "0.0.0"\n# %s-%s\n' "$GITHUB_RUN_ID" "$GITHUB_RUN_ATTEMPT" >"$root/Cargo.toml"
printf '# %s-%s\n' "$GITHUB_RUN_ID" "$GITHUB_RUN_ATTEMPT" >"$root/uv.lock"
printf '# %s-%s\n' "$GITHUB_RUN_ID" "$GITHUB_RUN_ATTEMPT" >"$root/requirements.txt"
printf '%s-%s\n' "$GITHUB_RUN_ID" "$GITHUB_RUN_ATTEMPT" >"$root/bun.lock"
printf '# %s-%s\n' "$GITHUB_RUN_ID" "$GITHUB_RUN_ATTEMPT" >"$root/trivy.yaml"
