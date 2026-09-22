#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/platform.sh"

if command -v nix >/dev/null 2>&1; then
	nix --version
	echo 'install=false' >>"$GITHUB_OUTPUT"
elif [[ $RUNNER_ENVIRONMENT == self-hosted ]]; then
	echo '::error::Install Nix on this self-hosted runner and expose nix on PATH before using this action.'
	exit 1
else
	echo 'install=true' >>"$GITHUB_OUTPUT"
fi
