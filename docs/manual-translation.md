# Manual reference/target translation

Stage6c is in progress. This is the implementation contract; the desktop workflow
is not available yet. WebVTT and legacy project import are already accepted.

## Intended workflow

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

The planned desktop owner keeps the accepted reference context with Rust's
loaded target, separate from its existing Editor history. An epoch distinguishes
reference changes from target revisions. Reference reads/parsing and query work
run off the UI thread; a bounded, cancellable admission path must reject stale
target generations and replaced reference requests before adoption. The frontend
owns only selected reference rows, paging, visibility and temporary offset/file
choices. It cannot mutate the reference or accept target content independently.

The current pure foundation adds no worker or desktop owner. The exact worker,
IPC, project schema and recovery schema changes must be documented with their
integration and race tests before ADR0033 can be accepted.

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

Next: implement bounded desktop loading/querying, reference adoption and source
guards, typed UI, project/recovery persistence and all remaining acceptance.
Stage6c and ADR0033 remain in progress/proposed.

## Legacy source reference

Read-only inspection at VisualSubSync commit
`71778b9115e57c03e9be85234318c2004ac427b3`: `main.pas` `LoadVOSubs`,
`GetVOText`, `ActionTranslationTemplateExecute`; `TranslateFormUnit.pas`
`TTranslatorCopyOriginal` and template variants; `ProjectUnit.pas` `SubtitlesVO`.
Legacy source uses time-overlap reference text and can create a separate target
while retaining the original as VO. This is source inference, not a new run of
VisualSubSync, and no legacy code was copied. VSF intentionally uses explicit
half-open overlap, an immutable reference snapshot, and format-aware preservation.
