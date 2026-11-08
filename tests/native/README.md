# Native investigation tools

Start with the [native maintenance contract](../../docs/native-runtime.md) for
current recipe variants, pinned build tools, SDK checks, runtime identity,
validation scope and adoption gates. The [current status](../../docs/status.md)
separates implementation from historical experiments and release support.

Run synthetic controls through `check-controls.py` using the pinned dependencies
in `requirements.txt`; [Native oracle controls](#native-oracle-controls)
provides the commands. That runner discovers every `*-test.py` suite and rejects
failure or timeout. The fake boundary in `mpv-client-test.cpp` tests shared
command/event policy without a real player. `media-transport-test.rs` compiles the
production Rust transport policy against a deterministic player boundary, covering
paused/seeking cursor selection, loop-clear transactions and rollback failures.
Ordinary Cargo workspace tests do not compile this native-feature boundary; the
headless CI matrix runs it separately on macOS, Windows and Linux with pinned Rust:

```sh
rustc --edition=2024 --test --cfg 'feature="media-prototype"' tests/native/media-transport-test.rs -o /tmp/vsf-media-transport-test
/tmp/vsf-media-transport-test
```

On Windows, choose a writable output path ending in `.exe` and run that executable.
These fake-client controls do not link mpv or prove device behavior.

| Directory or tool | Role |
| --- | --- |
| `mpv-clock/`, including `candidates/` | Selected pinned baseline and optional audio corrections. Source preparation and applicable upstream tests are automated. Content/clock gates remain open. |
| `mpv-render/`, `mpv-coreaudio-cleanup/` | Provenance and regression context for corrections; the preparation manifests distinguish already-upstream prerequisites from local patches. |
| `ggml-unload/`, `mpv-subtitle-reuse/`, `libvlc-seek/` | Separate retained experiments, excluded from the selected mpv recipe. Their existence is not dependency adoption. |
| `linux-dialogs/` | Private-bus portal lifecycle regression; requires Linux, dbus-daemon and Python Gio. |
| `scene-candidates/` | Bounded compressed-scene investigation, separate from native playback acceptance. |
| Audio/capture scripts | Content, continuity and clock analysis with synthetic negative controls. Retain exact input/runtime/device identity for actual capture results. |
| `audio-editor-content.py` | Two-channel content verification for the [actual Mac editor audio lane](../desktop/README.md#native-audio-re-entry-and-replacement-macos-and-linux). Validates capture/source identity, repeated-loop content and uninterrupted suffix start/EOF/order. Its five controls reject wrong sources, omitted/repeated content and inserted silence; they do not prove physical presentation. |

Experiment guides retain the reproduction steps, dated findings and limitations
needed to interpret their probes without private research notes. They describe
historical experiments; the linked native maintenance contract and current status
remain the authorities for adoption. Raw generated evidence belongs under ignored
fixture/output directories and must not overwrite prior captures.

## Native oracle controls

Run from the repository root.

The Python analysis tools have synthetic positive/negative controls that require
no capture device or player. Use Python 3.14.7 (the CI pin) and an isolated environment:

```sh
python3 -m venv /tmp/vsf-native-controls
/tmp/vsf-native-controls/bin/python -m pip install --only-binary=:all: -r tests/native/requirements.txt
/tmp/vsf-native-controls/bin/python tests/native/check-controls.py
```

On Windows, choose a local virtual-environment directory and its `Scripts/python.exe`.
The runner executes every `tests/native/*-test.py` suite, with a per-suite timeout,
and fails for missing dependencies or any nonzero exit. CI runs it with the pinned
NumPy version. Passing synthetic controls establishes the oracle's behavior on
those inputs; it does not substitute for a new GPU/audio/process capture.

When changing native mpv client policy, also compile and run
`tests/native/mpv-client-test.cpp`. It supplies fake client calls to test queue
ordering/bounds, render/media error precedence and unavailable signed properties;
it does not validate GPU or audio-device behavior. With the Mac development
headers installed:

```sh
clang++ -std=c++17 -Wall -Wextra -Werror -DMPV_STATIC -I/opt/homebrew/include tests/native/mpv-client-test.cpp -o /tmp/vsf-mpv-client-test
/tmp/vsf-mpv-client-test
```

Linux uses the same C++17 source with mpv's pkg-config include flags. Windows
uses the configured ARM64 MSVC developer environment, `/std:c++17 /EHsc /W4
/WX /DMPV_STATIC` and the SDK include directory. Keep assertions enabled and
record compiler/platform identity separately from application checks.

## Workflow performance observation

For task006 performance observation, launch a built app with the process-local
environment variable `VSF_WORKFLOW_TRACE=1`. Stderr records JSON lines prefixed
`VSF_WORKFLOW_TRACE`: `document-acknowledgment` measures the typed client request
through IPC response decoding, and `preview-command` measures accepted-preview
serialization and native replacement commands. Records include operation,
generation/revision and durations, without subtitle text or paths. Tracing is
off by default. Its configuration request is asynchronous, so early requests
before that response can be absent; verify sample counts against the workload.
Frontend/backend durations use their respective monotonic clocks. Wall-clock
timestamps support approximate correlation only. Neither event proves Svelte
paint, GPU presentation or speaker output. The performance tracker retains
those separate acceptance gates.

Records are formatted completely before writing to stderr so native renderer
messages do not interleave inside serde's fragmented JSON formatting. Validate
every trace record and the expected operation counts; malformed or missing
records invalidate latency accounting rather than being silently filtered out.

Explicit Apply-button clicks and textarea commit keydowns also emit `apply-input`
records. They contain the browser event timestamp, trusted-event flag, performance
time origin, a monotonic bracket around `Date.now()`, and generation/revision
before the attempted edit. Correlate with the subsequent successful edit ACK;
the input record alone does not prove acceptance. Native menu actions and other
shortcuts without a supplied DOM event are not covered. Logging is queued after
the synchronous handler dispatches its work, then written on a blocking worker.
No event object, key text, subtitle content or document snapshot is retained.

The [DOM event timestamp](https://dom.spec.whatwg.org/#dom-event-timestamp) is
relative high-resolution time; it is not a physical device timestamp. The
[performance time origin](https://www.w3.org/TR/hr-time-3/#time-origin) is not
guaranteed to track later wall-clock adjustments. Keep the raw clocks separate:
`validClock` checks finite, nonnegative values and event/handler ordering only.
It does not certify resolution, clock synchronization or trustworthy input.
Reject invalid or missing records for latency accounting. Before correlating
VM events with host screenshots, independently bound clock offsets and drift,
retain uncertainty, and verify event delivery and unique visible labels.
Do not subtract an assumed automation delay or treat handler/request completion
as proof of visible presentation.

For Mac native-render attribution, add `VSF_RENDER_TRACE=1` to the process
environment. The render worker preallocates at most 16,384 stage records and
dumps `VSF_RENDER_FRAME` lines after graphics teardown; the summary reports
overflow. Allocation failure disables only the diagnostic. No frame-time IO,
decoded pixels or subtitle text enter this trace. Fields split drawable-lock
wait, update/setup, render, clear, presentation-call and report-swap stages.
Render time includes mpv's normal presentation-target wait; it is not GPU time.
`swapped=1` means the adapter completed its presentation call, not that the
compositor displayed a specific document revision. Begin/end Unix times use
one wall-clock anchor plus monotonic elapsed time for approximate correlation
with workflow traces; wall-clock changes invalidate that correlation. Abrupt
termination loses the buffered trace. Normal runs without the variable allocate
no trace buffer or collect timing clocks. This diagnostic currently covers the
Mac ANGLE and legacy OpenGL adapters only.

For actual Metal presentation feedback on the ANGLE path, also set
`VSF_PRESENT_TRACE=1`. An opt-in CAMetalLayer subclass forwards drawable
acquisition unchanged and registers the public Metal presentation callback.
`VSF_METAL_PRESENT` records drawable IDs, acquisition, presentation and callback
host times; `VSF_METAL_TRACE` reports pending callbacks and overflow. A zero
presentation time means unpresented/dropped, not success. Callback arrival is
not the presentation time. Two `VSF_METAL_CLOCK`/`VSF_METAL_CLOCK_END` anchors
bracket wall-clock reads with host-clock reads; reject changed wall-clock offsets
before correlating with input timestamps. The 16,384-slot buffer is shared only
to keep late callbacks safe; callbacks write distinct slots and publish completion
atomically. The render worker dumps after graphics/lock teardown without waiting
for callbacks. Pending slots must not be counted as presented. Disabled runs use
the original layer class and allocate no presentation buffer. Abrupt termination
loses the trace. This adds no pixel readback or native dependency patch; it does
not tag a drawable with a document revision. Match controlled paused edits to
unique visible screenshot labels and retain all intervening draws before making
any claim about subtitle presentation latency.
