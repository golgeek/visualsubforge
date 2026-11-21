#!/usr/bin/env bash
set -euo pipefail
format=$1
root=$(cat /matrix/root)
export VSF_EVIDENCE=/evidence VSF_FIXTURES=/fixtures VSF_GUI_CHECK=/tests/packaged-gui.py
export VSF_NULL_SINK=ready-default VSF_LIBRARY_AUDIT=1 VSF_GUI_ENVIRONMENT
cp /matrix/*.txt /evidence/
chown preview:preview /evidence
run=(runuser -u preview --)
if [[ $format == tar ]]; then
    "${run[@]}" "$root/install.sh"
    installed=/home/preview/.local/opt/visualsubforge
    find "$installed" -printf '%p %T@\n' | sort > /evidence/first-install.txt
    "${run[@]}" "$root/install.sh"
    find "$installed" -printf '%p %T@\n' | sort | cmp /evidence/first-install.txt -
else
    installed=$root
fi
(cd "$installed" && sha256sum --quiet --strict -c FILES.sha256)
if [[ ${VSF_FULL_TRIAL:-1} == 1 ]]; then
    "${run[@]}" python3 /tests/packaged-cli.py --bundle "$installed" --bin bin --fixtures /fixtures/resync --output /evidence/cli
    rm -rf -- '/evidence/cli/Relocated VSF Preview'
fi
export VSF_GUI_BIN="$installed/bin/visualsubforge.real"
"${run[@]}" xvfb-run -a -s '-screen 0 1280x800x24 -nolisten tcp' dbus-run-session -- bash /tests/linux-gui.sh
if [[ $format == tar ]]; then
    "${run[@]}" "$installed/uninstall.sh"
    test ! -e "$installed"
    for entry in bin/{visualsubforge,vsf} share/applications/visualsubforge.desktop share/icons/hicolor/512x512/apps/visualsubforge.png; do
        test ! -e "/home/preview/.local/$entry" && test ! -L "/home/preview/.local/$entry"
    done
else
    apt-get remove -y visualsubforge
    while IFS= read -r entry; do if [[ -e /$entry || -L /$entry ]]; then echo "$entry"; fi; done < /matrix/deb-entries.txt > /evidence/after.txt
    diff -u /matrix/before.txt /evidence/after.txt > /evidence/removal.diff
fi
