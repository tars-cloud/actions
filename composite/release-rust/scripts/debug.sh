#!/usr/bin/env bash

case ${TARS_DEBUG_INPUT:-${TARS_DEBUG:-false}} in
true | false) ;;
*)
	printf '::error::debug must be true or false.\n' >&2
	exit 1
	;;
esac
if [[ ${TARS_DEBUG_INPUT:-false} == true || ${TARS_DEBUG:-false} == true || ${RUNNER_DEBUG:-} == 1 ]]; then
	export TARS_DEBUG=true
else
	export TARS_DEBUG=false
fi

debug_log() {
	if [[ $TARS_DEBUG == true ]]; then printf '[tars:debug] %s\n' "$*" >&2; fi
}

debug_finish() {
	local status=$1
	debug_log "$debug_phase finished: exit=$status elapsed=$((SECONDS - debug_started))s"
}

debug_start() {
	if [[ $TARS_DEBUG != true ]]; then return; fi
	debug_phase=$1
	debug_started=$SECONDS
	debug_log "$debug_phase started"
	trap 'debug_finish "$?"' EXIT
}

debug_command() (
	debug_start "$1"
	shift
	"$@"
)

debug_environment() {
	if [[ $TARS_DEBUG != true ]]; then return; fi
	local system=${ENVIRONMENT_SYSTEM:-}
	if [[ -z $system ]]; then
		case ${RUNNER_ARCH:-} in
		X64) system=x86_64-linux ;;
		ARM64) system=aarch64-linux ;;
		*) system=runner-native ;;
		esac
	fi
	printf '[tars:debug] environment=%q runner_arch=%q selected_system=%q flake_shell=%q\n' \
		"${ENVIRONMENT_TYPE:-devenv}" "${RUNNER_ARCH:-unknown}" \
		"$system" "${FLAKE_SHELL:-.#default}" >&2
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
	debug_start "${1:-action}"
	debug_environment
fi
