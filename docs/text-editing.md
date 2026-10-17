# Tag-aware text tools

Open **Find / Replace…** above the cue list. Choose Find, Find and replace, Join
lines or Reflow lines. Resolve an unapplied cue draft with Apply, Discard or
Cancel before creating a preview. Searching and reviewing never change accepted
subtitles. **Go to match** closes the dialog, selects its cue and highlights the
original source range, including correct UTF-16 offsets for Unicode text.
Ctrl/Cmd+F opens Find; Ctrl+H or Cmd+Shift+H opens replacement review. Cmd+H keeps
its ordinary Mac behavior.

Find is literal, across individual cues in document order, with optional case
sensitivity. Case-insensitive search uses Unicode simple case folding. It does
not normalize composed/decomposed characters or equate `ß` with `ss`. Regex and
cross-cue matches are not supported. Replacement is literal too: `$1` has no
special meaning. An empty replacement deletes a safe match. Queries/replacements
are limited to 4096 UTF-8 bytes; replacement excludes markup delimiters
`< > { }`, backslashes, carriage returns and NUL. Edit tags directly when needed.

Balanced `<b>`, `<i>` and `<u>` and complete ASS/SSA override blocks are protected
source tokens. A cue containing a complete override uses the same ASS text
escapes as native preview. Search maps breaks/nonbreaking spaces to readable
characters, but replacements crossing tags or escapes are skipped with a reason.
Unknown overrides, karaoke, drawings, transforms, unsupported HTML and malformed
formatting need manual review. Plain SRT backslashes remain literal. A final
token comparison also rejects replacements that would accidentally form markup.
This source mapping is separate from the historical quality-counting policy.

The preview shows exact original/proposed source, match counts, skipped reasons,
the effective profile and whole-plan quality finding totals. **Replace this
match** accepts one safe occurrence. **Accept Replace All** accepts all safe
replacements across all pages. **Accept text repairs** accepts all safe line
changes. Each acceptance creates one Undo step. Saving is separate; recovery
and resynchronization Save As protections remain in force.

## Formatting and line repairs

Select a span in the cue text editor, then use **Bold**, **Italic**, **Underline**
or Ctrl/Cmd+B/I/U. The shortcut wraps plain selected source with the matching
HTML style; selecting exactly the text inside that wrapper toggles it off. It
updates the draft and restores its selection. **Apply edit** accepts it through
the normal document operation. Empty selections, partial Unicode characters,
tag/escape boundaries and conflicting existing same-style markup are rejected
with an explanation. ASS/SSA overrides remain byte-for-byte intact; a style
already controlled by ASS needs manual tag editing.

Join lines replaces safe physical line breaks/space runs with single spaces only
when the result fits the profile's preferred single-line and maximum width.
Reflow prefers one line within that preference, otherwise a balanced two-line
split where possible, then greedy wrapping up to the allowed number of lines.
Both use UTF-16 source counts under the active quality profile and verify the
resulting stored source against its enabled line limits. They never split words,
nonbreaking spaces or Unicode characters. There is no linguistic or pixel-width
claim. Previewed text may still have other quality findings.

Dialogue/list markers, positioned/animated text, ASS text escapes, blank lines,
tabs, leading/trailing whitespace and breaks crossing formatting boundaries are
conservative skips. A long unbreakable word or text that cannot fit the enabled
line limits is also skipped. Tags are never stripped to construct replacement
text. Full ASS/SSA files/styles/metadata remain outside this SRT workflow.

## Ownership, limits and validation

`vsf-application::text` owns pure token mapping, literal search, formatting and
immutable proposals. `subtitle-core::Edit::TextBatch` validates a complete ordered
set of cue texts without allowing timing/identity changes. The existing history
and application size validator publish the whole operation or restore its prior
state, including saved-state identity and redo. A proposal additionally checks
its full source baseline before creating the edit.

The desktop document owner admits one text worker, with no pending queue and a
10-second cooperative deadline. It captures a document/profile under the mutex,
computes outside it, and guards generation, revision and profile epoch during
publication/acceptance. Accepted edits/history/profile changes invalidate old
results. Shutdown cancels and joins outside the document mutex. Quality and gap
workers remain separately bounded, so at most three such workers can coexist.
All handlers dispatch blocking work away from the UI thread.

The frontend owns dialog inputs, selection, drafts and response ordering. An
input change disables acceptance immediately; late replies cannot revive a
disposed or obsolete preview. Closing during admission cancels its eventual ID.
During acceptance the dialog stays protected until `DocumentSession` reconciles
the Rust snapshot. Accepted changes invalidate the checker and request a fresh
quality check when its view is enabled.

Documents retain the 16 MiB persistence limit. Individual cues above 64 KiB are
visibly skipped by text tools; replacement expansion cannot exceed that cue
limit. Search fails above 100,000 matches rather than applying a partial result.
Review pages contain at most 200 cues/matches; the UI requests 100 at a time.
Formatting selections are bounded and validated before processing.

Run pinned checks from CONTRIBUTING, plus:

```sh
cargo test --locked -p subtitle-core -p vsf-application
cargo test --locked -p visualsubforge document::texts
npm run test:browser -- --grep 'workflow: text-editing'
cargo build --locked --release -p vsf-application --example text-benchmark
python3 scripts/text-benchmark.py
npm run build:desktop-test
npm run test:desktop -- --performance
```

The predeclared 10,000-cue/~1 MiB release budget is p95 <500 ms over 20 warm
runs per operation and <96 MiB additional sampled process RSS. On the development
Mac, the final 940,000-byte fixture passes: Find 16.600 ms, Replace 38.144 ms,
Join 24.974 ms and Reflow 26.183 ms p95; additional RSS 30,883,840 bytes. All
three transforming workloads change 10,000 cues. RSS uses sampled `ps`, not
exhaustive allocation accounting. Receipt: ignored
`tests/fixtures/generated/text-editing/benchmark-final.json`.

Desktop interaction/cancellation/invalidation have a separately declared
one-second budget. The real Mac debug application passes with 10,000 cues:
849 ms whole preview, 114 ms atomic acceptance, 2 ms p95 paged IPC, 1 ms draft
interaction, 4 ms cancellation and 14 ms accepted-edit invalidation. The run
checks 100 visible review rows, acceptance from a later page, complete Undo,
single-match replacement, join/reflow, formatting drafts, failed saves, restart,
recovery and normal shutdown. Retained receipt/screenshots are under
`tests/fixtures/generated/text-editing/desktop-final/` (original run
`test-results/desktop-KrVREy`). The release and desktop fixtures/builds differ;
their durations measure different operations.

Final pinned checks pass: 218 Rust tests (including exact BOM/CRLF batch
save/reopen and active-worker shutdown), 39 frontend unit tests, 40 browser
workflows, strict Clippy/lint/type/format/IPC-drift checks and native-media
compilation. Logs are in the ignored `text-editing/` evidence directory:
`workspace-final.log`, `clippy-final.log`, `frontend-unit-v1.log`,
`browser-v2.log`, `ipc-final.log`, `native-compile-v1.log`, `desktop-v2.log`.
Generated browser/WebDriver input is not physical keyboard, native-dialog, GPU
or audio acceptance. Windows/Linux installation/device gates remain open.
