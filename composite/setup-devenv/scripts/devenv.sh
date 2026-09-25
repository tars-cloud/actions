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
if [[ -n ${DEVENV_INSTALLABLE:-} &&
	(${ENVIRONMENT_TYPE:-devenv} != devenv || $DEVENV_INSTALLABLE == -* || $DEVENV_INSTALLABLE == *$'\n'*) ]]; then
	echo '::error::devenv-installable requires direct mode and a Nix installable, not command options.'
	exit 1
fi
configure_dependency_access
if [[ ${ENVIRONMENT_TYPE:-devenv} == devenv ]]; then
	if [[ -n ${DEVENV_INSTALLABLE:-} ]]; then
		installation=$(mktemp -d "${RUNNER_TEMP:?}/tars-devenv.XXXXXX")
		nix build --out-link "$installation/result" -- "$DEVENV_INSTALLABLE"
		export PATH="$installation/result/bin:$PATH"
		"$installation/result/bin/devenv" --no-tui --version
		printf '%s\n' "$installation/result/bin" >>"$GITHUB_PATH"
	elif ! command -v devenv >/dev/null 2>&1; then
		nix profile add nixpkgs#devenv
		export PATH="$HOME/.nix-profile/bin:$PATH"
		echo "$HOME/.nix-profile/bin" >>"$GITHUB_PATH"
	fi
	if [[ -z ${DEVENV_INSTALLABLE:-} ]]; then devenv --no-tui --version; fi
fi
probe_system
if [[ ${WARMUP:-true} == true ]] && ! foreign_system; then
	dispatch -c ':'
fi
