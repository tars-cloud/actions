#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/platform.sh"
if [[ $RUNNER_ENVIRONMENT == self-hosted ]]; then
	echo '::notice::Skipping disk cleanup on a persistent self-hosted runner.'
	echo 'cleaned=false' >>"$GITHUB_OUTPUT"
	exit 0
fi
paths=()
for category in ANDROID DOTNET HASKELL BOOST SWIFT; do
	value=${!category:-false}
	case $value in
	true | false) ;;
	*)
		echo "::error::$category must be true or false."
		exit 1
		;;
	esac
	[[ $value == true ]] || continue
	case $category in
	ANDROID) paths+=(/usr/local/lib/android) ;;
	DOTNET) paths+=(/usr/share/dotnet) ;;
	HASKELL) paths+=(/opt/ghc /usr/local/.ghcup) ;;
	BOOST) paths+=(/usr/local/share/boost) ;;
	SWIFT) paths+=(/usr/share/swift) ;;
	esac
done
echo 'Disk usage before cleanup:'
df -h /
for path in "${paths[@]}"; do
	printf 'Removing hosted SDK: %s\n' "$path"
	sudo rm -rf -- "$path"
done
echo 'Disk usage after cleanup:'
df -h /
echo 'cleaned=true' >>"$GITHUB_OUTPUT"
