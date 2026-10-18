# JavaScript plugin host

Stage7 is in progress. The bounded worker in `crates/plugin-host`, pure
[modern/VSS APIs and validated proposals](plugin-api.md), and
[local CLI check/preview/apply](plugin-cli.md) are implemented and tested on Mac.
The desktop job owner and typed commands are implemented and tested; user-facing
review controls and complete desktop acceptance remain required. ADR0008 is still proposed.

## Candidate execution boundary

The experiment pins `rquickjs`/`rquickjs-core`/`rquickjs-sys` 0.14.0 and the latter's
bundled QuickJS-NG 0.16.2 sources. The binding and engine use MIT licenses. Published
crate checksums are in Cargo.lock. The CLI now links the runtime; its non-dev
dependency closure includes these components. The source collector retains the
published engine notices and uses the existing pinned-upstream fallback for
rquickjs-core, whose published crate omits its root license. A targeted Mac closure
and archive/notice audit passes; complete release inventory remains required.
No legacy script implementation is redistributed by this increment.

The candidate exposes a fresh single-threaded runtime in a short-lived helper.
The parent owns process creation, bounded pipes, cancellation, deadline, kill and
reap. No native engine object or JavaScript value crosses that boundary. This
keeps an engine crash or blocked script out of the editor process, but is not an
OS security sandbox. It does not claim protection from native engine exploits or
an OS-enforced total RSS limit. This initial host is intended for explicitly
installed local plugins, not arbitrary remote code execution.

The [versioned rquickjs API](https://docs.rs/rquickjs/0.14.0/rquickjs/struct.Runtime.html)
provides heap/stack limits and interruption. The default C allocator is retained;
the binding documents that custom allocator features bypass its memory limit.
The `std` feature alone is enabled: no module resolver, native module loader,
async runtime or host file/network/DOM capabilities. The parent clears the helper's
environment, preserving only SystemRoot on Windows for runtime loading, and uses
direct executable/argument invocation. Windows launches without a console window.
The CLI launches its own executable in an early internal worker mode before normal
startup. The desktop owner launches that installed sibling CLI, avoiding UI
libraries in the helper. It rejects missing, non-regular and symlink runners and
never searches PATH. Existing package recipes install the CLI beside the desktop;
the default dev command now builds it. Custom target/profile builds must build both
executables together. Plugin installation/discovery and desktop controls remain pending.

The inspected [Boa 0.22.0 runtime limits](https://docs.rs/boa_engine/0.22.0/boa_engine/vm/struct.RuntimeLimits.html)
cover loop iterations, stack, recursion and backtrace limits. That API does not
provide a corresponding engine heap cap, so QuickJS is the first measured candidate
for this contract. This is an API comparison, not a performance or compatibility
claim about Boa. No Boa dependency was added to the workspace.

## Worker contract and bounds

The experimental protocol accepts one JSON request and returns one versioned JSON
reply, then exits. Version1 contains source, copied input and explicit limits.
Source is a synchronous function body receiving `input`; this is a low-level test
contract, not the final public plugin API. Unknown request/limit fields, invalid
versions and invalid limits are rejected. A failed invocation returns no result.

| Resource | Default | Enforced range/limit |
| --- | ---: | --- |
| Whole invocation deadline | 1000ms | 1–10000ms; parent includes input encoding, process/pipe work and result decoding |
| Engine heap | 64MiB | 8–128MiB, C allocator accounting; not whole-process RSS |
| Engine stack | 256KiB | 64–512KiB |
| JavaScript source | — | 1MiB UTF-8 |
| Request/reply wire message | — | 32MiB each; bounded reads and serialization |
| Script diagnostic / helper stderr | — | 4096 bytes each |

The engine interrupt covers evaluation, callbacks, regex work and JavaScript
serialization hooks. Parent cancellation/deadline can independently kill a helper
that stops reading/writing or returning. Three scoped pipe threads prevent a full
pipe from blocking the watchdog. The child guard is dropped inside the scope:
kill/reap precedes joining pipe threads on success, failure and early return.
No pipe thread or helper is intentionally detached. Ordinary OS process creation,
termination and scheduling latency are not a real-time guarantee.

No promise jobs are executed. Returned promises and queued jobs are rejected;
runtime destruction prevents later side effects. Each invocation gets fresh
globals. Strings retain JavaScript UTF-16 behavior and exact JSON CRLF/Unicode
content. Missing/unsupported capabilities do not receive guessed defaults.

## Validation and remaining acceptance

The Mac worker tests cover copied input, UTF-16/CRLF, global reset, unavailable host
capabilities, exceptions, circular/BigInt/undefined/async results, oversized output,
bounded errors, loops including catch-and-loop, expensive regexp, infinite toJSON
and error getters, recursion, actual allocation exhaustion, and 10k tagged Unicode
cue payloads. Faulty independently authored helpers test blocked input, a valid
reply followed by a hang, stdout/stderr floods, malformed/versioned replies and
crash. Cancellation waits until the helper is observed live; OS PID checks verify
termination/reaping. These are worker guarantees, not document Undo/rollback tests.

Predeclared release benchmark: read-only and proposed-data 10k payloads, one warm-up
and 20 measured fresh helpers each, <=500ms p95 for parent serialization, process/
pipe execution, response decoding and full-row checks; <=192MiB additional sampled
parent+helper RSS above an input-preallocation baseline. The extra process/copies
are included. Existing pure-application 96MiB gates are unchanged. Sampling can miss
peaks and does not establish a hard RSS ceiling.

Required before stage7/ADR0008 acceptance:

- Complete the supported API/behavior acceptance against the pinned VSS host and
  available platforms. The pure modern/VSS adapters, captured package loading,
  settings, neighbor/scene semantics and Rust proposal/history validation now have
  real-helper tests; this does not yet validate the desktop owner's lifetime.
  Unresolved-license scripts are tested locally and are not redistributed.
- Editor discovery, review, adoption, cancellation, close/replace/shutdown,
  save/reopen/recovery and races with existing accepted state. One admitted plugin
  run with no queued helper process; the current low-level `run` function relies
  on its caller to provide that application-wide admission bound.
- Final 10k behavior/performance with real APIs and document integration, native
  Mac workflow/chooser and available Windows/Linux VM/CI validation. Bindings
  present in a crate are not successful platform builds or execution evidence.

General actions, columns, UI hooks, split hooks and broad historical-pack
compatibility remain separately tracked beyond the first rule/fix subset.

## Desktop owner and typed transport — 2026-10-18

`document::plugins` now owns one job/no queue. Capture and final acceptance use
the existing document mutex; package IO, script execution and package verification
do not. Workers own immutable document/source/context copies and cancellation,
never the document owner. Completed proposals remain private in Rust. Cancelling
or replacing the document invalidates results without changing history; profile,
media and scene identity guards cover context changes. Acceptance also compares
the exact invocation settings, scope, selection and cursor supplied by the caller.
The upcoming UI must supply its current transient context and cancel stale starts.

Desktop preferences come from the captured effective profile. An absent CPS target
requires an explicit invocation override; RS target index remains unavailable
unless explicitly supplied. Completed standard/reduced scene results retain their
coverage label. Scene before/after values map to StartOffset/StopOffset; the plugin
filter offset is an explicit invocation value, with no invented legacy preset.
Missing/partial scene results remain unavailable. The VSS offset mapping follows
the previously recorded pinned-source inference in the local legacy reference.

Typed review pages contain at most200 findings and200 changes. Each raw text
preview is at most2048 UTF-8 bytes with an explicit full length/truncation flag.
The complete before/after text remains available in4–65536-byte UTF-8 chunks;
invalid byte boundaries are refused. Rust retains all validated rows regardless
of paging. Effective scalar settings, logs, preferences, coverage and package
identity are returned without exposing engine values or a mutable proposal.

Acceptance reserves the job while revalidating the package off-lock. Only that
successful verification can construct the private value consumed by the commit
method. It rechecks deadline, cancellation, generation/revision/profile key and
complete source/document/request identity, then applies one domain history batch
under the media→scene→document guard. A one-shot completion channel lets shutdown
cancel and wait for in-flight acceptance outside the locks, as it does for worker
joining. Accepted changes flow through ordinary preview, save, Undo/Redo and
recovery; no parallel accepted-state owner or plugin-specific recovery format exists.

Nine focused owner tests pass. Their controlled executor uses the real engine
in-process for deterministic owner/race tests; those cases do not themselves prove
process isolation. They cover modern/VSS fixes across all four formats, original
source properties and cue metadata, one Undo/Redo, failed-save preservation,
save/reopen/recovery, explicit context and neighbor review, long Unicode text,
edit→Undo staleness, replacement/close/profile changes, production media/scene
guards, package revalidation, cancellation and shutdown during acceptance,
single admission, old-job cancellation, errors/panic and redo preservation, and
installation-only runner resolution. A separate opt-in test passes with the real
CLI runner copied into three fresh Unicode/spaced installation directories, using
the production resolver/executor and unchanged1000ms limit. This is a helper and
owner test, not an actual window/chooser or packaged-install acceptance.

All501 workspace Rust tests pass/32 ignored;11 zero-test harnesses are excluded.
All44 frontend tests and strict/generated-IPC/default-build checks pass. The
transport uses finite numeric scalars and safe integer wrappers; actual Serde
input/review/chunk samples compile against the generated TypeScript boundary.
The initial generated sample included a random temporary path; fixing that fixture
makes generation/check deterministic. No browser/native UI workflow or new10k
owner measurement is claimed. The earlier pure/API/CLI scale evidence remains
separate; full desktop10k/history/recovery acceptance is still required.

The initial desktop self-spawn experiment passed once but timed out after a fresh
rebuild at the unchanged one-second parent deadline. A tiny-request probe over
three fresh binary copies measured495–529ms cold desktop launch versus141–195ms
for the CLI (warm second launches13–15ms versus7–10ms). The isolated test worker
measured42–62ms, but distributing another binary is unnecessary: all maintained
package recipes already install/sign/inventory the CLI next to the desktop. The
owner now uses only that sibling CLI in its early helper mode. It does not search
PATH, use a different installation or increase execution limits. The failed
experiment is retained; no self-spawn fallback remains in desktop startup.

Exact commands (Node24.21.0/npm11.19.0 via the standard nvm setup):

```sh
cargo test --locked -p visualsubforge document::plugins -- --test-threads=1
cargo build --locked -p vsf-cli
VSF_PLUGIN_DESKTOP_EXECUTABLE="$PWD/target/debug/visualsubforge" \
  cargo test --locked -p visualsubforge document::plugins::tests::installed_cli_helper \
  -- --ignored --test-threads=1
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm run ipc:generate
npm run ipc:check
npm run format:check
npm run lint
npm test
npm run tauri -- build --debug --no-bundle -- --locked
```

Evidence: `test-results/plugin-owner-20261018/`, including the retained failed
`desktop-helper-final.log`, `startup-probe.json`, `installed-runner.log`, final
workspace/strict/IPC/build logs and `summary.json`. Tested CLI SHA256:
`37195bdbec7627d416089e5dd051b8e8d809ad64c14ff0645f64396a662e8348`.
User-facing controls, draft/lost-reply reconciliation, browser/native/10k workflows,
historical-host comparison and Windows/Linux execution remain required. ADR0008
and stage7 remain proposed/in progress; stage8 and physical-device gates are unchanged.

## CLI and package IO evidence — 2026-10-18

On the same Mac/Rust toolchain, six application IO tests pass: bounded package
traversal, helpers, invalid encodings/counts/sizes/depth, symlinks and FIFO refusal,
exact review identity, cancelled/changed/unreadable inputs, competing output lease,
no-clobber publication and temporary cleanup. These tests inject wire replies;
they do not claim JavaScript execution by themselves.

Eight separate integration tests run the actual `vsf` executable and its internal
helper. Modern/VSS check, preview, deterministic rerun and apply pass for all four
formats, including Unicode/spaces in paths, source labels, raw tags, ASS/SSA
metadata/attachments and WebVTT timed content. Tests cover explicit scene/cursor/
selection/settings context, captured helpers, outside-scope neighbor changes and
markup warnings, invalid timing/precision/timed effects, exceptions after partial
mutation, absent APIs, nondeterministic review refusal, invalid arguments, existing
and aliased outputs, source/package/context/scope/setting changes, live SIGINT and
helper reaping, and source mutation during execution. Unix process/symlink tests
are explicitly platform-gated; this run is Mac evidence only.

All492 workspace Rust tests pass/31 ignored;11 zero-test harnesses are excluded.
All44 frontend tests, strict workspace Clippy, Rustfmt, Prettier, ESLint,
Svelte/TypeScript/Vite, generated-IPC comparison and the default desktop build
pass. There is no new frontend/IPC or native-editor behavior in this increment;
browser/native workflows were not rerun. Windows/Linux plugin execution and full
desktop/recovery acceptance remain open.

Thirty predeclared release workloads cover modern/VSS check, preview and apply on
five exact10k inputs: dense tagged SRT, ASS, SSA, styled and timed WebVTT. Each has
one warm-up and20 measured fresh CLI commands. The500ms p95 budget includes
process startup, package/source IO, helper, validation, full review output or
atomic publication, and final input verification. Worst p95 is333.31024996005ms
(timed WebVTT/VSS apply), with a335.46562504488975ms maximum in that workload.
Sampled totalCLI+helperRSS is120,782,848bytes (115.1875MiB) against192MiB, using
8278 samples including4082 live helpers. There is no input-baseline subtraction.
Every finding/change, original bytes, output SHA256 and output reparse through a
second real CLI check is verified outside the command timer. Sampling adds polling
latency and can miss RSS peaks; it is not an OS memory cap. Existing pure/API gates
are unchanged. The default desktop build overlapped part of this passing run.

Commands (repository root; Node commands use `source "$HOME/.nvm/nvm.sh" && nvm use --silent`):

```sh
cargo test --locked -p vsf-application --test plugin_files
cargo test --locked -p vsf-cli --test plugins
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --locked -p vsf-cli
cargo build --locked --release -p vsf-cli
python3 scripts/plugin-cli-benchmark.py
npm run format:check
npm run lint
npm test
npm run build
npm run ipc:check
npm run tauri -- build --debug --no-bundle -- --locked
```

Receipts live under `test-results/plugin-cli-20261018/`: application/CLI/workspace
test logs, strict/build logs, `benchmark.json` and `source-inventory.json`.
Measured release CLI SHA256:
`860eb7a351f3b0958fe494c995995bf8052cc5adbeb973ee80f5022ddf70ef90`.
The targeted source audit uses `cargo metadata --locked --offline --format-version 1
--filter-platform aarch64-apple-darwin`, the existing non-dev GUI/CLI closure and
archive/notice functions. All three registry archives match Cargo.lock and local
unpacked sources; original nested engine notices and the core crate's pinned
upstream license are retained. This is not a complete release inventory.

Retained failures: the first IO assertion expected `/var` rather than the published
canonical `/private/var` path; the test now compares canonical paths. Initial CLI
tests omitted the required VSS name, mistook a structured helper request error for
a process transport failure, and expected an argument error for a runtime setting
type mismatch. Those test expectations/fixtures were corrected, with the actual
helper protocol and API behavior retained. The first targeted license audit
assumed every crate contained root notices; using the existing pinned-upstream
fallback resolves the missing rquickjs-core notice. No performance budget was
relaxed and no desktop/VM acceptance is inferred from these results.

## Mac foundation evidence — 2026-10-18

On macOS27.0 build26A428 / Apple M3 Max, Rust1.98.1, ten focused worker tests pass
(4.00s initial complete run), including positive controls after every limited or
failed execution. All 462 workspace tests pass, with 30 opt-in tests ignored and 11
zero-test harnesses excluded from feature evidence. Strict workspace Clippy,
Rustfmt, Prettier and ESLint pass. The final protocol-boundary assertions rerun
the ten focused tests (3.95s debug, 2.73s release) and crate Clippy. Release tests
also check the optimized engine’s interruption/allocation bounds. No frontend or accepted-document code
changed; browser/native-editor workflows were not rerun for this isolated crate.
Windows/Linux runtime tests remain unverified. This does not accept ADR0008.

The release runtime benchmark passes, with input SHA-256
`48f502414caf9e404b4db0a6b69d4912a6301d19a09ec782791018eec2a8862d`:

| 10k payload | p95 ms | Maximum ms |
| --- | ---: | ---: |
| Read-only | 37.309 | 37.599 |
| Proposed data | 46.512 | 46.985 |

There are 76 RSS snapshots, 60 with a live helper. Parent baseline is 19,791,872
bytes; combined peak 73,367,552 bytes; increase 53,575,680 bytes versus the declared
201,326,592-byte limit. These are the complete synchronous parent/worker functions
and all-row validation, but no subtitle-domain proposal validation, history,
format serialization, UI or actual legacy compatibility is included yet.
Worker SHA-256 is
`4fd565d517242fed583e692b085b46357228edbce17c8916a8ebe350cac28d82`;
benchmark binary SHA-256 is
`16b46109beea93c5c057f431ca5e4cca680ec6e3b7e133200c09a3ed84f9c0d1`.

Reproduction from the repository root:

```sh
cargo test --locked -p vsf-plugin-host -- --test-threads=1
cargo test --release --locked -p vsf-plugin-host -- --test-threads=1
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release --locked -p vsf-plugin-host --bin vsf-plugin-worker --example runtime-benchmark
python3 scripts/plugin-runtime-benchmark.py
source "$HOME/.nvm/nvm.sh"
nvm use --silent
npm run format:check
npm run lint
```

Logs, feature-tree inspection and benchmark JSON are retained in
`test-results/plugin-runtime-20261018/`. The initial compile ran zero tests and is
build evidence only. An initial locked benchmark build correctly refused the new
development dependency entry; an offline lockfile update followed, then the locked
build passed. Five packages were added (the three rquickjs crates, allocator-api2
and relative-path); no existing package version changed. Boa was inspected in the
registry cache only. The preserved legacy checkout was not changed or executed.

## Pure API/proposal evidence — 2026-10-18

The [API matrix](plugin-api.md) now documents implemented modern/VSS rules and
fixes, package-local loading/settings, transaction-local neighbors, raw/stripped
text, scenes and explicit unsupported surfaces. Fourteen real-helper API tests
pass, plus two combined domain-batch tests. All 478 workspace tests pass, with 31
opt-in tests ignored; 11 zero-test harnesses are excluded from feature evidence.
Strict workspace Clippy, Rustfmt, Prettier and ESLint pass. The separately acquired unchanged
archived overlap rule also passes its opt-in local test; no legacy script is
redistributed. These tests cover the library proposal and Editor transaction,
not desktop admission, review, recovery or historical VSS runtime parity.

Twenty exact 10,000-cue workloads exercise both APIs in check/fix modes on five
fixtures. Each has one warm-up and 20 measured fresh-helper runs. The declared
500ms p95 includes initial parse, capture/package validation, execution, every
finding/edit/metadata check and, for fixes, accepted history, serialization,
reparse, one Undo/Redo and stale rejection. WebVTT fixes retain absolute payload
timestamps and change only text; other fixes combine text and a 10 ms shift with
unchanged duration. The dense SRT karaoke fixture retains its expected markup
review warnings. No filesystem save/recovery, UI or native acceptance is claimed.

| Fixture | Modern check / fix p95 ms | VSS check / fix p95 ms |
| --- | ---: | ---: |
| Dense tagged SRT | 80.052 / 159.977 | 109.993 / 218.899 |
| ASS | 99.022 / 240.446 | 127.120 / 302.633 |
| SSA | 97.325 / 225.818 | 124.276 / 281.466 |
| Styled WebVTT | 98.169 / 205.627 | 123.595 / 274.932 |
| Timed WebVTT | 94.898 / 196.327 | 120.584 / 254.737 |

All pass; the largest p95 is 302.632917 ms and that workload's maximum is 304.23275 ms.
Combined parent/direct-helper RSS rises from 9,895,936 to 208,142,336 bytes:
**+198,246,400 bytes (189.0625 MiB)** against the unchanged 192 MiB limit. There are
2737 samples, 1539 observing a live helper. Only raw fixture allocation precedes
the baseline; parsed documents, captures, history and protocol copies are included.
Sampling can miss peaks and does not establish an OS-enforced memory cap. Existing
pure-application 96 MiB gates have not been relaxed.

The final API benchmark binary SHA256 is
`c24bce7847fd807b3936c2765a48641bb3e7235fa733d8f3c6d9a998d7cae4b0`;
the worker is
`b4348bf2e3caac3aa0fa85cbb479d166e9f55af00116b44e40dfbd6afd31ab62`.
Exact input identities:

| Fixture | SHA256 |
| --- | --- |
| Dense tagged SRT (generated in example) | `c8118a0cc80dc2e2955c68d83401a7cb1e80f0858fc6d5ef45827a3c1bfec6e2` |
| ASS | `00e4deb1210c47e2943c30744da0e5d0512a57ba159b659d9dd16ef5d47c1c0c` |
| SSA | `0d2415e1e2d61776e7c9051c3c3c1c10b1c02770e225652b7b0132c57fb79b14` |
| Styled WebVTT | `783a7ad89406073e0cb6512c9ca9daaac9b7c09baeaa933dada2bd0e67aa339f` |
| Timed WebVTT | `46430c8e7b374e2bd6ccb3d4bd95cf3489d2571f00ff8c9120c9c59ce5e1c1e6` |

Reproduce with the pinned toolchains, from the repository root:

```sh
python3 scripts/ass-fixtures.py
python3 scripts/webvtt-fixtures.py
cargo test --locked -p subtitle-core --test update_batch
cargo test --locked -p vsf-plugin-host --test subtitles -- --test-threads=1
# Set this to your separately acquired, hash-verified archived file:
VSF_LEGACY_OVERLAP_SCRIPT=/path/to/jsplugin/overlapping.js cargo test --locked -p vsf-plugin-host --test subtitles unchanged_archived_overlap_rule_and_fix_run_locally -- --ignored --nocapture
cargo build --release --locked -p vsf-plugin-host --bin vsf-plugin-worker --example api-benchmark
python3 scripts/plugin-runtime-benchmark.py --api
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm run format:check
npm run lint
```

Receipts are in `test-results/plugin-api-20261018/`; `benchmark-v4.json` is the
final passing run. Retained attempts include a benchmark format-string compile
error, an incorrect oracle that expected no karaoke warning, then actual VSS CPU
and repeated-run RSS failures. Sharing accessor functions alone was insufficient:
`benchmark-v3.json` records p95 up to 659.335 ms and +248,627,200 RSS bytes. A bounded
engine phase probe isolated the legacy callback loop; a key-type microprobe found
numeric-Map overhead. The final dense wrapper array preserves own properties and
stable identity. Immutable cue metadata sharing avoids copying every event field
across captures/history; style replacement remains copy-on-write. All fixture
contents, iterations, validation steps and CPU/RSS limits remain unchanged.
An initial SRT test fixture also omitted its canonical final blank line; it was
corrected to test existing serializer behavior, without changing format policy.

Only workspace-local dev-dependencies were added to compose application and
runtime tests/examples; no registry package/version changed. The application,
domain and desktop dependency boundaries are unchanged. Full desktop/CLI owner
integration and native/VM acceptance still keep stage 7 and ADR0008 open.
Frontend interaction/browser tests and a new native desktop build were not rerun
for this pure library increment; no UI/IPC/native workflow implementation changed.
