# Preview candidate manual acceptance

**macOS candidate withdrawn from testing:** the signed apps from run
38205417832 can be killed by hardened-runtime enforcement when media opens.
The failure was reproduced on macOS 27 Apple Silicon with a synthetic fixture,
independent of project loading: mpv's built-in `stats` LuaJIT script executes an
unsigned page. Replacement packages with scripting disabled are being validated.
The Mac links below retain the original evidence, not a recommended download.

Use the candidate and `preview-acceptance-fixtures` artifact from the linked
package run. The fixture artifact is produced by the Linux workflow for use
on all platforms; [run 38208853449](https://github.com/golgeek/visualsubforge/actions/runs/38208853449)
includes it. Installation/playback
acceptance is still open. Do not infer support from a successful CI build.

The following candidates have audited source inventories. Windows and Ubuntu
artifacts include their source archives; the Windows download is about 6.6 GB,
although its installer is only about 25 MiB. Actions artifacts have limited
retention; preserve the candidate, matching sources and checksums together.

| Target | Candidate download | Source archives |
| --- | --- | --- |
| Apple Silicon, macOS 14+ build floor | [Signed/notarized app](https://github.com/golgeek/visualsubforge/actions/runs/38205417832/artifacts/11727984565) | [Matching ARM inventory/sources](https://github.com/golgeek/visualsubforge/actions/runs/38205417832/artifacts/11727318961) |
| Intel, macOS 15+ build floor | [Signed/notarized app](https://github.com/golgeek/visualsubforge/actions/runs/38205417832/artifacts/11728236399) | [Matching Intel inventory/sources](https://github.com/golgeek/visualsubforge/actions/runs/38205417832/artifacts/11728039019) |
| Windows x64 | [Unsigned installer and portable ZIP](https://github.com/golgeek/visualsubforge/actions/runs/38227273736/artifacts/11738654119) | Included in the same artifact |
| Ubuntu 24.04 x64 | [Debian package](https://github.com/golgeek/visualsubforge/actions/runs/38208853449/artifacts/11728388564) | Included in the same artifact |

Use the shared [fixtures](https://github.com/golgeek/visualsubforge/actions/runs/38208853449/artifacts/11729128569).
[Dependency review](dependency-review.md) records package hashes and evidence.
Windows installation/offline/native smoke passes with a virtual WASAPI device
and software D3D11; Ubuntu smoke uses a prepared virtual PulseAudio sink. Neither
establishes physical video/audio acceptance. The Mac signature checks likewise
are separate from this manual checklist.

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
| First playback and idle resume | Start playback immediately after opening VSF, then pause long enough for the audio device to idle and resume. Repeat the short selection loop. | Responsive video and sound without a long initial stall or truncated loop. Record observed delay and audio-device/backend details separately from later steady playback; CI uses a virtual sink whose clock is prepared before VSF starts. |
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
