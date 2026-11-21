#!/bin/sh
set -eu
exec "$(dirname "$(readlink -f "$0")")/install.sh" --uninstall "$@"
