#!/usr/bin/env bash
set -euxo pipefail
case "${VSF_NULL_SINK:-default}" in
  default|ready-default) sink_options=() ;;
  no-rewinds) sink_options=(norewinds=true) ;;
  *) echo 'Unknown diagnostic null-sink configuration' >&2; exit 2 ;;
esac
export XDG_RUNTIME_DIR=/tmp/vsf-gui-runtime
mkdir -m 700 "$XDG_RUNTIME_DIR"
pulseaudio --start --exit-idle-time=-1 --log-level=debug \
  --log-time=yes --log-target=file:/evidence/pulseaudio.log
silent_pid=
cleanup() {
  if [[ -n "$silent_pid" ]]; then
    kill "$silent_pid" || true
    wait "$silent_pid" || true
  fi
  pulseaudio --kill
}
trap cleanup EXIT
# PulseAudio creates this dedicated diagnostic file with mode 0600. The host
# artifact uploader has a different UID from this container's preview user.
chmod 644 /evidence/pulseaudio.log
pactl load-module module-null-sink sink_name=vsf_ci "${sink_options[@]}" > /evidence/null-sink.txt
pactl set-default-sink vsf_ci
glxinfo -B > /evidence/glxinfo.txt
pactl list short sinks > /evidence/audio-sinks.txt
pactl list sinks > /evidence/audio-sink-details.txt
if [[ "${VSF_NULL_SINK:-default}" == ready-default ]]; then
  # Keep the default sink's rewind behavior. A separate silent client requests
  # a bounded clock; observe readiness before launching the unchanged app.
  pacat --playback --raw --rate=44100 --channels=2 --format=s16le \
    --device=vsf_ci --latency-msec=160 --process-time-msec=20 \
    --client-name='VSF diagnostic clock' < /dev/zero > /evidence/silent-client.log 2>&1 &
  silent_pid=$!
  python3 - <<'PY'
import json, math, subprocess, time
from pathlib import Path

began = time.monotonic()
observations = []
consecutive = 0
ready = False
try:
    while time.monotonic() - began < 5:
        sinks = json.loads(subprocess.check_output(
            ['pactl', '--format=json', 'list', 'sinks'], text=True, timeout=2))
        sink = next(s for s in sinks if s['name'] == 'vsf_ci')
        latency = sink['latency']
        observations.append({'seconds': time.monotonic() - began,
                             'state': sink['state'], 'latency_us': latency})
        bounded = all(math.isfinite(latency[k]) and 0 <= latency[k] <= 100_000
                      for k in ('actual', 'configured'))
        consecutive = consecutive + 1 if sink['state'] == 'RUNNING' and bounded else 0
        if consecutive >= 5:
            ready = True
            break
        time.sleep(0.05)
    if not ready:
        raise RuntimeError('Virtual audio clock did not reach bounded latency')
finally:
    Path('/evidence/sink-readiness.json').write_text(json.dumps(
        {'ready': ready, 'observations': observations}, indent=2) + '\n')
PY
  kill -0 "$silent_pid"
fi
python3 /check-gui.py --executable /usr/lib/visualsubforge/visualsubforge \
  --fixture /fixtures/vfr-offset.mkv --output /evidence/gui \
  --environment "Ubuntu 24.04 container, network disabled, Xvfb/Mesa software renderer, PulseAudio ${VSF_NULL_SINK:-default} null sink"
