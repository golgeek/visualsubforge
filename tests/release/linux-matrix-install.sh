#!/usr/bin/env bash
# Build an offline-test image from the exact candidate and a pristine distro.
set -euo pipefail
format=$1
# shellcheck source=/dev/null
. /etc/os-release
mkdir -p /matrix/unpacked
export LC_ALL=C
case "$ID" in
    ubuntu|debian)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update
        install=(apt-get install -y --no-install-recommends)
        column=3
        case "$ID:${VERSION_ID:-}" in ubuntu:22.*|debian:12) column=2 ;; esac
        command="sudo apt install"
        tools=(python3 xvfb xauth dbus-x11 pulseaudio pulseaudio-utils mesa-utils libgl1-mesa-dri)
        ;;
    fedora)
        install=(dnf install -y --setopt=install_weak_deps=False)
        column=4
        command="sudo dnf install"
        tools=(python3 xorg-x11-server-Xvfb xorg-x11-xauth dbus-daemon pulseaudio pulseaudio-utils glx-utils mesa-dri-drivers shadow-utils util-linux)
        ;;
    arch)
        pacman -Sy --noconfirm
        install=(pacman -S --noconfirm --needed)
        column=5
        command="sudo pacman -S"
        tools=(python xorg-server-xvfb xorg-xauth dbus pulseaudio libpulse mesa-utils mesa)
        ;;
    *) exit 2 ;;
esac
if [[ $format == deb ]]; then
    deb=(/tmp/package/*.deb)
    dpkg-deb -x "${deb[0]}" /matrix/unpacked
    probe_root=/matrix/unpacked/usr/lib/visualsubforge
else
    tar -xzf /tmp/package/*_linux.tar.gz -C /matrix/unpacked
    roots=(/matrix/unpacked/*); probe_root=${roots[0]}
fi
table="$probe_root/share/doc/visualsubforge/runtime-packages.tsv"
cp "$table" /matrix/prerequisites.txt
cut -f1 "$table" > /matrix/sonames.txt
mapfile -t names < /matrix/sonames.txt
: > /matrix/missing-sonames.txt
: > /matrix/runtime-packages.txt
: > /matrix/expected-launcher.txt
declare -A seen
# Probe individually on the pristine base, independently of the launcher.
while read -r -a row; do
    name=${row[0]}
    package=${row[column-1]}
    if "$probe_root/bin/check-runtime" "$name" > /dev/null; then continue; fi
    echo "$name" >> /matrix/missing-sonames.txt
    printf 'VisualSubForge requires %s (package %s)\n' "$name" "$package" >> /matrix/expected-launcher.txt
    if [[ ! ${seen[$package]+yes} ]]; then
        echo "$package" >> /matrix/runtime-packages.txt
        seen[$package]=1
    fi
done < "$table"
mapfile -t missing_packages < /matrix/runtime-packages.txt
if (( ${#missing_packages[@]} )); then
    echo "$command ${missing_packages[*]}" >> /matrix/expected-launcher.txt
    # A sentinel at the actual exec target detects any start before refusal.
    mv "$probe_root/bin/visualsubforge.real" "$probe_root/bin/saved-app"
    printf '#!/bin/sh\ntouch /matrix/started-too-soon\nexit 99\n' > "$probe_root/bin/visualsubforge.real"
    chmod +x "$probe_root/bin/visualsubforge.real"
    status=0
    "$probe_root/bin/visualsubforge" --help > /matrix/missing-runtime.txt 2>&1 || status=$?
    cat /matrix/missing-runtime.txt
    test "$status" = 78
    test ! -e /matrix/started-too-soon
    [[ $(sha256sum < /matrix/expected-launcher.txt) == "$(sha256sum < /matrix/missing-runtime.txt)" ]]
    mv "$probe_root/bin/saved-app" "$probe_root/bin/visualsubforge.real"
fi
if [[ $format == deb ]]; then
    control=$(mktemp -d)
    dpkg-deb --control "${deb[0]}" "$control"
    for script in preinst postinst prerm postrm; do test ! -e "$control/$script"; done
    depends=$(dpkg-deb -f "${deb[0]}" Depends)
    echo "$depends" > /matrix/depends.txt
    dpkg-deb --fsys-tarfile "${deb[0]}" | tar -t | sort > /matrix/deb-entries.txt
    # Exercise apt's dependency solver on the pristine image. After removal,
    # its dependencies remain. Use that filesystem as the baseline for an
    # install/remove comparison with exactly the same runtime packages.
    "${install[@]}" "${deb[0]}"
    dpkg-query -W > /matrix/runtime-installed.txt
    apt-get remove -y visualsubforge
    while IFS= read -r entry; do
        if [[ -e /$entry || -L /$entry ]]; then
            [[ -d /$entry && ! -L /$entry ]] || { echo "Package file survived removal: $entry" >&2; exit 1; }
            echo "$entry"
        fi
    done < /matrix/deb-entries.txt > /matrix/before.txt
    "${install[@]}" "${deb[0]}"
    root=/usr/lib/visualsubforge
else
    if (( ${#missing_packages[@]} )); then "${install[@]}" "${missing_packages[@]}"; fi
    # Exercise the offline installer before test utilities can supply tools
    # that are absent on the distro's minimal runtime.
    mkdir /matrix/portable-home
    HOME=/matrix/portable-home "$probe_root/install.sh"
    HOME=/matrix/portable-home "$probe_root/install.sh"
    HOME=/matrix/portable-home /matrix/portable-home/.local/opt/visualsubforge/uninstall.sh
    rmdir /matrix/portable-home
    root=$probe_root
fi
"$root/bin/check-runtime" "${names[@]}"
# Exercise the actual packaged binaries before installing any GUI test tools.
"$root/bin/ffmpeg" -version > /matrix/ffmpeg-version.txt
"$root/bin/vsf" --version > /matrix/vsf-version.txt
for file in "$root/bin/"{visualsubforge.real,vsf,ffmpeg,ffprobe,check-runtime} "$root"/lib/*.so*; do ldd "$file"; done > /matrix/ldd.txt
if grep -F 'not found' /matrix/ldd.txt; then exit 1; fi
if [[ $format == deb ]]; then
    apt-get remove -y visualsubforge
    while IFS= read -r entry; do if [[ -e /$entry || -L /$entry ]]; then echo "$entry"; fi; done < /matrix/deb-entries.txt > /matrix/after.txt
    diff -u /matrix/before.txt /matrix/after.txt
fi
printf '%s\n' "${tools[@]}" > /matrix/test-tools.txt
"${install[@]}" "${tools[@]}"
case "$ID" in
    debian|ubuntu) dpkg-query -W > /matrix/packages.txt ;;
    fedora) rpm -qa | sort > /matrix/packages.txt ;;
    arch) pacman -Q > /matrix/packages.txt ;;
esac
useradd --create-home --uid 10001 preview
printf '%s\n' "$root" > /matrix/root
