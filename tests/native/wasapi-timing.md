# Standalone WASAPI timing reference

[Task 005](../../docs/tasks/005-audio-loop-timing.md) diagnostic, not an application backend. [wasapi-timing-probe.cpp](wasapi-timing-probe.cpp) generates a known four-second, 48 kHz stereo float signal: 440 Hz until source frame 72000, 660 Hz until frame 120000, then 880 Hz. It uses public shared-mode WASAPI directly, without mpv, FFmpeg, a decoder, UI, or queued-timestamp mapper.

The one MTA thread owns every COM/audio interface. Source samples and bounded trace storage are allocated before playback. It fills the available render buffer, samples `IAudioClock::GetPosition` with surrounding QPC reads, and records submitted-frame counts and padding. No file IO or trace logging occurs while playing. A scope guard stops the client before releasing it, including error cleanup. Reports use exclusive creation; the process exits nonzero on expected failures.

Modes are `ordinary`, `refresh` and `loop-clear`. Ordinary starts at source frame zero. Refresh stops/resets/restarts at one second, resolving the source frame from the last device-position observation. Loop-clear initially submits the half-open frame interval `[48624, 64128)` repeatedly, then performs that reset at five seconds and continues linearly from the observed loop position. These explicit source mappings are independently checkable; this does not reimplement mpv's decoder/queue behavior. Runs continue with silent frames after the source ends so both marker crossings remain observable.

Compile both helpers in the Windows CLANGARM64 development shell:

```sh
clang++ -std=c++17 -Wall -Wextra -Werror -municode \
  wasapi-timing-probe.cpp -o player.exe -lole32 -lmmdevapi -luuid
clang++ -std=c++17 -Wall -Wextra -Werror -municode \
  windows-process-capture.cpp -o capture.exe -lole32 -lmmdevapi -luuid
```

The player waits up to eight seconds for the recorder's readiness file before starting audio. The recorder must target that player's PID. Use a fresh case directory so reports, ready files and PCM cannot collide with earlier evidence. For example, from Python on Windows, with absolute `player`, `capture` and `folder` Paths:

```python
import json
import subprocess

mode = "ordinary"  # or refresh / loop-clear
player_args = [str(player), mode, str(folder / "ready"), str(folder / "report.json")]
process = subprocess.Popen(player_args)
recorder = None
try:
    capture_args = [str(capture), str(process.pid), "14" if mode == "loop-clear" else "9",
                    str(folder / "output.f32"), str(folder / "ready")]
    recorder = subprocess.Popen(capture_args)
    result = {"mode": mode, "player_exit": process.wait(timeout=22),
              "capture_exit": recorder.wait(timeout=12),
              "commands": [player_args, capture_args]}
    (folder / "case.json").write_text(json.dumps(result, indent=2))
finally:
    for child in (process, recorder):
        if child is not None and child.poll() is None:
            child.kill()
            child.wait()
```

Analyze with `python wasapi-timing-analysis.py CASE_DIRECTORY`, keeping `audio-tone-content.py` beside the analyzer. It reuses the existing tone-content oracle, validates packet/clock/reset ordering, and compares full captured tone-window intervals with device-clock crossing intervals. Both native APIs report timestamps in the same machine QPC domain, so no process-launch calibration or fitted clock offset is used. A successful analysis is an observation, not a claim that timing is accurate. Timing-error flags and imprecise device samples at marker crossings are rejected; other discontinuities remain visible.

Six cases on the supplied Windows 11 ARM64 Parallels VM reproduce positive capture-versus-device residuals in every marker comparison, including roughly 100–111 ms lower bounds after loop-clear. Therefore mpv is not necessary for this discrepancy in this environment. This does not identify its sole cause, establish acoustic latency, or validate physical Windows hardware, x86-64, MSVC/Tauri integration, or device-removal recovery. Task 005 retains exact bounds, source/binary hashes, three native negative controls, four analyzer negative controls, and both initial build failures.
