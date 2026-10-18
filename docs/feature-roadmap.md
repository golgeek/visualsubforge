# Feature and reliability milestones

Authorized 2026-10-17. This is the staged execution plan for the next umbrella
goal. [Current status](status.md) remains the authority for shipped behavior and
validation; a row below is complete only when its implementation and acceptance
evidence are recorded. Local task records retain detailed work notes.

## Sequence and acceptance

| Stage | Status | Required outcome and acceptance |
| --- | --- | --- |
| 1. Fonts | Complete; [evidence](preview-fonts.md) | Reproduce generic CJK fallback; improve rendering within the existing native strategy; expose useful missing-font diagnostics. Validate generic, explicit and attachment-backed fonts with original declarations unchanged, replacement/cleanup and bounded transport. |
| 2. Selection and batch timing | Complete; [evidence](batch-timing.md) | Contiguous/disjoint mouse/keyboard selection; reviewed shifts of selected/all/following cues and selection-to-playhead alignment. Exact format ticks, invalid/stale atomic rejection, one Undo/Redo, draft protection and 10k response measurements. |
| 3. ASS/SSA resync | Complete; [evidence](resynchronization.md#assssa-extension-validation--2026-10-17) | Shared GUI/CLI proposals retain metadata/attachments/tags and explain timed-effect handling. Review representable endpoints; no silent rounding or unsafe transformations; source protection, history, save/reopen and tagged-SRT regression. |
| 4. Language tools | In progress | Licensed English/French dictionaries, suggestions and personal/ignored words; optional typography/capitalization/dialogue rules in quality profiles and Problems. Unicode/markup-aware reviewed corrections, cancellation, persistence and undo. |
| 5. Styles, attachments, conversion | Pending | Rust-owned undoable style creation/editing with valid references, attachment inspection and supported font management. Explicit separate-file conversion with a reviewed loss report; exact preservation of unrelated source, recovery/history and failed-write tests. |
| 6a. WebVTT | Pending | Documented supported editing/round-trip subset, explicit unsupported constructs, shared IO/preview/history/project/recovery integration and format fixtures. |
| 6b. Legacy projects | Pending | Documented import subset with missing-reference/unsupported-setting diagnostics, preserved original, safe staging/cancellation and independently authored fixtures. |
| 6c. Manual translation | Pending | Reference and editable target with defined timing relationships, markup protection, save/reopen, project persistence and recovery. |
| 7. Plugin host | Pending | Evidence-backed bounded JS engine choice; useful checker and reversible fix through modern API and documented VSS subset. Limits, cancellation, stale rejection, atomic undo and exception rollback; licensing/provenance and supported API matrix. |
| 8. Native/distribution reliability | Pending | Reproduce retained audio loop/clear, Linux output/clock, gesture and dialog/lifecycle findings on current identified runtimes. Fix confirmed defects within the approved runtime strategy; run available Mac/VM/CI checks; prepare identifiable install/GPU/audio acceptance artifacts and instructions. |

## Completion boundary

Implement and validate the scope above on available environments, with focused
commits and exact commands/results. Preserve the existing domain/media/typed IPC
boundaries and accepted-state owner; record any new ownership decision before
implementation and accept it only after validation. Reuse established fixtures
and predeclare affected performance budgets before measurement.

Unavailable Windows/Linux physical-machine checks remain open release gates.
They do not halt independent implementation, but VM, CI, compilation or software
rendering must never be presented as physical GPU/audio/install acceptance.
Concrete blockers remain visible; documentation alone cannot close a feature.

## Reconciled starting point

Tasks 010–016 already implement the initial resync, quality, text, scene repair,
duration/speed repair, statistics and loss-preserving ASS/SSA milestones. Existing
ASS/SSA styles can be inspected/assigned; authoring, attachment management and
conversion are still absent. Resync is SRT-only. Inline ASS tags in SRT are already
preserved and previewed. Generic CJK fallback failed on the recorded Mac runtime;
an explicit installed-font control passed. Historical task text is evidence for
its dated scope, not a fresh backlog. Packaging task003d is in progress, with
separate native/adoption and physical-device gates.
