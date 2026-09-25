#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/environment.sh"
if [[ -z ${DEVENV_RUN:-} ]]; then
	echo '::error::run must contain Bash commands.'
	exit 1
fi
probe_system
dispatch -c "$DEVENV_RUN"
