#!/usr/bin/env bash
# Package into a new directory; never overwrite the last working package on a
# failed assembler command. Renames also work without Windows symlink privilege.
set -euo pipefail
output=$1
shift
pending="$output.pending-$$"
backup=""
mkdir -p "$(dirname "$output")"
command=()
for argument in "$@"; do command+=("${argument//\{output\}/$pending}"); done
"${command[@]}"
test -d "$pending"
if [[ -e "$output" ]]; then
    backup="$output.previous-$$"
    # A long-lived backup can outlast its publisher's PID.
    while [[ -e "$backup" || -L "$backup" ]]; do backup+=-1; done
    mv "$output" "$backup"
fi
if ! mv "$pending" "$output"; then
    if [[ -n "$backup" ]]; then mv "$backup" "$output"; fi
    exit 1
fi
# The package just moved aside is the immediate predecessor. If the active
# package was manually removed, keep the most recently modified backup.
previous=("$output".previous-*)
if [[ -z "$backup" ]]; then
    for candidate in "${previous[@]}"; do
        if [[ -d "$candidate" && ( -z "$backup" || "$candidate" -nt "$backup" ) ]]; then
            backup=$candidate
        fi
    done
fi
for candidate in "${previous[@]}"; do
    if [[ -d "$candidate" && "$candidate" != "$backup" ]]; then
        rm -rf -- "$candidate" || printf 'Could not remove old package: %s\n' "$candidate" >&2
    fi
done
printf 'Built native package: %s\n' "$output"
