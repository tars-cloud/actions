#!/usr/bin/env bash
set -euo pipefail
source "${BASH_SOURCE[0]%/*}/debug.sh"
debug_start setup-trivy
script_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$script_directory/environment.sh"
probe_system
export TARS_PATH_BOUNDARY="${RUNNER_TEMP:-/tmp}/tars-path-boundary-$$-$RANDOM"
export PATH="$TARS_PATH_BOUNDARY:$PATH"
dispatch "$script_directory/trivy-in-environment.sh"
