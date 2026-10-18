# Preview candidate manual acceptance

The Windows x64 and Ubuntu24.04 x64 candidates below are updated to f0dbf68 on
October18 and include the feature milestones. The Mac downloads in the table
remain October12 baselines. Use an identified build containing the milestones
for the extended feature checks below. Their fixture pack must contain `subtitles/`, `plugins/` and
`legacy.vssprj`; earlier fixture archives do not contain them. Record the fixture
`provenance.json` and `SHA256SUMS` alongside the application revision. A newer
fixture pack does not establish that an older app implements these features.

**macOS replacement candidates (2026-10-12):** the links below replace the
withdrawn apps from run 38205417832. Disabling unused mpv scripts fixes their
hardened-runtime startup termination. Both replacements pass a hardened native
initialization check. The exact notarized ARM download also passes the native
smoke and restores an older project on a physical macOS 27 Mac, with video and
waveform visible. This does not complete audio, clean-install or oldest-OS gates.

Use the candidate and `preview-acceptance-fixtures` artifact from the linked
package run. The fixture artifact is produced by the Linux workflow for use
on all platforms; [run 38208853449](https://github.com/golgeek/visualsubforge/actions/runs/38208853449)
includes it. Installation/playback
acceptance is still open. Do not infer support from a successful CI build.

The following candidates have audited source inventories. Windows and Ubuntu
artifacts include their source archives; the Windows download is about 6.0 GB,
although its installer is only about 29 MiB. Actions artifacts have limited
retention; preserve the candidate, matching sources and checksums together.

| Target | Candidate download | Source archives |
| --- | --- | --- |
| Apple Silicon, macOS 14+ build floor | [Signed/notarized app](https://github.com/golgeek/visualsubforge/actions/runs/38334674579/artifacts/11784371040) | [Matching ARM inventory/sources](https://github.com/golgeek/visualsubforge/actions/runs/38331848710/artifacts/11782814984) |
| Intel, macOS 15+ build floor | [Signed/notarized app](https://github.com/golgeek/visualsubforge/actions/runs/38335057462/artifacts/11784079472) | [Matching Intel inventory/sources](https://github.com/golgeek/visualsubforge/actions/runs/38331848710/artifacts/11783364443) |
| Windows x64, f0dbf68 (October18) | [Unsigned installer and portable ZIP](https://github.com/golgeek/visualsubforge/actions/runs/39142924874/artifacts/12143876134) | Included in the same artifact |
| Ubuntu 24.04 x64, f0dbf68 (October18) | [Debian package](https://github.com/golgeek/visualsubforge/actions/runs/39142923333/artifacts/12143295022) | Included in the same artifact |

Use the shared [fixtures](https://github.com/golgeek/visualsubforge/actions/runs/38208853449/artifacts/11729128569).
The October18 Linux run also retains the baseline fixture set; extended feature
fixtures can be generated from b379885 or later with
`python3 scripts/package-release-fixtures.py --output /path/to/new-fixture-folder`
after the three media-generation recipes listed in its provenance. The collector
records the checkout revision, dirty state and exact input/recipe hashes.
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

For builds containing the feature milestones, also run these checks. Work on
separate output files and keep the packaged inputs unchanged. The tiny embedded
fonts are original test shapes; do not install them as system fonts.

| Check | Input/action | Expected result |
| --- | --- | --- |
| ASS/SSA and fonts | Open `subtitles/styles.ass` and `styles.ssa` over the MP4. Inspect missing-font notices, edit a style, assign it to a cue, then Undo/Redo. Open `embedded-font.ass`; inspect its font attachment, replace it with `fixture-font-cff.otf`, remove it, then Undo each operation. | Original metadata and override tags survive save/reopen. Style and attachment edits are reviewed and independently undoable. With the attached TTF, A is a triangle and B a rectangle; the CFF font reverses those shapes. Record missing glyphs, CJK fallback and physical preview separately. `fixture-font-cff2.otf` is an additional format control; its inclusion is not a GPU acceptance claim. |
| Tagged SRT and batch timing | Open `subtitles/fansub.srt`. Select cues 1 and 3 with the platform selection modifier; preview a +100 ms shift and accept, Undo and Redo. Repeat contiguous keyboard selection. | Exactly the selected cues move. ASS overrides, HTML markup, Unicode and the unselected cue remain unchanged. Each accepted batch is one Undo step. Save the result separately. |
| Styled resync | Repeat the offset resync with `subtitles/resync.ass`, `.ssa` and `.srt`. Repeat the speed/ad-edit analysis using these original files. | Offset proposals start cues at 6.000 and 7.100 seconds and preserve metadata/tags. Speed changes refuse the first cue's `\k20` timed effect; no partial edit or output is accepted. The existing plain SRT control above still completes. |
| Language tools | Select English spelling for `subtitles/english.srt`; review a correction of one `helo` to `hello`. Select French for `french.srt` and review `bonjor` to `bonjour`. Exercise Ignore and a personal word, then Undo a correction. | Suggestions use the selected dictionary. Correcting one occurrence preserves the other English occurrence, emoji, markup and timing. Ignore/personal words follow their documented session/persistent scope. Restore any test personal words/settings afterward. |
| Conversion and WebVTT | Review an ASS-to-SRT export of `styles.ass`, then save a separate output. Open `styles.vtt`, edit ordinary visible text and timing, save separately and reopen. | Conversion reports style/metadata losses before publication and leaves the source/history unchanged. WebVTT identifiers, settings and STYLE/REGION/NOTE blocks survive; unsupported constructs retain their documented preview limitations. |
| Legacy project import | Import `legacy.vssprj`. In a separate copy, change the video path to a nonexistent file and repeat, locating `h264-1080p.mp4` explicitly. | Review resolves three subtitle cues and the relative video path, reports the unsupported dictionary setting and focused-time interpretation, and resumes paused at 1.250 seconds after creating a new project. Missing media requires relocation; original project/subtitles remain unchanged. |
| Manual translation | Attach `subtitles/fansub.srt` as a reference, start an independent target copy, set a +100 ms matching offset and edit one target cue. Save subtitles/project separately and reopen. | Reference text remains unchanged; matching uses the chosen offset without retiming either document. Reopened projects retain the reference relationship and target changes. Reference and target markup remain intact. |
| Modern and VSS plugins | With a fresh `fansub.srt`, select each `plugins/*/manifest.json` in turn, check, preview fixes and accept; Undo/Redo, save separately and reopen. | Each example finds one cue and proposes one text change: remove the two trailing spaces from cue 1. All times, tags and other text remain unchanged. Acceptance is one Undo step. These independently authored examples exercise the documented subset, not historical-host parity. |

The same plugin control can be checked from the installed CLI:

```sh
vsf plugin preview --subtitles subtitles/fansub.srt --package plugins/trim-trailing/manifest.json --json
vsf plugin apply --subtitles subtitles/fansub.srt --package plugins/trim-trailing/manifest.json --accept REVIEW_ID_FROM_PREVIEW --output plugin-result.srt --json
```

Use the actual review ID, then repeat with `plugins/vss-trim-trailing/manifest.json`
and a different output path. Expect one proposed change and a published receipt;
reusing an output path must refuse without changing its bytes. CLI publication
does not persist desktop Undo history.

Keep fixture inputs unchanged and verify their `SHA256SUMS` afterward. Submit a
pass/fail/not-tested result for each row, expected versus observed behavior,
logs/screenshots where useful, and exact reproduction steps. Report real speaker/
headphone playback separately from a virtual/null sink. Physical display/audio,
clean installation and oldest-OS results are separate acceptance evidence; one
tester or a VM pass cannot establish all three.
