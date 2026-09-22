#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/environment.sh"
validate_environment
case ${WARMUP:-true} in
true | false) ;;
*)
	echo '::error::warmup must be true or false.'
	exit 1
	;;
esac
if [[ ${ENVIRONMENT_TYPE:-devenv} == devenv ]]; then
	if ! command -v devenv >/dev/null 2>&1; then
		nix profile add nixpkgs#devenv
		export PATH="$HOME/.nix-profile/bin:$PATH"
		echo "$HOME/.nix-profile/bin" >>"$GITHUB_PATH"
	fi
	devenv --no-tui --version
fi
if [[ ${WARMUP:-true} == true ]]; then
	dispatch -c ':'
fi
