# Preview candidate manual acceptance

Use the candidate and `preview-acceptance-fixtures` artifact from the linked
package run. The fixture artifact is produced by the Linux workflow for use
on all platforms; [run 38203253891](https://github.com/golgeek/visualsubforge/actions/runs/38203253891)
includes it. Installation/playback
acceptance is still open. Do not infer support from a successful CI build.

Record the package SHA-256, app revision, OS version/architecture, physical
machine or VM, GPU/driver, display scaling and audio device. Test on a clean OS
without FFmpeg, mpv, developer SDKs or codec packs. Installation may download
ordinary OS runtimes; disconnect networking after installation. On Linux use
`sudo apt install ./<candidate>.deb`; on Windows use the installer; on macOS copy
the notarized app into Applications. Record security prompts and every extra step.

The current Mac candidates require macOS 14 on Apple Silicon and macOS 15 on
Intel. Ubuntu candidates target 24.04 x64. Windows x64 candidates are built on
Server 2022; clean Windows 10/11 behavior remains to be established. A newer-OS
pass does not establish the oldest-version gate.

| Check | Input/action | Expected result |
| --- | --- | --- |
| Offline launch | Start from the normal application launcher. | Editor opens without missing-library/tool errors or PATH changes. |
| Video and sound | Open `h264-1080p.mp4`, then `hevc-4k.mp4`. | Twelve seconds of moving test patterns and a continuous 440 Hz tone; colours invert at six seconds. Seek before/after six seconds and near the end. No stale frame, crash or stuck playback. Record the decoder actually reported and any dropped frames; these clips alone cannot measure A/V latency. |
| Subtitle preview | Open `playback.srt` with either MP4. Pause inside each cue. | Text, accents and italic markup render; the second cue starts at the colour inversion. Edit/apply, Undo and Redo update the paused preview. |
| Variable rate and tracks | Open `vfr-offset.mkv`. Step forward from two seconds; switch audio tracks. | The next frame is at 2.1 seconds. Two tracks are selectable; brief impulses occur at 1.25 and 2.0 seconds respectively. A 1.0–1.5 second selection loops and clearing it restores normal playback. |
| Analysis | Allow waveform and scene detection to complete on an MP4. | Waveform appears, scene detection finds the six-second visual cut, and seeking a marker shows the corresponding frame. Record selected detection detail and any additional/missing cuts. |
| Files and history | Save an edited subtitle copy, close and reopen it; save/reopen a project. | Accepted text/timings survive. Undo/Redo restores exact edits before closing. Original fixture SRT remains unchanged. A cancelled Save As changes neither path nor bytes. |
| Settings | Change one harmless setting, Apply, restart, then restore it. | Applied settings persist; cancelled drafts do not. Project overrides remain distinguishable from workspace defaults. |
| Resync | Load `resync/offset.mkv` and `resync/source.srt`; Tools → Resynchronize Subtitles, source `resync/source.mkv`. Repeat with `speed-ads.mkv`. | Complete proposals match the corresponding `.expected.srt` timings/text. Accept is one Undo/Redo step. Save As to a new file; the source is preserved. |
| Replacement and quit | Replace videos during analysis, close media, reopen, then quit while playing. | No old result replaces the current media; no crash, hang, continuing sound or surviving VSF/FFmpeg process after shutdown. |

For a CLI comparison, run the installed `vsf` (`vsf.exe` on Windows; inside
`VisualSubForge.app/Contents/MacOS` on Mac) from the extracted fixture directory:
replace `vsf` below with its full installed path; PowerShell requires `&` before
a quoted executable path. No PATH change is needed.

```sh
vsf resync --source-video resync/source.mkv --source-subtitles resync/source.srt --target-video resync/offset.mkv --output offset-result.srt --report offset-report.json --json
```

Expect exit zero, `status: complete`, and text/timings matching
`resync/offset.expected.srt`. Repeating that exact command must refuse existing
outputs and leave their bytes unchanged. Use new output/report names for the
speed/ad-edit case. No separately installed analysis tools should be needed.

Keep fixture inputs unchanged and verify their `SHA256SUMS` afterward. Submit a
pass/fail/not-tested result for each row, expected versus observed behavior,
logs/screenshots where useful, and exact reproduction steps. Report real speaker/
headphone playback separately from a virtual/null sink. Physical display/audio,
clean installation and oldest-OS results are separate acceptance evidence; one
tester or a VM pass cannot establish all three.
