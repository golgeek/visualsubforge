#!/usr/bin/env bash
# Package into a new directory; never overwrite the last working package on a
# failed assembler command. Renames also work without Windows symlink privilege.
set -euo pipefail
output=$1
shift
pending="$output.pending-$$"
backup="$output.previous-$$"
mkdir -p "$(dirname "$output")"
command=()
for argument in "$@"; do command+=("${argument//\{output\}/$pending}"); done
"${command[@]}"
test -d "$pending"
if [[ -e "$output" ]]; then mv "$output" "$backup"; fi
if ! mv "$pending" "$output"; then
    if [[ -e "$backup" ]]; then mv "$backup" "$output"; fi
    exit 1
fi
# Keep the previous successful package too. It may be running on Windows.
printf 'Built native package: %s\n' "$output"
