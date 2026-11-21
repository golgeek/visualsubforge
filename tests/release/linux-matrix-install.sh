#!/usr/bin/env bash
# Build an offline-test image from the exact candidate and a pristine distro.
set -euo pipefail
format=$1
# shellcheck source=/dev/null
. /etc/os-release
mkdir /matrix
case "$ID" in
    ubuntu|debian)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update
        install=(apt-get install -y --no-install-recommends)
        gtk=libgtk-3-0
        case "$ID:$VERSION_ID" in ubuntu:24.*|ubuntu:26.*|debian:13) gtk=libgtk-3-0t64 ;; esac
        runtime=(libwebkit2gtk-4.1-0 "$gtk" libva2 libva-drm2 libva-x11-2)
        tools=(python3 xvfb xauth dbus-x11 pulseaudio pulseaudio-utils mesa-utils libgl1-mesa-dri)
        ;;
    fedora)
        install=(dnf install -y --setopt=install_weak_deps=False)
        runtime=(webkit2gtk4.1 gtk3 libva)
        tools=(python3 xorg-x11-server-Xvfb xorg-x11-xauth dbus-daemon pulseaudio pulseaudio-utils glx-utils mesa-dri-drivers shadow-utils util-linux)
        ;;
    arch)
        pacman -Sy --noconfirm
        install=(pacman -S --noconfirm --needed)
        runtime=(webkit2gtk-4.1 gtk3 libva)
        tools=(python xorg-server-xvfb xorg-xauth dbus pulseaudio libpulse mesa-utils mesa)
        ;;
    *) exit 2 ;;
esac
if [[ $format == deb ]]; then
    deb=(/tmp/package/*.deb)
    control=$(mktemp -d)
    dpkg-deb --control "${deb[0]}" "$control"
    for script in preinst postinst prerm postrm; do test ! -e "$control/$script"; done
    depends=$(dpkg-deb -f "${deb[0]}" Depends)
    echo "$depends" > /matrix/depends.txt
    for required in libva2 libva-drm2 libva-x11-2; do tr ',' '\n' <<<"$depends" | awk '{$1=$1};1' | grep -Fx "$required"; done
    apt-get satisfy -y --no-install-recommends "$depends"
    dpkg-deb --fsys-tarfile "${deb[0]}" | tar -t | sort > /matrix/deb-entries.txt
    while IFS= read -r entry; do if [[ -e /$entry || -L /$entry ]]; then echo "$entry"; fi; done < /matrix/deb-entries.txt > /matrix/before.txt
    "${install[@]}" "${deb[0]}"
    root=/usr/lib/visualsubforge
else
    printf '%s\n' "${runtime[@]}" > /matrix/runtime-packages.txt
    mkdir /matrix/unpacked
    tar -xzf /tmp/package/*_linux.tar.gz -C /matrix/unpacked
    roots=(/matrix/unpacked/*); root=${roots[0]}
    if "$root/bin/visualsubforge" --help > /matrix/missing-runtime.txt 2>&1; then
        echo 'Expected a missing-loader diagnosis on the minimal image' >&2; exit 1
    fi
    grep -F 'VisualSubForge requires' /matrix/missing-runtime.txt
    grep -F "${runtime[*]}" /matrix/missing-runtime.txt
    "${install[@]}" "${runtime[@]}"
fi
# Check these edges before adding any of the test tools.
"$root/bin/check-runtime"
for file in "$root/bin/"{visualsubforge.real,vsf,ffmpeg,ffprobe}; do ldd "$file"; done > /matrix/ldd.txt
if grep -F 'not found' /matrix/ldd.txt; then exit 1; fi
printf '%s\n' "${tools[@]}" > /matrix/test-tools.txt
"${install[@]}" "${tools[@]}"
case "$ID" in
    debian|ubuntu) dpkg-query -W > /matrix/packages.txt ;;
    fedora) rpm -qa | sort > /matrix/packages.txt ;;
    arch) pacman -Q > /matrix/packages.txt ;;
esac
useradd --create-home --uid 10001 preview
printf '%s\n' "$root" > /matrix/root
