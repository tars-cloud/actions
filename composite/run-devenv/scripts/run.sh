#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/environment.sh"
case ${DEVENV_MODE:-shell} in
shell) ;;
test)
	if [[ ${ENVIRONMENT_TYPE:-devenv} != devenv ]]; then
		echo '::error::test mode requires a direct devenv environment.'
		exit 1
	fi
	validate_system
	dispatch
	exit
	;;
*)
	echo '::error::mode must be shell or test.'
	exit 1
	;;
esac
if [[ -z ${DEVENV_RUN:-} ]]; then
	echo '::error::run must contain Bash commands.'
	exit 1
fi
validate_system
dispatch -c "$DEVENV_RUN"
