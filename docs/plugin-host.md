# JavaScript plugin host

Stage7 is in progress. The bounded worker foundation exists in
`crates/plugin-host`; the editor and CLI do not yet load or execute plugins.
Modern subtitle APIs, VSS compatibility, reviewed fixes and their integration
remain required. ADR0008 is still proposed.

## Candidate execution boundary

The experiment pins `rquickjs`/`rquickjs-core`/`rquickjs-sys` 0.14.0 and the latter's
bundled QuickJS-NG 0.16.2 sources. The binding and engine use MIT licenses. Published
crate checksums are in Cargo.lock; existing distribution inventory/source recipes
must include these components when a shipped application actually links them.
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
Native package placement and desktop/CLI startup integration remain unimplemented.

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

- Modern API and supported VSS rule/fix subset with precise capability/version,
  settings, package-local loading and missing-dependency behavior.
- Transaction-local writable legacy wrappers, read-your-writes, raw versus stripped
  text policy, neighbor/scene semantics and representative unchanged legacy checks.
  Unresolved-license scripts may be tested locally but must not be redistributed.
- Rust validation of stable cue identities, revision/context, source-format
  precision/markup safety and all proposed edits; accept as one Undo transaction.
  Exceptions, cancellation, invalid results and stale replies commit nothing.
- Editor/CLI discovery, review, adoption, cancellation, close/replace/shutdown,
  save/reopen/recovery and races with existing accepted state. One admitted plugin
  run with no queued helper process; the current low-level `run` function relies
  on its caller to provide that application-wide admission bound.
- Final 10k behavior/performance with real APIs and document integration, native
  Mac workflow/chooser and available Windows/Linux VM/CI validation. Bindings
  present in a crate are not successful platform builds or execution evidence.

General actions, columns, UI hooks, split hooks and broad historical-pack
compatibility remain separately tracked beyond the first rule/fix subset.

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
