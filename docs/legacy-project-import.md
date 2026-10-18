# Legacy VisualSubSync project import

Stage6b is complete within the recorded Mac scope. File → Import VisualSubSync Project provides an
explicit review, file relocation and publication of a new `.vsfproject`.
The Rust owner, browser workflows, actual Mac native workflows, real choosers and
desktop 10k acceptance pass.
WebVTT is already accepted; translation
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
Stored positions preserve legacy milliseconds. Normal project restoration can
quantize the displayed position to a decoded frame and adjust the viewport for
playback following or media bounds; subsequent project saves retain that current
view. Raw WAV playback restores, but waveform generation currently refuses the
fixture's unknown container start time. This is retained for stage8 reliability.

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

## Ownership and validation boundaries

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

## Native workflows and real macOS choosers — 2026-10-18

Same pinned Mac toolchains and existing private mpv runtime; no runtime update.
The native test config copies `tests/desktop/tauri.test.json`, changing only the
identifier to `com.golgeek.visualsubforge.legacy-import-native-20261018`.
The picker config contains only the identifier override
`com.golgeek.visualsubforge.legacy-import-picker-20261018`; it has neither
desktop-e2e nor global Tauri access. Build and launch commands:

```sh
source "$HOME/.nvm/nvm.sh" && nvm use --silent
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype \
  --config "$PWD/test-results/legacy-import-native-20261018/tauri.json" -- --locked
ditto target/debug/bundle/macos/VisualSubForge.app \
  test-results/legacy-import-native-20261018/VisualSubForgeLegacyImport.app
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  VSF_LEGACY_IMPORT_CAPTURE=1 VSF_LEGACY_IMPORT_CAPTURE_LABELS=import-audio-restored \
  node tests/desktop/run.mjs --legacy-import \
  --binary "$PWD/test-results/legacy-import-native-20261018/VisualSubForgeLegacyImport.app/Contents/MacOS/visualsubforge"
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app --features media-prototype \
  --config "$PWD/test-results/legacy-import-picker-20261018/tauri.json" -- --locked
ditto target/debug/bundle/macos/VisualSubForge.app \
  test-results/legacy-import-picker-20261018/VisualSubForgeLegacyImportPicker.app
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  "$PWD/test-results/legacy-import-picker-20261018/VisualSubForgeLegacyImportPicker.app/Contents/MacOS/visualsubforge"
```

The native lane supplies chooser results and DOM events but uses real WebView,
Rust, files and playback. Final `desktop-PSOVDS` passes all four formats and normal
code0 shutdown. It independently authors UTF-8/BOM/UTF-16LE/BE wrappers and media;
checks explicit foreign-Windows relocation, Unicode references, literal warnings,
cancelled/failed/competing destinations and stale project/subtitle/media inputs;
and verifies that publication leaves the existing draft, accepted document,
Undo/Redo, media session, policy and project bytes unchanged. Explicit Open uses
unsaved-work protection, restores native preview and permits exact edit/history/
Save As/reopen. Crash recovery requires a matching checkpoint and preserves an
external source edit through failed Save As, followed by successful separate-file
save. Real IPC cancellation drains before another review is admitted.

WAVFile-only import also restores its audio track and paused 1.250s position.
The independent native image exposes **“Container start time is unknown”** instead
of a waveform. Import/reference restoration passes; waveform generation and
speaker output were not accepted by that earlier case. The subsequent
[stage8 correction and complete native rerun](native-reliability.md) validate the
raw-WAV waveform; speaker-output acceptance remains separate.
For video, initial project JSON retains 1.501s and a 1–6s waveform interval;
following displays 0–5s and frame quantization displays 1.500s after Open.

Two failed harness attempts remain with their logs. `desktop-1Miw9g` acted on a
backend-loaded media state before frontend readiness, so Save Project correctly
did nothing. The harness now waits for enabled playback and reconciled UI state.
`desktop-I8kvsr` matched an older ASS recovery checkpoint before killing the SSA
process. The wait now requires exact path, format, generation, revision and full
accepted bytes. Neither correction changed product code. Valid independent native
images from the second run remain evidence only for their observed states: SRT
review/notice layout and restored red ASS title with CJK. Runtime receipts for
PIDs78097/78152 each identify 46 backing images, none unavailable, including the
expected private libmpv and exact executable. The third run separately establishes
the complete workflow pass; its audio image does not have a per-PID runtime receipt.

The separate picker trial uses actual File/Open/Save sheets, not the test chooser.
Source cancellation/retry and file filtering, explicit media/Unicode SRT
relocation, destination cancellation with no output and successful retry all pass.
Independent checks before Open confirm relative references, exact stored position/
viewport/default policy, no scene cache and unchanged SHA256 hashes for all three
original inputs. Open restores native top-aligned italic CJK, populated waveform
and paused video. Cmd-Q exits normally with code0; originals remain unchanged.
PID80262's runtime receipt records 46 backing images, none unavailable, with both
expected executable and private libmpv verified before quit. Normal project autosave
then records the current 1.500s/0–5s view, distinct from the captured initial JSON.

Executable SHA256 identities:

- Native workflow: `83113ded46ca521c500ecf54a76f4556a742246f566031d8150efc0dc62ea9e9`.
- Real chooser: `2aa6083b24b0b81dfe101a6b3ddc640a1650bad4de7e3bfbd9e9c8444f075781`.

Runtime inspection uses `python3 scripts/record-native-runtime.py --pid PID
--expect /absolute/app/executable --expect /absolute/private/libmpv.2.dylib
--output receipt.json` while each observed process is alive. Build/run logs are in
`test-results/legacy-import-native-20261018/`; workflow JSON, original/output
hashes and images are in the named desktop directories. Real picker source/input
hashes, pre-open project JSON, runtime identity, native images and `receipt.json`
remain in `test-results/legacy-import-picker-20261018/`.

Both native builds, Node syntax, ESLint, Prettier and `git diff --check` pass.
No product code changed in this acceptance increment, so the prior 425 Rust,
44 frontend and 58 browser results remain the implementation evidence, not new
runs. Desktop10k review/publication/reopen/cancel/stale measurements remain next;
stage6b/ADR0032 are still open. No Windows/Linux chooser, installation or physical
GPU/audio acceptance is inferred from these Mac checks.

## Desktop 10,000-cue acceptance — 2026-10-18

Stage6b/ADR0032 are accepted after the following final measurements, alongside
the parser/owner/browser/native/chooser evidence above. Task023 remains open for
manual translation. This does not close the raw-WAV waveform issue assigned to
stage8 or the separate physical Windows/Linux release gates.

The first scale run, `desktop-6kUc9n`, stopped at a harness assertion that omitted
the UI's localized thousands separator. The assertion now uses the same locale
and normalizes layout whitespace; full accepted cue arrays still establish the
10,000 count. `desktop-pLucrE` then completed every behavioral check but failed the
unchanged performance gate: styled project open/reopen took 1440/1656ms and timed
WebVTT took 1354/1549ms. Neither result is counted as an acceptance pass.

Sampling the debug inspection executable showed substantial time in generic
multi-character Unicode searches for ASCII record delimiters. Individual ASCII
separator searches now validate cue metadata and WebVTT payloads; WebVTT's line
iterator scans bytes until CR/LF, whose offsets are necessarily UTF-8 boundaries.
No validation, fresh file read or parse is skipped. No cache, worker, dependency,
accepted-state owner or project-restoration protocol is added. Existing BOM,
CRLF/CR/LF/mixed-ending, Unicode, timed-marker, structural and rollback cases pass;
a new metadata regression covers Unicode retention and boundary injection.
Debug styled inspection p95 changed from 403.025ms in the sampled run to 135.229ms
after the change; these diagnostic runs are separate from release CPU/RSS gates.

Exact commands, in addition to the fixture generation documented above:

```sh
cargo build --locked -p vsf-application --example legacy-project-benchmark
target/debug/examples/legacy-project-benchmark
# Diagnostic baseline sampled the running example, before the scan change:
sample 83287 8 1 -file test-results/legacy-import-scale-20261018/debug-inspection-sample.txt
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
source "$HOME/.nvm/nvm.sh" && nvm use --silent
node --check tests/desktop/legacy-import-performance.mjs
npm run lint
npm run format:check
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype \
  --config "$PWD/test-results/legacy-import-scale-20261018/tauri.json" -- --locked
ditto target/debug/bundle/macos/VisualSubForge.app \
  test-results/legacy-import-scale-20261018/VisualSubForgeLegacyImportScale.app
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  node tests/desktop/run.mjs --legacy-import-performance \
  --binary "$PWD/test-results/legacy-import-scale-20261018/VisualSubForgeLegacyImportScale.app/Contents/MacOS/visualsubforge"
cargo build --locked --release -p vsf-application --example legacy-project-benchmark
python3 scripts/legacy-project-benchmark.py
git diff --check
```

The scale config copies `tests/desktop/tauri.test.json` and changes only the
identifier to `com.golgeek.visualsubforge.legacy-import-scale-20261018`. Native
build passes in 38.13s; executable SHA256 is
`6df0772026d8a95acadd0ae968ee947409bfe12a2c7e0685e96f1a027b2b019c`.
426 workspace Rust tests pass, 30 external/native tests remain ignored, and strict
Clippy/Rustfmt/ESLint/Prettier plus the build's Svelte/TypeScript checks pass.
Zero-test harnesses are not feature validation. The IPC contract and frontend
behavior are unchanged; their prior browser/unit coverage is not reported as a
fresh run.

Final `desktop-sTWABd` passes all four exact fixture hashes used by the library
benchmark. Each workload performs five actual GUI reviews/new-project writes,
checks unchanged active state and original bytes, then restores all 10,000 cues,
edits with Undo/Redo, saves exact expected bytes to a separate file, reopens the
original project references, crashes after an exact checkpoint and recovers/saves
the full document. Cancelled work drains before another review starts. A changed
original produces Stale and no output, preserving the changed bytes; the harness
then restores its own controlled fixture. Source/legacy/media hashes and complete
cue comparisons verify preservation. Normal application shutdown is code0.

| 10,000-cue input | Project open | Project reopen | Recovery | Maximum publication, 5 cycles | Cancel drained |
| --- | ---: | ---: | ---: | ---: | ---: |
| Plain WebVTT | 550ms | 412ms | 239ms | 413ms | 73ms |
| Styled WebVTT | 757ms | 730ms | 406ms | 415ms | 130ms |
| Timed WebVTT | 701ms | 831ms | 373ms | 400ms | 139ms |
| Styled ASS | 612ms | 605ms | 422ms | 418ms | 163ms |

Across all cases, dialog/close <=16ms, review-start acknowledgment <=6ms,
full review <=406ms, edit <=253ms, Undo/Redo <=61ms, save <=170ms,
Cancel acknowledgment <=12ms and stale-input rejection <=14ms. Every measured
interaction remains below 1000ms; background review retains its 10-second limit.
Report/DOM rows stay bounded. Supplied DOM events and file choices measure actual
application work, not physical input or native chooser latency. The small video
exercises native project restoration, not physical GPU/audio output acceptance.

The rebuilt release library benchmark passes p95 9.983/16.684/15.180/37.548ms
for plain/styled/timed/ASS and +48,267,264 sampled RSS bytes across 118 samples,
within the unchanged 500ms/96MiB gates. CPU/RSS sampling runs separately from the
desktop measurements. Logs, failed attempts, profile and `benchmark.json` remain
under `test-results/legacy-import-scale-20261018/`; the named desktop directories
retain raw timings, generated inputs, exact hashes, images and shutdown receipts.
