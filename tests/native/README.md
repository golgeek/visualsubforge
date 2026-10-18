# Native investigation tools

Start with the [native maintenance contract](../../docs/native-runtime.md) for
current recipe variants, pinned build tools, SDK checks, runtime identity,
validation scope and adoption gates. The [current status](../../docs/status.md)
separates implementation from historical experiments and release support.

Run synthetic controls through `check-controls.py` using the pinned dependencies
in `requirements.txt`; [CONTRIBUTING](../../CONTRIBUTING.md#browser-and-analysis-regressions)
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
| `audio-editor-content.py` | Two-channel content verification for the [actual Mac editor audio lane](../desktop/README.md#native-audio-re-entry-and-replacement-macos). Validates capture/source identity, repeated-loop content and uninterrupted suffix start/EOF/order. Its five controls reject wrong sources, omitted/repeated content and inserted silence; they do not prove physical presentation. |

Experiment documents retain dated results, limitations and links into intentionally
untracked local task/research notes. Those links may be absent in a fresh checkout.
They are historical context, not the current status authority or dependencies of
the tracked source recipes and control runner. Generated evidence belongs under
ignored fixture/output directories and must not overwrite prior captures.
