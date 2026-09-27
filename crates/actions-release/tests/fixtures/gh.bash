#!/usr/bin/env bash
set -euo pipefail

method=GET
route=
while (($#)); do
	case "$1" in
	--method)
		method=$2
		shift
		;;
	repos/*) route=$1 ;;
	esac
	shift
done
response="$RELEASE_FIXTURE/responses/${route//\//_}"
if [[ $method == GET ]]; then
	[[ $GH_TOKEN == fixture-read-token ]]
	cat "$response"
	exit
fi
[[ $GH_TOKEN == fixture-write-token ]]
printf '%s %s\n' "$method" "$route" >>"$RELEASE_FIXTURE/writes"
payload="$RELEASE_FIXTURE/payload"
cat >"$payload"
open="$RELEASE_FIXTURE/responses/repos_owner_actions_pulls?state=open&base=trunk&head=owner:release_next&per_page=100"
case "$route" in
repos/owner/actions/pulls | repos/owner/actions/pulls/42)
	head=$(git --git-dir="$RELEASE_REMOTE" rev-parse refs/heads/release/next)
	base=$(git --git-dir="$RELEASE_REMOTE" rev-parse refs/heads/trunk)
	jq --arg head "$head" --arg base "$base" '{number:42, html_url:"https://example.invalid/pull/42", title:.title, body:.body,
      head:{sha:$head, ref:"release/next", repo:{full_name:"owner/actions"}},
      base:{sha:$base, ref:"trunk", repo:{full_name:"owner/actions"}}}' "$payload" >"$RELEASE_FIXTURE/pr"
	jq -s '[.]' "$RELEASE_FIXTURE/pr" >"$open"
	cat "$RELEASE_FIXTURE/pr"
	;;
repos/owner/actions/git/refs*)
	ref=$(jq -r '.ref // empty' "$payload")
	if [[ -z $ref ]]; then ref="refs/${route#repos/owner/actions/git/refs/}"; fi
	sha=$(jq -r .sha "$payload")
	git --git-dir="$RELEASE_REMOTE" update-ref "$ref" "$sha"
	printf '{}\n'
	;;
repos/owner/actions/releases)
	jq -s '[.]' "$payload" >"$RELEASE_FIXTURE/responses/repos_owner_actions_releases?per_page=100"
	printf '{}\n'
	;;
*)
	printf 'Unexpected write: %s %s\n' "$method" "$route" >&2
	exit 1
	;;
esac
