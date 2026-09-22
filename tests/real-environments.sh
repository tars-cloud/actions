#!/usr/bin/env bash
set -euo pipefail
root=$(realpath .)
scratch=$(mktemp -d "$root/.tars/scratch/real-environments-XXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
export RUNNER_OS=Linux RUNNER_ENVIRONMENT=self-hosted
case $(uname -m) in
x86_64) export RUNNER_ARCH=X64 ;;
aarch64) export RUNNER_ARCH=ARM64 ;;
*) exit 1 ;;
esac
export GITHUB_WORKSPACE="$root" GITHUB_OUTPUT="$scratch/output"
export PROJECT_DIRECTORY="$root" ENVIRONMENT_TYPE=devenv
bash "$root/internal/trivy.sh"

mkdir -p "$scratch/ambient" "$scratch/direct"
printf '#!/bin/sh\nexit 99\n' >"$scratch/ambient/trivy"
chmod +x "$scratch/ambient/trivy"
export PATH="$scratch/ambient:$PATH"
cp devenv.yaml devenv.lock "$scratch/direct/"
printf '{ pkgs, ... }: { packages = [ pkgs.bash ]; cachix.enable = false; }\n' >"$scratch/direct/devenv.nix"
export PROJECT_DIRECTORY="$scratch/direct"
if bash "$root/internal/trivy.sh" >"$scratch/missing.log" 2>&1; then
	echo 'Missing direct Trivy was incorrectly accepted.'
	exit 1
fi
grep -q 'Add pkgs.trivy' "$scratch/missing.log"
echo 'Direct environment rejected ambient Trivy.'
if [[ ${1:-all} == direct ]]; then
	exit 0
fi

export PROJECT_DIRECTORY="$root/tests/fixtures/flakes" ENVIRONMENT_TYPE=flakes
for name in default named; do
	export FLAKE_SHELL="path:$PROJECT_DIRECTORY#$name"
	bash "$root/internal/devenv.sh"
	bash "$root/internal/trivy.sh"
done
export FLAKE_SHELL="path:$PROJECT_DIRECTORY#missing-trivy"
if bash "$root/internal/trivy.sh" >"$scratch/missing.log" 2>&1; then
	echo 'Missing flake Trivy was incorrectly accepted.'
	exit 1
fi
grep -q 'Add pkgs.trivy' "$scratch/missing.log"
echo 'Flake environment rejected ambient Trivy.'
