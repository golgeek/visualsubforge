#!/usr/bin/env bash
set -euo pipefail
export WAYLAND_DISPLAY=vsf-wayland
weston --backend=headless --socket="$WAYLAND_DISPLAY" --xwayland --idle-time=0 \
    --log="$VSF_EVIDENCE/weston.log" &
compositor=$!
trap 'kill "$compositor" || true; wait "$compositor" || true' EXIT
for _ in {1..100}; do
    kill -0 "$compositor"
    if [[ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]]; then
        if [[ $GDK_BACKEND == wayland ]]; then break; fi
        display=$(sed -nE 's/.*xserver listening on display (:[0-9]+).*/\1/p' "$VSF_EVIDENCE/weston.log")
        if [[ -n $display ]]; then export DISPLAY="$display"; break; fi
    fi
    sleep 0.1
done
bash tests/release/linux-gui.sh
