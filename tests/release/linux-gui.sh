#!/usr/bin/env bash
set -euxo pipefail
case "${VSF_NULL_SINK:-default}" in
  default) sink_options=() ;;
  no-rewinds) sink_options=(norewinds=true) ;;
  *) echo 'Unknown diagnostic null-sink configuration' >&2; exit 2 ;;
esac
export XDG_RUNTIME_DIR=/tmp/vsf-gui-runtime
mkdir -m 700 "$XDG_RUNTIME_DIR"
pulseaudio --start --exit-idle-time=-1 --log-level=debug \
  --log-time=yes --log-target=file:/evidence/pulseaudio.log
trap 'pulseaudio --kill' EXIT
pactl load-module module-null-sink sink_name=vsf_ci "${sink_options[@]}" > /evidence/null-sink.txt
pactl set-default-sink vsf_ci
glxinfo -B > /evidence/glxinfo.txt
pactl list short sinks > /evidence/audio-sinks.txt
pactl list sinks > /evidence/audio-sink-details.txt
python3 /check-gui.py --executable /usr/lib/visualsubforge/visualsubforge \
  --fixture /fixtures/vfr-offset.mkv --output /evidence/gui \
  --environment 'Ubuntu 24.04 container, network disabled, Xvfb/Mesa software renderer, PulseAudio null sink'
