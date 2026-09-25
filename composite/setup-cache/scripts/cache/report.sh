#!/usr/bin/env bash
set -euo pipefail

escape() {
	local value=$1
	value=${value//%/%25}
	value=${value//$'\r'/%0D}
	value=${value//$'\n'/%0A}
	printf '%s' "$value"
}

case ${CACHE_BACKEND:-} in
github | s3) ;;
*)
	echo '::error::Unknown cache backend.'
	exit 1
	;;
esac

level=notice
case ${CACHE_OUTCOME:-} in
failure)
	status=error
	level=warning
	message='Restore reported a failure; inspect the preceding backend error.'
	;;
skipped | cancelled | '')
	status=skipped
	message='Restore did not complete.'
	;;
success)
	case ${CACHE_HIT:-} in
	true)
		status=hit
		message='Exact key restored.'
		;;
	false)
		status=fallback
		message='Compatible fallback restored; the requested exact key was not found.'
		;;
	*)
		status=miss-or-unavailable
		message='No archive restored. A cold miss is normal; upstream warnings may identify a service or download problem.'
		;;
	esac
	;;
*)
	echo '::error::Unknown cache restore outcome.'
	exit 1
	;;
esac

printf 'cache-status=%s\n' "$status" >>"$GITHUB_OUTPUT"
printf '::%s title=Cache restore::%s: %s\n' "$level" "$CACHE_BACKEND" "$message"
printf 'Requested key: %s\n' "$(escape "${CACHE_KEY:-}")"
while IFS= read -r location; do
	if [[ -n $location ]]; then printf 'Cache path: %s\n' "$(escape "$location")"; fi
done <<<"${CACHE_PATHS:-}"
if [[ $status == hit ]]; then
	echo 'Save policy: the exact key already exists, so no new archive is uploaded.'
else
	echo 'Save policy: uploads run in the backend post-job step after a successful job. Missing paths cannot be saved.'
	echo 'Running a version check does not populate a download cache. Existing empty directories may still be archived.'
	echo 'Restore status does not confirm an upload; inspect the backend post-job save log.'
fi
if [[ $CACHE_BACKEND == s3 ]]; then echo 'Storage: S3 only; no GitHub storage fallback.'; fi
