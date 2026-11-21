#!/usr/bin/env bash
# Check the actual apt transaction in a fresh Resolute container.
set -euo pipefail
[[ $# == 1 && -f "$1" ]]
docker run --rm -i -v "$(realpath "$1"):/tmp/visualsubforge.deb:ro" \
  ubuntu:26.04@sha256:d66b366dac61b575bee2aef07e9c26e8070096fbe84a3d4d3eb0da069b5d76db bash -eu <<'CHECK'
export LC_ALL=C DEBIAN_FRONTEND=noninteractive
apt-get update -qq
sha256sum /tmp/visualsubforge.deb
dpkg-deb -f /tmp/visualsubforge.deb Version Depends
cp /var/lib/dpkg/status /tmp/status-before
rc=0
apt-get install -y --no-install-recommends /tmp/visualsubforge.deb > /tmp/apt-output 2>&1 || rc=$?
cat /tmp/apt-output
test "$rc" -eq 100
grep -Eq 'Depends: libxml2(:amd64)? .*but it is not installable' /tmp/apt-output
cmp /tmp/status-before /var/lib/dpkg/status
test ! -e /usr/lib/visualsubforge
echo "Apt refused the package before unpacking. The dpkg status database is unchanged."
CHECK
