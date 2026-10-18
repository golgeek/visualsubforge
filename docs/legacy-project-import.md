# Legacy VisualSubSync project import

Stage6b is in progress. File → Import VisualSubSync Project now provides an
explicit review, file relocation and publication of a new `.vsfproject`.
The Rust owner and browser workflows are implemented; actual native, chooser and
desktop 10k acceptance remain pending. WebVTT is already accepted; translation
remains the next increment. This is not a release/platform acceptance claim.

Choose a `.vssprj`, then Review import. Locate missing media/subtitles explicitly
and review again after each change. Review shows original/resolved paths,
unsupported settings, preserved millisecond positions and the captured workspace
default quality profile. Create new VSF project requires an unused filename.
Cancelling the picker or a failed destination permits retry. Changed inputs require
a fresh review. After creation, Open created project uses normal protected project
restoration; creating the file does not apply or discard an active cue draft.
Import remains available without native playback; opening the resulting project
requires a build with playback support, which the dialog explains.

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

| Legacy data | Import behavior |
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

## Ownership and remaining acceptance

Application parsing and reference inspection are independent of Tauri and native
media. Rust's `Projects` service owns one additional import IO worker and one
immutable review, with no queue. Preparation captures exact source/subtitle bytes,
canonical and selected paths, sampled media identity and workspace default policy.
It never borrows the current project's effective policy. Preparation and publication
share the same admission slot and ten-second cooperative deadline. Workers capture
only immutable input/job state, not the service; shutdown takes the join handle
under the owner mutex and joins outside owner/document/media locks.
The worker is separate from revision-bound document analysis because import can
prepare a different project while no subtitle document is loaded.

Publication obtains the existing writer lease and writes/fsyncs a temporary file
off-lock. It rechecks source/subtitle bytes and selected symlinks, media selection
and sampled identity, and all protected original path names. A per-job mutex
serializes final cancellation with no-clobber publication. A competing output wins
without replacement; failed writes clean up their temporary and retain a retryable
review. A successful receipt survives late Cancel. These checks are not a cross-file
transaction against external writers: a non-cooperating edit can still race the
last input check. Media identity is sampled metadata/content, not full-file hashing
or decoder validation. Normal project open performs its usual media validation.

`project_import_start/view/cancel/publish` are thin blocking-task IPC adapters.
`LegacyImportSession` holds transient selected paths, report and request epochs;
one in-flight status read prevents queues and stale responses cannot revive retired
reviews. An unknown publication outcome keeps duplicate-export and close guards
until status is resolved. Closing during preparation cancels even a late job ID.
No accepted document/history, preferences or project attachment is changed.

Walkthrough: App's File action occludes native video and mounts
`LegacyImportDialog`; its session calls the typed client to start Rust inspection.
The same Rust owner publishes reviewed data to a new file. Only the separate Open
button calls `ProjectWorkflow.openPath`, which preflights that exact path, protects
unsaved work, restores media/subtitles and finally attaches the project. App owns
modal/focus restoration. The dialog itself has no document or media owner.

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

## Owner and UI validation — 2026-10-18

Same development Mac, Rust 1.98.1, Node 24.21.0/npm 11.19.0 (`source
"$HOME/.nvm/nvm.sh" && nvm use --silent` before Node commands):

```sh
cargo test --locked -p visualsubforge project::import
npm run ipc:generate
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm run check
npm run lint
npm test
npm run build
npm run format:check
npm run ipc:check
npm run test:browser
npm run test:browser -- --grep "(projects|legacy-import)"
npm run tauri -- build --debug --no-bundle -- --locked
git diff --check
```

425 workspace Rust tests pass (30 external/native tests ignored), including twelve
import-owner tests. Coverage includes actual four-format new-project publication
and reopen, exact original preservation, unchanged attachment/policy, relocation,
old job IDs, one worker/no queue, cancellation before/during IO, post-temporary
source/subtitle/media changes, media-symlink retargeting, competing destinations,
deleted optional-reference names, failure/retry/temporary cleanup, deadline/worker
failure, JS-safe identity exhaustion and shutdown joins. Zero-test harnesses are
not counted as feature validation. The focused final run is `tests-5.log`.

All 44 frontend unit tests and 58 Chromium/WebKit browser cases pass. The import
component uses Rust-generated Serde report samples with modeled IPC; the App
workflow verifies native-video occlusion requests, workspace gates, preserved
unapplied draft, exact output-path opening and both Cancel/Discard restoration
decisions. Controlled session replies cover single-flight polling, stale relocation
reports, late starts/pickers, uncertain publication, final success receipts and
cancelled-worker draining. Builds without playback can create a project while
Open is explicitly unavailable. These are browser/IPC-model checks, not native
window/filesystem/picker/GPU acceptance.

Strict Svelte/TypeScript, ESLint, Prettier, Rustfmt, Clippy, generated IPC drift,
production frontend and default desktop builds pass. The final desktop build
includes the playback-independent import menu gate; no app launch is implied.
The read-only legacy checkout remains clean at the pinned commit above.

Logs remain under `test-results/legacy-project-owner-20261018/`. Failed initial
attempts are retained: the desktop crate has no `--lib` target; two path assertions
initially compared `/var` to macOS's canonical `/private/var`; strict frontend
checks caught optional-property typing and callback control-flow narrowing in the
test harness. Commands/assertions were corrected without changing filesystem
guards. The full browser run includes the final availability change in WebKit;
the subsequent focused run verifies both engines against the final source.

Next: actual native import/relink/retry/source-change/open/history/recovery and
normal shutdown, real project/subtitle/media/output chooser checks, and desktop10k
review/publication/reopen timings using the predeclared fixtures and budgets.
Stage6b/ADR0032 remain in progress/proposed; translation and stages7–8 remain open.
