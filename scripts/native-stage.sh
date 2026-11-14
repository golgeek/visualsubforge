#!/usr/bin/env bash
# Native SDK prefixes must remain at their build location. Keep failed attempts
# beside them for diagnosis; a completion marker is written only on success.
set -euo pipefail
output=$1
shift
if [[ -f "$output/.complete" ]]; then exit 0; fi
if [[ -e "$output" ]]; then mv "$output" "$output.failed-$(date +%s)-$$"; fi
mkdir -p "$(dirname "$output")"
command=()
for argument in "$@"; do command+=("${argument//\{output\}/$output}"); done
printf 'Building %s\n' "$output"
"${command[@]}"
test -d "$output"
touch "$output/.complete"
