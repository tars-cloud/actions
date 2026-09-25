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
devenv_version=''
if [[ ${ENVIRONMENT_TYPE:-devenv} == devenv ]]; then
	if [[ -n ${DEVENV_INSTALLABLE:-} ]]; then
		installation=$(mktemp -d "${RUNNER_TEMP:?}/tars-devenv.XXXXXX")
		nix build --out-link "$installation/result" -- "$DEVENV_INSTALLABLE"
		export PATH="$installation/result/bin:$PATH"
		version_output=$("$installation/result/bin/devenv" --no-tui --version)
	elif ! command -v devenv >/dev/null 2>&1; then
		nix profile add nixpkgs#devenv
		export PATH="$HOME/.nix-profile/bin:$PATH"
		echo "$HOME/.nix-profile/bin" >>"$GITHUB_PATH"
	fi
	if [[ -z ${DEVENV_INSTALLABLE:-} ]]; then version_output=$(devenv --no-tui --version); fi
	if [[ $version_output =~ ^devenv[[:space:]]+([0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?)($|[[:space:]]) ]]; then
		devenv_version=${BASH_REMATCH[1]}
	else
		echo '::error::Could not parse the selected devenv CLI version.'
		exit 1
	fi
	printf '%s\n' "$version_output"
	if [[ -n ${DEVENV_INSTALLABLE:-} ]]; then printf '%s\n' "$installation/result/bin" >>"$GITHUB_PATH"; fi
fi
probe_system
if [[ ${WARMUP:-true} == true ]] && ! foreign_system; then
	dispatch -c ':'
fi
resolved_system=${ENVIRONMENT_SYSTEM:-}
if [[ -z $resolved_system ]]; then
	case $RUNNER_ARCH in
	X64) resolved_system=x86_64-linux ;;
	ARM64) resolved_system=aarch64-linux ;;
	esac
fi
printf 'devenv-version=%s\nsystem=%s\n' "$devenv_version" "$resolved_system" >>"$GITHUB_OUTPUT"
