#!/usr/bin/env bash
set -euo pipefail
source "${BASH_SOURCE[0]%/*}/debug.sh"
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

classify_probe_line() {
	local lower=${1,,}
	case $lower in
	*"exec format error"* | *"cannot execute binary file"* | \
		*"while setting up the build environment: executing "* | \
		*"${ENVIRONMENT_SYSTEM,,}"*"with features"*"is required to build"* | \
		*"qemu-aarch64:"* | *"qemu-x86_64:"* | \
		*"qemu: uncaught target signal"* | *"qemu: fatal:"* | \
		*"rosetta error:"*)
		probe_emulation_failure=true
		;;
	*"http error 429"* | *"http 429"* | *"429 too many requests"* | *"returned error: 429"* | \
		*"http status"*"429"* | *"status code 429"* | \
		*"http error 401"* | *"http error 403"* | *"http error 408"* | \
		*"http error 5"[0-9][0-9]* | \
		*"unable to download"* | *"failed to download"* | *"cannot download"* | *"could not be downloaded"* | \
		*"unable to fetch"* | *"failed to fetch"* | *"cannot fetch"* | \
		*"tarball"*"failed"* | *"failed"*"tarball"* | \
		*"could not resolve host"* | *"couldn't resolve host"* | *"temporary failure in name resolution"* | \
		*"connection timed out"* | *"operation timed out"* | *"timeout was reached"* | \
		*"connection refused"* | *"connection reset"* | \
		*"ssl connect error"* | *"ssl certificate problem"* | *"tls handshake"* | \
		*"transfer closed with"* | *"curl: ("* | *"error sending request for url"* | \
		*"rate limit exceeded"* | *"api rate limit"*)
		probe_network_failure=true
		;;
	esac
}

probe_system() {
	validate_environment
	if ! foreign_system; then return; fi
	local configuration line platforms=''
	configuration=$(nix config show)
	while IFS= read -r line; do
		if [[ $line == 'extra-platforms = '* ]]; then platforms=${line#*= }; fi
	done <<<"$configuration"
	if [[ " $platforms " != *" $ENVIRONMENT_SYSTEM "* ]]; then
		printf '::error::Runner must already support %s through emulation and Nix extra-platforms.\n' "$ENVIRONMENT_SYSTEM"
		return 1
	fi
	local probe_fd probe_pid status=0 shell_arch='' probe_emulation_failure=false probe_network_failure=false
	# shellcheck disable=SC2016
	local probe='if [[ ${MACHTYPE%%-*} != "${1%-linux}" ]]; then printf "tars-system-probe: shell architecture is %s\n" "${MACHTYPE%%-*}" >&2; exit 1; fi'
	exec {probe_fd}< <(dispatch -c "$probe" tars-system-probe "$ENVIRONMENT_SYSTEM" 2>&1)
	probe_pid=$!
	while IFS= read -r line || [[ -n $line ]]; do
		if [[ $line == 'tars-system-probe: shell architecture is '* ]]; then
			shell_arch=${line#'tars-system-probe: shell architecture is '}
		else
			printf '%s\n' "$line" >&2
			classify_probe_line "$line"
		fi
	done <&"$probe_fd"
	exec {probe_fd}<&-
	wait "$probe_pid" || status=$?
	if ((status != 0)); then
		if [[ -n $shell_arch ]]; then
			printf '::error::Cannot execute the %s environment: shell architecture is %s. Check runner emulation configuration.\n' "$ENVIRONMENT_SYSTEM" "$shell_arch"
		elif [[ $probe_emulation_failure == true ]] || ((status == 126)); then
			printf '::error::Cannot execute the %s environment. Check the shell failure above and runner emulation configuration.\n' "$ENVIRONMENT_SYSTEM"
		elif [[ $probe_network_failure == true ]]; then
			printf '::error::Failed to prepare the %s environment due to a network or download error. Check the failure above.\n' "$ENVIRONMENT_SYSTEM"
		else
			printf '::error::Failed to prepare the %s environment. Check the evaluation or shell failure above.\n' "$ENVIRONMENT_SYSTEM"
		fi
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
	debug_environment
	validate_environment
	export CI="${CI:-true}"
	export SECRETSPEC_PROVIDER=env
	if [[ -n ${CONSUMER_SECRETSPEC_PROFILE:-} ]]; then export SECRETSPEC_PROFILE="$CONSUMER_SECRETSPEC_PROFILE"; fi
	export SECRETSPEC_REASON="${SECRETSPEC_REASON:-github-actions}"
	configure_dependency_access
	local command
	if [[ ${ENVIRONMENT_TYPE:-devenv} == devenv ]]; then
		command=(devenv --no-tui)
		if [[ -n ${ENVIRONMENT_SYSTEM:-} ]]; then command+=(--system "$ENVIRONMENT_SYSTEM"); fi
		if [[ $TARS_DEBUG == true ]]; then
			command+=(shell --verbose --)
		else
			command+=(shell --quiet --)
		fi
	else
		command=(nix develop --impure)
		if [[ $TARS_DEBUG == true ]]; then command+=(--verbose --print-build-logs); fi
		if [[ -n ${ENVIRONMENT_SYSTEM:-} ]]; then command+=(--system "$ENVIRONMENT_SYSTEM"); fi
		command+=("${FLAKE_SHELL:-.#default}" --command)
	fi
	(
		# Only the selected environment may supply a declared package profile.
		unset DEVENV_PROFILE
		if [[ $TARS_DEBUG == true ]]; then
			# Pass consumer commands as arguments, never as diagnostic text.
			# shellcheck disable=SC2016
			set -- -c 'export TARS_DEBUG=true; printf "[tars:debug] consumer shell entered\n" >&2; exec bash --noprofile --norc -euo pipefail "$@"' tars-debug "$@"
		fi
		"${command[@]}" bash --noprofile --norc -euo pipefail "$@"
	)
}
