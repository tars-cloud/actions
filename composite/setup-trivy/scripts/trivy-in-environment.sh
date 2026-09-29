#!/usr/bin/env bash
set -euo pipefail

# Clean devenv shells discard the runner PATH; their profile lists declared tools.
if [[ -n ${TARS_PATH_BOUNDARY:-} && $PATH == *":$TARS_PATH_BOUNDARY:"* ]]; then
	tool_path=${PATH%%":$TARS_PATH_BOUNDARY:"*}
else
	tool_path=${DEVENV_PROFILE:+$DEVENV_PROFILE/bin}
fi
if [[ -z $tool_path ]]; then
	echo '::error::Add pkgs.trivy to the selected devenv module packages and update its environment lockfile.'
	exit 1
fi
if ! trivy_binary=$(PATH="$tool_path" command -v trivy); then
	echo '::error::Add pkgs.trivy to the selected devenv module packages and update its environment lockfile.'
	exit 1
fi
version_text=$("$trivy_binary" --version)
printf '%s\n' "$version_text"
version=${version_text%%$'\n'*}
version=${version#Version: }
printf 'version=%s\n' "$version" >>"$GITHUB_OUTPUT"
