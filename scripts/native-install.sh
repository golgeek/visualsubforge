#!/usr/bin/env bash
set -euo pipefail
platform=$1
package=$2
case "$platform" in
    macos)
        source="$package/VisualSubForge.app"
        destination="${INSTALL_DIR:-$HOME/Applications}/VisualSubForge.app"
        test -x "$source/Contents/MacOS/visualsubforge"
        bash scripts/native-publish.sh "$destination" ditto "$source" '{output}'
        printf 'Installed: %s\n' "$destination"
        ;;
    linux)
        # apt resolves the package's declared GTK/WebKit/portal dependencies.
        # Elevation happens only for this optional install, never the build.
        artifacts=("$package"/*.deb)
        [[ ${#artifacts[@]} == 1 && -f "${artifacts[0]}" ]]
        sudo apt-get install --reinstall "$(cd "$package" && pwd)/$(basename "${artifacts[0]}")"
        ;;
    windows)
        source="$package/VisualSubForge"
        base=${INSTALL_DIR:-$(cygpath -u "$LOCALAPPDATA")/Programs}
        destination="$base/VisualSubForge"
        test -f "$source/visualsubforge.exe"
        bash scripts/native-publish.sh "$destination" cp -R "$source" '{output}'
        VSF_INSTALLED_APP="$(cygpath -w "$destination/visualsubforge.exe")"
        export VSF_INSTALLED_APP
        # PowerShell expands these expressions, not Bash.
        # shellcheck disable=SC2016
        powershell.exe -NoProfile -NonInteractive -Command '$ErrorActionPreference="Stop"; $shell=New-Object -ComObject WScript.Shell; $shortcut=$shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath("Programs")) "VisualSubForge.lnk")); $shortcut.TargetPath=$env:VSF_INSTALLED_APP; $shortcut.WorkingDirectory=Split-Path $env:VSF_INSTALLED_APP; $shortcut.Save()'
        printf 'Installed: %s (Start menu shortcut created)\n' "$destination"
        ;;
    *) printf 'Unknown install platform: %s\n' "$platform" >&2; exit 1;;
esac
