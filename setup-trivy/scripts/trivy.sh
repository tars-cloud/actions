#!/usr/bin/env bash
set -euo pipefail
script_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$script_directory/environment.sh"
export TARS_PATH_BOUNDARY="${RUNNER_TEMP:-/tmp}/tars-path-boundary-$$-$RANDOM"
export PATH="$TARS_PATH_BOUNDARY:$PATH"
dispatch "$script_directory/trivy-in-environment.sh"
