#!/usr/bin/env bash
set -euxo pipefail
export XDG_RUNTIME_DIR=/tmp/vsf-gui-runtime
mkdir -m 700 "$XDG_RUNTIME_DIR"
pulseaudio --start --exit-idle-time=-1
trap 'pulseaudio --kill' EXIT
pactl load-module module-null-sink sink_name=vsf_ci > /evidence/null-sink.txt
pactl set-default-sink vsf_ci
glxinfo -B > /evidence/glxinfo.txt
pactl list short sinks > /evidence/audio-sinks.txt
python3 /check-gui.py --executable /usr/lib/visualsubforge/visualsubforge \
  --fixture /fixtures/vfr-offset.mkv --output /evidence/gui \
  --environment 'Ubuntu 24.04 container, network disabled, Xvfb/Mesa software renderer, PulseAudio null sink'
