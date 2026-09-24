#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/platform.sh"
if ! command -v cachix >/dev/null 2>&1; then
	nix profile add nixpkgs#cachix
	export PATH="$HOME/.nix-profile/bin:$PATH"
	echo "$HOME/.nix-profile/bin" >>"$GITHUB_PATH"
fi
cachix --version
printf 'binary=%s\n' "$(command -v cachix)" >>"$GITHUB_OUTPUT"
