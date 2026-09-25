#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/platform.sh"

validate_environment() {
	case ${ENVIRONMENT_SYSTEM:-} in
	'' | x86_64-linux | aarch64-linux) ;;
	*)
		echo '::error::system must be x86_64-linux or aarch64-linux, or empty for the runner system.'
		return 1
		;;
	esac
	local directory=${PROJECT_DIRECTORY:-.}
	if [[ $directory != /* ]]; then
		directory="$GITHUB_WORKSPACE/$directory"
	fi
	cd -- "$directory"
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

foreign_system() {
	[[ -n ${ENVIRONMENT_SYSTEM:-} &&
		! (${RUNNER_ARCH:-} == X64 && $ENVIRONMENT_SYSTEM == x86_64-linux) &&
		! (${RUNNER_ARCH:-} == ARM64 && $ENVIRONMENT_SYSTEM == aarch64-linux) ]]
}

probe_system() {
	validate_environment
	if ! foreign_system; then return; fi
	local configuration line platforms=''
	configuration=$(nix show-config)
	while IFS= read -r line; do
		if [[ $line == 'extra-platforms = '* ]]; then platforms=${line#*= }; fi
	done <<<"$configuration"
	if [[ " $platforms " != *" $ENVIRONMENT_SYSTEM "* ]]; then
		printf '::error::Runner must already support %s through emulation and Nix extra-platforms.\n' "$ENVIRONMENT_SYSTEM"
		return 1
	fi
	local status=0
	# shellcheck disable=SC2016
	dispatch -c 'test "${MACHTYPE%%-*}" = "${1%-linux}"' tars-system-probe "$ENVIRONMENT_SYSTEM" || status=$?
	if ((status != 0)); then
		printf '::error::Cannot execute the %s environment. Check the shell failure above and runner emulation configuration.\n' "$ENVIRONMENT_SYSTEM"
		return "$status"
	fi
}

configure_dependency_access() {
	if [[ -n ${DEPENDENCY_GITHUB_TOKEN:-} ]]; then
		export NIX_CONFIG="${NIX_CONFIG:-}
extra-access-tokens = github.com=$DEPENDENCY_GITHUB_TOKEN"
	fi
}

dispatch() {
	validate_environment
	export CI="${CI:-true}"
	export SECRETSPEC_PROVIDER=env
	export SECRETSPEC_REASON="${SECRETSPEC_REASON:-github-actions}"
	configure_dependency_access
	local command
	if [[ ${ENVIRONMENT_TYPE:-devenv} == devenv ]]; then
		command=(devenv --no-tui)
		if [[ -n ${ENVIRONMENT_SYSTEM:-} ]]; then command+=(--system "$ENVIRONMENT_SYSTEM"); fi
		command+=(shell --quiet --)
	else
		command=(nix develop --impure)
		if [[ -n ${ENVIRONMENT_SYSTEM:-} ]]; then command+=(--system "$ENVIRONMENT_SYSTEM"); fi
		command+=("${FLAKE_SHELL:-.#default}" --command)
	fi
	"${command[@]}" bash --noprofile --norc -euo pipefail "$@"
}
