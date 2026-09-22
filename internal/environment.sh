#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/platform.sh"

validate_environment() {
	cd -- "${PROJECT_DIRECTORY:-$GITHUB_WORKSPACE}"
	case ${ENVIRONMENT_TYPE:-devenv} in
	devenv) required=(devenv.nix devenv.yaml devenv.lock) ;;
	flakes) required=(flake.nix flake.lock) ;;
	*)
		echo '::error::type must be devenv or flakes.'
		return 1
		;;
	esac
	for file in "${required[@]}"; do
		if [[ ! -f $file ]]; then
			printf '::error::Selected environment requires %s in working-directory.\n' "$file"
			return 1
		fi
	done
	if [[ ${ENVIRONMENT_TYPE:-devenv} == flakes && ${FLAKE_SHELL:-.#default} == -* ]]; then
		echo '::error::flake-shell must be a flake selector, not a command option.'
		return 1
	fi
}

dispatch() {
	validate_environment
	export CI="${CI:-true}"
	export SECRETSPEC_PROVIDER=env
	export SECRETSPEC_REASON="${SECRETSPEC_REASON:-github-actions}"
	if [[ -n ${DEPENDENCY_GITHUB_TOKEN:-} ]]; then
		export NIX_CONFIG="${NIX_CONFIG:-}
extra-access-tokens = github.com=$DEPENDENCY_GITHUB_TOKEN"
	fi
	local command
	if [[ ${ENVIRONMENT_TYPE:-devenv} == devenv ]]; then
		command=(devenv --no-tui shell --quiet -- bash --noprofile --norc -euo pipefail)
	else
		command=(nix develop --impure "${FLAKE_SHELL:-.#default}" --command bash --noprofile --norc -euo pipefail)
	fi
	"${command[@]}" "$@"
}
