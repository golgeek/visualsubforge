# Working agreements

## Start each task

- Read [README.md](README.md), [ARCHITECTURE.md](ARCHITECTURE.md), and the relevant feature contract.
- Consult the supported-format and legacy-compatibility guides when reproducing existing behavior. Local research is optional context.
- Keep public status and validation evidence current. State blockers and unverified assumptions explicitly.
- This is a learning project: favor clear, idiomatic code and small reviewable changes. Explain significant ownership, concurrency, and state-management choices.

## Boundaries

- Keep subtitle domain logic independent of Tauri, frontend DTOs, filesystem access, and native media libraries.
- Keep native media integration behind a narrow media boundary. Document thread affinity, ownership, and cleanup for native resources.
- Keep Tauri handlers thin: translate inputs and outputs and delegate application operations.
- Rust owns accepted document state and undo history. The frontend owns transient interaction state; document how edits are committed and reconciled.
- Do not do blocking IO or media analysis on the UI thread.
- Do not send decoded video frames through ordinary Tauri commands or events.
- Create shared helpers for demonstrated needs. Prefer descriptive modules over a general-purpose `utils` bucket or speculative framework.
- Keep the legacy checkout unchanged and optional. Record intentional behavior differences.

## Rust and frontend conventions

- Represent time units and cue identifiers explicitly. Keep precision and rounding decisions at documented boundaries.
- Return meaningful errors for expected failures. Avoid panic-based handling for user input and recoverable IO failures.
- Limit `unsafe` to audited native integration code with safety comments and lifecycle tests where practical.
- Use strict TypeScript and idiomatic Svelte state management. Keep timeline calculations independently testable.
- Route frontend/backend communication through one typed client boundary.
- Use cancellation and stale-result protection for asynchronous media and document operations.

## Validation and documentation

- Test behavior and invariants: undo restoration, format round trips, time mapping, failed-save preservation, and relevant edge cases.
- Run the checks appropriate to the change. Record exact commands and results, including checks that could not run.
- Use the pinned toolchains and checks in CONTRIBUTING.md. Distinguish zero-test harness checks from feature validation.
- Update `ARCHITECTURE.md` in the same change when implemented ownership or dependencies change. Explain proposed changes, significant choices and validation in the PR description.
- Do not turn an untested prototype choice into an accepted architecture decision.
- Keep decision records, task histories and legacy research local; do not add them to the public repository. Public instructions and documentation must work without them.
- Keep instructions concise and avoid repeating policies across files. Add local `AGENTS.md` files only when actual area-specific guidance is needed.
