#!/usr/bin/env bash
set -euo pipefail

# Both shell entry points prepend project tools while retaining the runner PATH.
# The boundary excludes inherited tools, even when the runner uses Nix itself.
case $PATH in
*":$TARS_PATH_BOUNDARY:"*) tool_path=${PATH%%":$TARS_PATH_BOUNDARY:"*} ;;
*) tool_path='' ;;
esac
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
