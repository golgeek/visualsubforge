# Legacy VisualSubSync project import

Stage6b is in progress. The intended workflow is an explicit import review,
reference relocation when needed, and publication of a new `.vsfproject` file.
No desktop import capability is claimed until its owner, UI and native checks
are complete. WebVTT is already accepted; translation remains the next increment.

The application foundation now parses legacy projects, inspects/relocates selected
references, validates subtitles and revalidates captured text inputs. The desktop
review owner, guarded publication and UI are still pending.

## Reference and supported mapping

Read-only source inspection uses VisualSubSync commit
`71778b9115e57c03e9be85234318c2004ac427b3`: `ProjectUnit.pas`,
`TVSSProject.LoadFromINIFile`; `main.pas`, `TMainForm.LoadProject` and
`TMainForm.SaveProject`; `ThirdParty/TntIniFiles.pas`, `TTntIniFile.Create` and
`ReadString`; and `MiscToolsUnit.pas`, `WideResolveRelativePath`.
This is source inference, not a new run of the legacy application. No code or
subtitle content is copied, and the legacy checkout remains unchanged.

The legacy writer creates BOM-marked UTF-16LE INI files on Unicode Windows and
stores paths relative to the project. Import also accepts strict UTF-8 (optional
BOM) and BOM-marked UTF-16BE. Invalid encodings and unmarked legacy code pages
are refused with conversion guidance; locale guessing could select the wrong
file. Sections/keys are case insensitive and matching surrounding quotes are
removed, following [Windows INI string semantics](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getprivateprofilestring).
Duplicate keys are refused rather than
guessing which value the author intended. Limits are 256 KiB, 512 settings and
4096 characters per value. Unknown settings produce review warnings.

| Legacy data | Planned import behavior |
| --- | --- |
| `VideoSource` | Selected media, with explicit relocation if missing or foreign-platform path syntax. |
| `WAVFile` | Selected media when VideoSource is empty. Separate external audio is diagnosed when video is selected; VSF uses the selected media's audio and waveform. |
| `SubtitlesFile` | Existing UTF-8 SRT/ASS/SSA/WebVTT, validated before publication; no subtitle rewrite. Missing/invalid references can be replaced explicitly. |
| `FocusedTimeMs` | Paused resume position in seconds at the project boundary. Legacy uses it to select a cue; this difference is reported. |
| `WAVDisplayerPositionStartMs` / `WAVDisplayerPositionStopMs` | Valid waveform interval, with the legacy 0–15 second default and explicit warnings for invalid/partial ranges. |
| `WAVMode`, `PeakFile` | Review warns that waveform is regenerated from the selected media. No legacy peak/cache import. |
| `SubtitlesVO` | Retained in review with an unsupported-reference warning until the separate translation workflow supports attachment. |
| `Dictionary`, `Presets` | Reported; VSF preferences remain authoritative. No legacy profile/plugin execution or implicit dictionary substitution. |
| Text-pipe/script data, display flags, window geometry, unknown sections/keys | Reported as unsupported, not applied or executed. |

The supported subset requires one available media file and one valid subtitle
file. Subtitle-only and peak-only projects need an explicitly chosen media file.
Relative Windows separators resolve against the project directory; Windows drive,
UNC and POSIX absolute paths are never guessed into another platform's roots.
URLs, drive-relative and device paths require explicit relocation. No recursive
basename search silently chooses a similarly named file.

## Ownership and acceptance plan

Application parsing and reference inspection are independent of Tauri and native
media. All IO runs on blocking workers. Rust's project service will own one
bounded import review with captured source/subtitle bytes, selected media identity
and immutable proposed project data. UI edits/reply epochs remain transient.
Cancel/replacement must invalidate old work before publication; acceptance must
recheck reviewed inputs and refuse existing output paths, including aliases.
An import cannot mutate active subtitles, history, preferences or the current
project attachment. Opening the exported file uses normal protected restoration.

Validate UTF-8/UTF-16, Unicode and Windows paths, missing/relocated references,
unsupported/invalid settings, all four subtitle formats, original preservation,
source changes, cancellation/replacement, failed publication, fresh reopen and
normal shutdown. Browser and actual native import/chooser checks are required.
Use the exact 10k plain/styled/timed WebVTT inputs plus a representative ASS input
for affected validation paths: <=500ms pure p95, <=96MiB additional sampled RSS,
<1000ms interactive response and a ten-second cooperative preparation deadline.
Blocking filesystem calls remain non-preemptible. These limits are not relaxed
to accommodate a failing implementation.

## Foundation validation — 2026-10-18

Development Apple Silicon Mac, Rust 1.98.1 and Python 3.14.7:

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p vsf-application --lib legacy_project
cargo clippy --locked -p vsf-application --all-targets -- -D warnings
cargo fmt --all -- --check
python3 scripts/webvtt-fixtures.py
python3 scripts/ass-fixtures.py
cargo build --locked --release -p vsf-application --example legacy-project-benchmark
python3 scripts/legacy-project-benchmark.py
git diff --check
```

413 workspace Rust tests pass, with 30 native/external tests ignored; zero-test
harnesses are not feature validation. Fourteen new tests cover UTF-8 and both
UTF-16 orders, astral Unicode and actual Unicode file paths, malformed encodings,
case/quotes/duplicates, invalid/sentinel/range positions, unsupported-setting
diagnostics, bounded inputs, Windows/UNC/device/URL distinctions, missing and
explicitly relocated references, all four subtitle formats, source/subtitle
changes and retargeted symlinks, cancellation and unchanged original files.
The final focused run passes all fourteen cases. Strict workspace/application
Clippy and formatting pass. The initial Clippy run flagged the pinned toolchain's
preferred `as_chunks` API; that was corrected, with its failed log retained.

The benchmark generates four UTF-16LE project wrappers and copies independently
authored 10k subtitle inputs before the RSS baseline. Each of one warmup plus
twenty measured cycles reads/parses the project, resolves references, reads and
fully validates subtitles, rechecks captured project/subtitle paths and bytes,
and independently verifies unchanged originals. It does not open media, mutate
history, publish a project or run the desktop. Elapsed timings include IO.

| 10,000-cue subtitle input | Bytes | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Plain WebVTT | 682,922 | 9.531ms | 9.688ms |
| Styled WebVTT, BOM/CRLF | 2,052,100 | 18.271ms | 18.611ms |
| Timed WebVTT, mixed endings | 1,838,692 | 15.908ms | 16.038ms |
| Styled ASS | 1,266,177 | 36.236ms | 36.764ms |

Additional sampled RSS is 50,724,864 bytes across 119 observations, below the
96 MiB limit. Sampling is not exhaustive allocation accounting. The exact
subtitle and project SHA256 hashes are in `benchmark-2.json`; WebVTT hashes match
the already-accepted format/desktop fixtures. Logs and the earlier inspection-only
benchmark remain in `test-results/legacy-project-foundation-20261018/`.
No frontend or native tests were rerun for this application-only increment.

Next: the bounded Rust import review/publication owner, then the typed review UI,
relink/retry/cancel workflows, source-change and output-race tests, actual native
and chooser checks, and final desktop 10k measurements. Stage6b and ADR0032 remain
in progress/proposed; this foundation does not close the import milestone.
