# Manual reference/target translation

Stage6c is in progress. The pure foundation and Rust desktop owner/IPC/recovery
and project persistence are implemented, with the user-facing reference panel.
Actual Mac native workflows and real choosers pass; combined scale acceptance
remains pending. WebVTT and legacy project import are already accepted.

## Workflow

Open **View → Translation Reference** or the **Translation reference** button
above the cue list. Attach/replace a reference for the current target, or choose
**Start target copy…** to make an independent target from a selected source file.
Copying asks to save/discard/cancel current work before replacement. Reference
attachment/offset/detachment preserve unapplied text, timing and style drafts.
The panel and Problems share the available side-pane space.

Choose **Matching cues** for the accepted target selection or **Browse all** for
independent paging. Select a reference row to inspect its raw and reading text;
the view never interprets source markup as HTML. An explicit signed integer offset
uses milliseconds. Closing the panel cancels its reading work without detaching
the reference. Refresh restarts a cancelled/failed query. Failed reference changes
stay visible while background reading resumes.

Open an existing target and attach a read-only reference, or start a translation
from a separate copy of a reference. Both documents may be SRT, ASS, SSA or WebVTT.
Starting a translation copies the complete source format, text, timings, styles,
attachments and unknown records; the user then translates the target manually.
It requires a separate output and never overwrites the reference. Blank/tag-only
templates and automatic translation are not part of this initial manual workflow.

The reference panel shows original text and a plain-text reading projection with
markup/preview limits stated. It never executes HTML, CSS, ASS tags or scripts.
The target continues to use its existing editor, quality tools and native preview.
Target text/time/style edits and batch operations keep the existing atomic Undo/
Redo behavior. Attaching, replacing, detaching or offsetting a reference changes
the translation context, not subtitle content or target history.

Matching uses positive overlap between half-open intervals `[start, end)`.
An endpoint touching the next cue is not a match. A signed reference offset maps
`reference time + offset` to target time; it does not rewrite either document,
retime embedded effects or claim an automatic synchronization. Negative mapped
times remain negative, rather than being clamped into an artificial match.
Multiple matches are listed in time/source order with original cue numbers.
No match is explicit; rows are never paired solely by position or silently
replaced by a nearest cue. Users can browse the reference independently.
Matching follows accepted target timing; an unapplied target draft is labeled.

Ordinary subtitle saves contain only the target's original format. A VSF project
persists reference location, expected content identity and matching offset.
Missing or changed references require explicit relocation/reload; project restore
must validate references before discarding current work. Accepted-state recovery
also retains the captured reference content and offset, so recovery does not
depend on a file that has disappeared or changed. Recovery keeps its explicit
Save As and external-conflict protections. An unattached subtitle file alone does
not carry a reference relationship; save a project to reopen the workflow.

## Ownership and boundaries

Pure application `translation` operations retain an immutable reference and
indexed interval queries, exact original bytes and format-aware text projection.
They have no filesystem, Tauri, frontend or native media dependencies. Format
validation continues through the existing bounded subtitle reader/parser.

The desktop owner now keeps accepted reference context with Rust's loaded target,
separate from its existing Editor history. Context consists of the immutable
reference, signed offset, copied-source provenance and a project-dirty flag.
An epoch distinguishes reference changes from target revisions. Context-only
changes neither manufacture subtitle edits nor discard target Undo/Redo. A copied
target starts unsaved and requires Save As; copied-source protection survives
Undo, reference detachment and subsequent saves for that loaded target's lifetime.

One additional reference worker handles load preparation and paged reading/matches,
with no queue. It owns immutable captures only and never locks or retains Store.
Store owns admission, job identity and a cancellation flag; generation, revision
and reference epoch guard every adoption and result. Queries capture accepted
target intervals, never a frontend draft. Input reads retain the existing regular
file/16 MiB limits. Cancellation/deadline checkpoints surround bounded IO/parser
phases and scan rows; the ten-second deadline is cooperative, not an OS syscall
interrupt. Normal shutdown takes the handle under the document lock, then joins
outside it. A one-shot completion channel also lets shutdown await reserved
acceptance IO outside the lock; its sender is dropped on every return path.
Owner destruction has a final cancellation/join fallback.

Acceptance reserves one slot, rechecks the selected path/canonical destination
and exact bytes, and constructs an independent target off-lock. It then rechecks
document/context/job identity, cancellation and discard permission before one
locked assignment. No external filesystem lock is claimed across adoption: the
accepted immutable snapshot and fingerprint identify the captured version.
Queries return at most 200 rows, with 1 KiB UTF-8-safe raw/reading excerpts and an
optional selected row capped at 64 KiB. Truncation and projection limits are
explicit. Source text always remains complete in Rust and in recovery.

The thin typed `translation_action` command delegates to blocking application
work; generated contracts and `client.ts` expose its actions/replies. The frontend
TranslationSession owns only paging, pending-operation state and reply epochs.
It coalesces desired queries, serializes query work and drains cancelled jobs
before reference changes. Late starts are cancelled; stale/disposed pages are
never adopted. App gates reference mutations through its workspace operation and
protects target replacement. DocumentSession serializes acceptance with document
commands; context replies replace only the accepted context while keeping drafts.
A target copy returns a new generation through the same document reconciliation.
After an acceptance error, a status refresh reconciles potentially committed Rust
state, preserving drafts when the target version is unchanged. This covers a lost
reply without blindly retrying the mutation. No new Rust worker/dependency is added.

Recovery v5 retains exact reference text, format, SHA-256, paths, offset and copy
provenance. Restore validates all data without reading the original reference.
The checkpoint key includes context epoch/dirty state, so an old prepared write
cannot cross a reference-only change. Existing recovery still requires Save As.
The record envelope is 300 MiB: three independently bounded 16 MiB subtitle texts
can each expand sixfold as JSON, plus metadata. Oversized/corrupt preparation
preserves the previous checkpoint. Older recovery versions remain readable.

Target Save/Save As and the final conversion commit protect current and copied
reference paths, including deleted provenance and case/normalization aliases.
Project v2 persists the expected reference path, format, SHA-256, signed offset and
copied-source provenance. It also permits a saved target without media; existing
v1 media projects remain readable and retain their format when no new fields are
needed. A reference-only relationship still requires a saved target.

Project preflight loads an immutable reference before unsaved-work protection.
Missing/changed references return an issue with no usable attachment token; the
user can cancel, choose a replacement, or explicitly reload changed contents.
Relocation retains identity checking and changed reload marks the context dirty.
Attachment compares the media selection, target path/generation/revision and
reference epoch, then transfers the captured context under document→project lock
order. It does not reread the reference after the user discards current work.
Projects retains no second reference snapshot after successful attachment.

Project publication verifies that the current reference still matches its
captured bytes and protects both current and copied origins. The existing
media→document→project publication lock order also guards an empty media selection
and its session. Only successful publication clears context dirty state; failures
and conflicts retain it. Full contents remain in recovery, not project JSON.

The frontend resolves reference issues before asking to discard drafts, keeps
native video occluded during decisions, and refreshes accepted context after
attachment/save. Saving current context invalidates a staged project token, so
Open revalidates its next project before replacement. Context-only changes prompt
for project Save/Discard/Cancel on replacement and quit. Discard on quit skips
automatic project publication. Subtitle-only projects skip media/scenes/seek and
work in builds without playback.

ADR0033 remains proposed until the complete native/chooser/scale acceptance passes.

## Acceptance

- All four source formats, mixed reference/target formats, Unicode and fansub
  ASS-in-SRT retain original bytes and metadata. Copying a reference creates an
  independently editable target without changing its source snapshot.
- One-to-many overlaps, unsorted input, equal endpoints, gaps, signed offsets,
  large times, nested intervals and paging are deterministic. Offset changes never
  rewrite tags or cue-relative/absolute timed effects.
- Read-only raw/reading projections distinguish unsupported markup from a faithful
  rendering. Existing format-aware target tools retain their safety checks.
- Attach/replace/detach/start-copy, normal target editing/history/save/reopen,
  projects, recovery, stale loads, cancellation, failures and normal shutdown are
  tested end to end. Source/output aliases and externally changed files remain
  protected. A failed reference operation preserves the current target/context.
- Actual Mac native workflow and real file choosers are required in addition to
  owner/browser tests. Platform/device limitations remain explicit.
- Exact 10,000-cue plain/styled/timed WebVTT and ASS workloads cover reference
  preparation, all-cue matching, dense overlaps, paging and cancellation. Existing
  library gates remain <=500ms p95 and <=96MiB additional sampled RSS. Desktop
  interactions remain <1000ms, with ten-second background deadlines. Measure
  project/recovery and source protection with the extra reference state present.

## Foundation implementation and validation — 2026-10-18

`Reference::parse` uses the existing 16 MiB UTF-8/format validation, retains exact
bytes and a SHA-256 identity, and builds a sorted cue index with prefix maximum
end times. A cloned `Reference` shares immutable data through `Arc`; it does not
clone an editor or create another history. Queries shift the target interval into
reference coordinates, skip impossible ranges using binary search, then retain
positive overlaps. Prefix maxima keep long/nested cues from being skipped.
Worst-case dense queries still scan all reference cues, with cooperative
checkpoints and at most 200 returned rows. Parsing and sorting have checkpoints
between bounded phases, not interruption inside the format parser or sort.

The caller supplies accepted target times; returned rows borrow original cues.
`reading` uses the existing format-aware markup projection and rejects previews
over 64 KiB without losing original text. `copy_target` returns an independent
document plus immutable source properties for the existing `Editor<Source>`.
No filesystem operation, worker, IPC command or accepted desktop owner is added
by this increment. Desktop publication must still enforce separate outputs.

`cargo test --locked -p vsf-application --lib translation` passes eight focused
tests: four-format immutable references and independent target Edit/Undo/Redo,
half-open/nested/unsorted matching, signed offsets, an independent interval oracle,
dense 10k paging and cooperative cancellation, invalid input, format-aware reading
and exact byte identity. `cargo fmt --all` passes after correcting a cast/comparison
parse ambiguity and a Unicode byte-string test fixture. The initial failures and
passing output are retained in `test-results/translation-foundation-20261018/`.
Final Rust 1.98.1 workspace validation passes **434 tests, 30 ignored**; eight
zero-test harnesses are not feature evidence. Strict Clippy, Rustfmt, Prettier
and ESLint pass. Node 24.21.0/npm 11.19.0 were selected through the pinned nvm
configuration; benchmark sampling used Python 3.14.7.

Reproduction from the repository root (generated fixtures already present for
this run; regenerate with `python3 scripts/webvtt-fixtures.py` and
`python3 scripts/ass-fixtures.py` if needed):

```sh
cargo test --locked -p vsf-application --lib translation
cargo build --release --locked -p vsf-application --example translation-benchmark
python3 scripts/translation-benchmark.py
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
source "$HOME/.nvm/nvm.sh"
nvm use --silent
npm run format:check
npm run lint
```

Five release workloads each use one warm-up and 20 measured iterations. Input
generation/reads precede the RSS baseline; reference parsing/indexing, all-cue
reading/browsing, matching, independent target edit/serialize/reopen/Undo/Redo
and exact source preservation are measured. Four pinned disjoint fixtures query
every target cue and require exactly its own reference cue. The dense fixture
checks all 50 pages of 10,000 overlapping tagged-SRT cues and mid-scan cancellation.

| 10,000-cue workload | Exact SHA-256 | p95 / maximum ms |
| --- | --- | --- |
| Plain WebVTT | `337cffd83d4ad10fc053bd565f96b4cbc77fd7c817db39fc40e6d254a5d09198` | 32.596 / 33.410 |
| Styled WebVTT, BOM/CRLF | `783a7ad89406073e0cb6512c9ca9daaac9b7c09baeaa933dada2bd0e67aa339f` | 71.522 / 71.748 |
| Timed WebVTT, mixed endings | `46430c8e7b374e2bd6ccb3d4bd95cf3489d2571f00ff8c9120c9c59ce5e1c1e6` | 66.085 / 66.089 |
| ASS | `00e4deb1210c47e2943c30744da0e5d0512a57ba159b659d9dd16ef5d47c1c0c` | 86.800 / 87.887 |
| Dense tagged SRT | `ec3e642d86941c902dd4cb995ebc02fcc55ec2f9bcb3f8b53ac62b6d8dc76f05` | 28.851 / 29.057 |

`benchmark.json` records all results and 379 RSS observations: baseline 9,093,120,
peak 108,969,984, increase **99,876,864 bytes**. The 500 ms p95 and 100,663,296-byte
(96 MiB) additional-memory budgets pass. Memory headroom is only 786,432 bytes;
sampling is not exhaustive allocation accounting, and desktop integration must
measure its full concurrent target/reference/recovery state separately. No budget
was relaxed. The benchmark excludes desktop latency, project/recovery IO, file
choosers and native rendering. No frontend/IPC behavior changed, so browser/native
feature acceptance remains pending rather than inferred from these tests.

Subsequent owner implementation is recorded below. Typed UI, project persistence
and all remaining native/scale acceptance are still required.
Stage6c and ADR0033 remain in progress/proposed.

## Desktop owner increment — 2026-10-18

Twelve focused translation/related conversion tests pass: four-format copies and
history, attach/offset/detach, mixed-format queries, bounded Unicode previews,
invalid inputs, failed/stale loads, external byte/symlink changes, cancellation
during acceptance, single-worker admission and off-lock shutdown, source alias/
deleted-name protection, failed-save preservation, four-format recovery with a
missing reference, corrupt hash/format/offset/path/version refusal, and background
reference-only checkpointing plus normal cleanup. A delayed conversion test
attaches a reference after temporary output preparation; its final commit refuses
the newly protected path even after that reference file is removed.

The signed IPC boundary additionally tests exact positive/negative integers and
rejects fractional/lossy values. Generated Serde/TypeScript samples include signed
mapped times, context summaries, paged raw/reading text and all translation actions.
The owner preserves original source bytes and the existing target editor/history.

Evidence: `test-results/translation-owner-20261018/`. Rust 1.98.1,
Node 24.21.0/npm 11.19.0. Commands:

```sh
cargo check --locked -p visualsubforge
cargo test --locked -p visualsubforge translation
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
source "$HOME/.nvm/nvm.sh"
nvm use --silent
npm run ipc:generate
npm run ipc:check
npm run check
npm test
npm run build
npm run format:check
npm run lint
npm run tauri -- build --debug --no-bundle -- --locked
```

The first check/test attempts exposed two missing closure result annotations;
their logs are retained as `check-1.log`/`tests-1.log`. Correcting the annotations
changed no acceptance policy. Final workspace validation passes 447 Rust tests /30 ignored; eight zero-test
harnesses are not feature evidence. Owner tests, strict checks, generated IPC, 44 frontend
unit tests and the default desktop build pass. These are not browser/native
translation acceptance: no panel, normal chooser, complete project restore or
combined target/reference/project/recovery 10k measurement has run yet. The pure
benchmark above is a baseline only, with its memory headroom still a concern for
the complete workflow. Next is project persistence and the user-facing workflow,
including draft/exit protection, then actual native and full scale validation.

## Project persistence increment — 2026-10-18

Five new backend cases cover four-format subtitle-only round trips, unchanged
target history, signed offsets and portable provenance; changed/missing/relocated
references; captured-source restoration and stale media/target/revision/epoch
rejection; failed publication retaining dirty context; successful publication
clearing only context; malformed versions/metadata and deleted-source protection.
Generated Serde/TypeScript samples now include subtitle-only translation projects
and changed-reference issues.

Browser project coverage adds preflight cancellation and focus, explicit changed
reload/relocation, preserved drafts, context-only quit Cancel/Discard/Save,
restaging after saving current context, and subtitle-only open/save with playback
enabled and disabled. Lifecycle coverage includes disposal during accepted-context
refresh. These use production components with modeled IPC, not native filesystem,
chooser or playback acceptance.

Evidence: `test-results/translation-project-20261018/`. Rust 1.98.1,
Node 24.21.0/npm11.19.0. Commands:

```sh
cargo check --locked -p visualsubforge
cargo test --locked -p visualsubforge
cargo test --locked -p visualsubforge project::translation_tests
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
source "$HOME/.nvm/nvm.sh"
nvm use --silent
npm run ipc:generate
npm run ipc:check
npm run check
npm test
npm run build
npm run format:check
npm run lint
npm run test:browser -- --grep projects
npm run test:browser
npm run test:browser -- --grep 'projects|workspace-lifecycle'
npm run tauri -- build --debug --no-bundle -- --locked
```

All 452 workspace Rust tests pass /30 ignored; eight zero-test harnesses are not
feature validation. Strict Clippy, Rustfmt, generated IPC, Svelte/TypeScript,
Prettier, ESLint and 44 frontend tests pass. The final default desktop build passes
in 5.49s; it was not launched. Six initial targeted browser cases pass, each project
harness executing 50 checks. The wider run passes 57/58 cases; its Chromium lifecycle
fixture had not initialized playback availability before opening a media project.
The corrected fixture also expects the new context refresh and checks disposal
during it; final targeted results are recorded below. The failure log, screenshot
and trace remain in `retained-lifecycle-failure/`. Initial new Rust test compilation
attempts accessed private fields; the tests now inspect serialized snapshots
without widening production visibility. Those attempts remain in
`translation-tests-1.log` and `translation-tests-2.log`.

Final targeted rerun passes all eight Chromium/WebKit project/lifecycle cases
in 48.4s (`browser-projects-lifecycle-2.log`): 50 project checks and 25 lifecycle
checks per ordinary harness, plus delayed-registration and scene-notice cases.
The other wider-suite cases already passed; no unrelated failure remains.

The translation panel/start-copy entry point and actual native/chooser/combined10k
acceptance remain pending. Project persistence does not close stage6c or ADR0033.
No native runtime, physical-platform acceptance or performance limit changed.

## Reference panel increment — 2026-10-18

The View menu and inline panel expose attach/replace/detach, independent target
copy, signed offsets, accepted-time matching, bounded 100-row paging and selected
raw/reading views. Target draft/history ownership remains in DocumentSession/Rust.
Browser coverage includes markup rendered as literal text, draft preservation,
copy Cancel/Discard, failed load preservation, cancellation and source retention.
Controlled lifecycle cases cover stale accepted projections, lost context/copy
replies, serial latest-selection queries, cancellation before load dispatch and
disposal before a job identity arrives.

Evidence: `test-results/translation-panel-20261018/`. Pinned Rust 1.98.1 and
Node 24.21.0/npm11.19.0. Commands:

```sh
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
source "$HOME/.nvm/nvm.sh"
nvm use --silent
npm run ipc:generate
npm run ipc:check
npm run check
npm run lint
npm test
npm run build
npm run format:check
npm run test:browser -- --grep workspace-lifecycle
npm run test:browser -- --grep projects
npm run test:browser -- --grep 'projects|workspace-lifecycle'
npm run test:browser
npm run tauri -- build --debug --no-bundle -- --locked
```

The initial panel run exposed operation errors being cleared when background
queries resumed. Separate mutation/query error state fixes the defect without
changing reference or target acceptance. The failed log and representative
screenshot/trace are retained in `retained-reference-error-failure/`. The final
targeted run passes eight Chromium/WebKit cases in 51.5s, with 60 project/panel and 32
lifecycle checks per ordinary harness. The full run passes all 58 browser cases
in 2.7m. All 452 workspace Rust tests pass /30 ignored; eight zero-test harnesses
are not feature evidence. Strict Clippy, Rustfmt, ESLint, Prettier, generated IPC,
Svelte/TypeScript and 44 frontend unit tests pass. The default desktop build passes
in 29.35s and was not launched. Exact logs retain the checks and initial failure.

Actual native Mac workflows, real source/destination choosers and combined
target/reference/project/recovery10k validation remain required; browser fixtures
use modeled IPC and do not establish native acceptance. Stage6c stays open.

## Native desktop workflows — 2026-10-18

The real desktop lane passes on macOS 27.0 (26A428), Apple Silicon, using the
existing private mpv runtime. The renderer reports Apple M3 Max / OpenGL 4.1
Metal 91.7. The test config copies `tests/desktop/tauri.test.json`, changing only
the identifier to `com.golgeek.visualsubforge.translation-native-20261018`.
The copied bundle executable SHA-256 is
`30eca1d12306f9f1fc2a6c35b868d945a7e29ca9d7d690d20cf578b028db8cba`.
Pinned Rust 1.98.1 and Node 24.21.0/npm 11.19.0 were used; no runtime was updated.

```sh
source "$HOME/.nvm/nvm.sh" && nvm use --silent
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype \
  --config "$PWD/test-results/translation-native-20261018/tauri.json" -- --locked
ditto target/debug/bundle/macos/VisualSubForge.app \
  test-results/translation-native-20261018/VisualSubForgeTranslation.app
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  VSF_TRANSLATION_CAPTURE=1 node tests/desktop/run.mjs --translation \
  --binary "$PWD/test-results/translation-native-20261018/VisualSubForgeTranslation.app/Contents/MacOS/visualsubforge"
node --check tests/desktop/translation.mjs
node --check tests/desktop/run.mjs
npm run format:check
npm run lint
```

The native bundle build passes in 43.42s. Final evidence is
`test-results/desktop-boA7rv/`: `receipt.json` and `translation.json` pass, including
normal application shutdown with code0. The lane runs the ordinary persistence/
quality/text regression first, then checks all four formats for lossless independent
copies, failed/protected Save As, target draft preservation across offsets and
browsing, exact text-only edits, Undo/Redo, save/project restoration and preview
revision acknowledgments. SRT additionally checks changed-reference Cancel without
discarding a draft, explicit reload, missing-file relocation and protection of a
deleted original path. Every format is recovered after an abrupt owned-process
termination with the reference deleted and target externally changed; captured
reference bytes/offsets survive, conflicts preserve external bytes, and a separate
target and subtitle-only project save successfully. An ASS target with a WebVTT
reference preserves its format, accepted cues, revision and unapplied draft.

Independent native-window images and accessibility observations are retained for
the four restored previews and mixed-format reference. They show tagged SRT
positioning, ASS/SSA title styles and CJK text, and literal raw/reading text in the
reference pane. Scrolling reaches the raw-text section. WebVTT retains its existing
preview limitations: the emoji is visible in the editor/reference but remains a
missing-glyph box in native playback; this does not establish browser-equivalent
CSS/region/ruby/timed-effect rendering. The generated video contains no audio, so
this lane supplies no speaker/waveform acceptance.

The first run, `desktop-6rVmBA`, stopped on an ambiguous test selector matching two
reference rows. The harness now explicitly selects the first row. Its failure
receipt, screenshot and log remain; no production change was needed. Build/run/
format/lint logs are in `test-results/translation-native-20261018/`; syntax,
formatting and ESLint checks pass. The prior full production regression results
remain above. This increment changes only the desktop harness and documentation.

File selections and DOM input are supplied. Actual OS chooser checks and combined
10,000-cue target/reference/project/recovery acceptance remain required; stage6c
and ADR0033 stay open. Physical Windows/Linux and native-runtime adoption gates
are unchanged.

## Real macOS file dialogs — 2026-10-18

An isolated non-WebDriver bundle passes the actual native file-dialog workflow.
Its config contains only identifier
`com.golgeek.visualsubforge.translation-picker-20261018`; it uses `media-prototype`
without `desktop-e2e` or global Tauri access. Same Mac, pinned toolchains and private
runtime as above. Build passes in 14.15s; executable SHA-256 is
`38ca76f0bf84e98a60f6debf8858c4ccd01088e167a61a8f0c02d236e85d307c`.

```sh
source "$HOME/.nvm/nvm.sh" && nvm use --silent
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app --features media-prototype \
  --config "$PWD/test-results/translation-picker-20261018/tauri.json" -- --locked
ditto target/debug/bundle/macos/VisualSubForge.app \
  test-results/translation-picker-20261018/VisualSubForgeTranslationPicker.app
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  "$PWD/test-results/translation-picker-20261018/VisualSubForgeTranslationPicker.app/Contents/MacOS/visualsubforge"
```

`test-results/translation-picker-20261018/receipt.json` records 19 checks, native
images/AX observations, input hashes and exact saved project snapshots. Actual
Open, Save As, Save Project and Save Project As sheets cover all four subtitle
extensions, accented/CJK paths, source/destination cancellation and successful
separate outputs. After macOS's Replace confirmation, Rust refuses the reference
destination and hashes prove the original files unchanged. The ASS target equals
the original with only the intended text replacement; the SSA target is byte-exact.

Reference replacement preserves a native target draft, including when cancelled;
ASS/WebVTT and SSA/tagged-SRT pairings keep target format and source markup.
Changed-reference Cancel precedes any draft loss; explicit Reload persists the new
identity. Missing-reference Choose uses the actual picker; cancelling it returns
to the unresolved-reference prompt. Choosing the moved file restores the expected
hash/offset, and Save Project persists the new path. Controlled fixture changes
create these scenarios; original fixture bytes are retained separately and restored
after shutdown. Starting another target copy tests both Cancel (preserved draft)
and Discard (independent SSA copy). Separate project publication preserves the
previous project byte-for-byte.

Quit with a changed reference offset offers Save project. Cancel preserves the
in-memory offset and previous on-disk value; Save project writes the new offset
and the owned process exits normally with code0. No other application is stopped.
Native AX/paste/click input is not a physical-keyboard latency measurement. An
initial automated path typing attempt produced incomplete Unicode text; selecting
the exact path through the native text field resolved it before opening any file.
No product change was needed. The earlier production regression remains applicable.

Combined desktop10k validation remains next. Stage6c and ADR0033 stay open until
that passes; plugin and native/distribution milestones are still required.

## Legacy source reference

Read-only inspection at VisualSubSync commit
`71778b9115e57c03e9be85234318c2004ac427b3`: `main.pas` `LoadVOSubs`,
`GetVOText`, `ActionTranslationTemplateExecute`; `TranslateFormUnit.pas`
`TTranslatorCopyOriginal` and template variants; `ProjectUnit.pas` `SubtitlesVO`.
Legacy source uses time-overlap reference text and can create a separate target
while retaining the original as VO. This is source inference, not a new run of
VisualSubSync, and no legacy code was copied. VSF intentionally uses explicit
half-open overlap, an immutable reference snapshot, and format-aware preservation.
