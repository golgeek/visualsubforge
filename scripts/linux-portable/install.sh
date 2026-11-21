#!/bin/sh
# All supported distributions provide GNU coreutils.
set -eu
src=$(dirname "$(readlink -f "$0")")
mode=install
if [ "${1:-}" = --uninstall ]; then mode=uninstall; shift; fi
case "$#:${1:-}" in
    0:) prefix="$HOME/.local"; app="$prefix/opt/visualsubforge" ;;
    1:--system)
        if [ "$(id -u)" != 0 ]; then
            if [ "$mode" = uninstall ]; then exec sudo -- "$src/uninstall.sh" --system; fi
            exec sudo -- "$src/install.sh" --system
        fi
        prefix=/usr/local; app=/opt/visualsubforge ;;
    *) echo "Usage: $mode.sh [--system]" >&2; exit 2 ;;
esac
die() { printf '%s\n' "$*" >&2; exit 1; }
files='bin/visualsubforge bin/vsf share/applications/visualsubforge.desktop share/icons/hicolor/512x512/apps/visualsubforge.png'
for file in $files; do
    link="$prefix/$file"
    if [ -e "$link" ] || [ -L "$link" ]; then
        if [ ! -L "$link" ] || [ "$(readlink "$link")" != "$app/$file" ]; then
            die "Unrelated entry at $link"
        fi
    fi
done

# Verify ownership before touching the destination. An altered tree or one
# containing extra files is left for its owner to inspect.
verify_tree() (
    cd "$1"
    # Reject an external, repeated or ambiguous path before consuming the list.
    awk 'length($0) < 67 || substr($0, 65, 2) != "  " { exit 1 }
         { p = substr($0, 67) }
         p ~ /^\// || p ~ /(^|\/)\.\.?($|\/)/ || p ~ /\/\// || seen[p]++ { exit 1 }' FILES.sha256 &&
    [ -f FILES.sha256 ] && [ ! -L FILES.sha256 ] &&
        [ -f share/doc/visualsubforge/native-runtime.json ] &&
        [ -z "$(find . ! -type f ! -type d -print -quit)" ] &&
        sha256sum --strict --quiet -c FILES.sha256 &&
        [ "$(find . -type f ! -path ./FILES.sha256 -exec printf 'x%.0s' {} + | wc -c)" -eq "$(wc -l < FILES.sha256)" ]
)
if [ "$mode" = uninstall ]; then
    if [ -e "$app" ] || [ -L "$app" ]; then
        if [ -L "$app" ] || ! verify_tree "$app"; then die "Unrecognized or modified installation at $app"; fi
        cmp -s "$src/FILES.sha256" "$app/FILES.sha256" || die "Use the uninstall.sh installed in $app"
    fi
    for file in $files; do
        if [ -L "$prefix/$file" ]; then rm -- "$prefix/$file"; fi
    done
    if [ -d "$app" ]; then
        # Remove only manifest members. Preserve directories containing user files.
        (cd "$app" && cut -c67- FILES.sha256 | while IFS= read -r file; do rm -- "$file"; done)
        rm -- "$app/FILES.sha256"
        find "$app" -depth -type d -empty -delete
    fi
    exit 0
fi

verify_tree "$src" || die "Package checksum or file list differs"
if [ -e "$app" ] || [ -L "$app" ]; then
    if [ -L "$app" ] || ! verify_tree "$app" || ! cmp -s "$src/FILES.sha256" "$app/FILES.sha256"; then
        die "Refusing to overwrite $app; uninstall the existing version first"
    fi
else
    parent=$(dirname "$app")
    mkdir -p "$parent"
    stage=$(mktemp -d "$parent/.visualsubforge.XXXXXXXX")
    trap 'rm -rf -- "$stage"' EXIT
    cp -a -- "$src/." "$stage/"
    chmod 755 "$stage"
    mv -Tn -- "$stage" "$app"
    [ ! -d "$stage" ] || die "Another installation appeared at $app"
    trap - EXIT
fi
for file in $files; do
    mkdir -p "$(dirname "$prefix/$file")"
    if [ ! -L "$prefix/$file" ]; then ln -sT -- "$app/$file" "$prefix/$file"; fi
done
