# Development workflow

[Current status](docs/status.md) records implementation and remaining gates. [Architecture](ARCHITECTURE.md) describes the implemented design. Public
contributor documentation does not depend on local research or task histories.

## Working on a task

1. Define the focused outcome and acceptance criteria in the change description.
2. Read the relevant code, expected behavior and available context.
3. Explain consequential design choices before implementing them; update `ARCHITECTURE.md` with verified ownership/dependency changes and explain unimplemented proposals in the change description.
4. Implement one small, end-to-end behavior or one bounded feasibility experiment.
5. Validate it, record evidence, and update the relevant documentation.
6. Mark the change complete only when its acceptance criteria are met. Record remaining work explicitly.

Use short-lived branches and focused commits. A review description should explain the resulting behavior, significant tradeoffs, and exact validation commands/results. Keep decision records, task histories and legacy research local; do not add them to the public repository. Put validation results and evidence in PR descriptions only; keep public guides focused on current behavior, runnable checks and their limits.

## Definition of done

- Acceptance criteria met or explicitly revised with a reason.
- Relevant automated checks pass; unavailable platform checks are identified rather than implied.
- Errors, cancellation, and resource cleanup are handled where the change introduces them.
- Documentation and public status match the implementation.
- The change is understandable to someone learning Rust and Svelte.

For foundational features, add a short walkthrough tracing the UI action through application and domain code and back. Explain why ownership and state are arranged that way. Add walkthroughs when actual code exists.

## Validation strategy

- Domain tests cover timing invariants, editing operations, undo/redo, and parser/serializer behavior.
- Integration tests cover command boundaries, persistence failures, and media adapter lifecycle.
- UI tests cover important interactions, keyboard focus, and stale asynchronous responses.
- Actual desktop runs cover native playback, display scaling, rendering, and audio/video alignment on macOS, Windows, and Linux.

Compilation alone does not validate native playback. The media prototype must produce a platform evidence matrix.

## Commands and toolchains

For an end-to-end native build and launch on the current Apple Silicon Mac, use
[Build and run VSF](docs/build-macos.md). It connects the mpv/ANGLE builds to the
application and selects the intended runtime explicitly. Audio candidates remain
experimental until the [native adoption gates](docs/native-runtime.md#remaining-adoption-gates) pass.

Run all commands from the repository root. Rust 1.98.1 (including rustfmt and Clippy) is selected by `rust-toolchain.toml`. Node 24.21.0 and its bundled npm 11.19.0 are pinned by `.nvmrc` and `package.json`. Commit both `Cargo.lock` and `package-lock.json`; use locked commands below rather than updating dependencies during routine setup.

### Prerequisites

- Install [rustup](https://rust-lang.org/tools/install/).
- macOS: install Xcode Command Line Tools (`xcode-select --install`) or select a full Xcode installation.
- Windows: install Visual Studio Build Tools with **Desktop development with C++**, the Windows SDK, and WebView2. Use the MSVC Rust toolchain.
- Ubuntu 24.04: `sudo apt-get update` followed by `sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.
- Consult [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for other distributions. These are development environments, not a supported release-platform promise.

### Install and launch

For the app with media on macOS, Windows x64 or Ubuntu 24.04 amd64, follow
[the native build guide](docs/build.md): `make build`, optionally `make install`.
`make debug` produces a debugging app with the same native media support.

For the **media-free test/development shell**, with
[nvm](https://github.com/nvm-sh/nvm) loaded in your macOS/Linux shell:

```sh
nvm install
nvm use
node --version
npm --version
rustup show
npm ci
npm run tauri -- dev -- --locked
```

`nvm use` selects the version for the current shell; run it again in a new terminal. On Windows, install Node 24.21.0 with npm 11.19.0 using the official installer or a Windows-compatible version manager; POSIX nvm is not a Windows shell tool. CI reads `.nvmrc` directly through setup-node.

For native Windows ARM64 development, use the ARM64 Node distribution and Rust's `aarch64-pc-windows-msvc` host. Include **MSVC Build Tools for ARM64/ARM64EC** and a Windows SDK in the C++ build-tools installation; the default x64 compiler selection alone does not establish an ARM64 development environment. Verify `rustc -vV`, `node --version`, and `npm --version` before running the same locked checks below. The [status table](docs/status.md) distinguishes supplied-VM evidence from platform support.

Tauri starts Vite at `http://localhost:1420` and opens the desktop window. Port conflicts fail explicitly. Stop with Ctrl-C. `npm run dev` runs only the browser UI and is not evidence of desktop compatibility. This test-only shell needs no legacy checkout, FFmpeg or libmpv, but cannot open
projects containing media. Use the Make targets above for ordinary app testing.

### Checks

```sh
# Pure Rust only: no Node, system webview packages, or native media needed.
cargo check --locked -p subtitle-core
cargo test --locked -p subtitle-core
cargo clippy --locked -p subtitle-core --all-targets -- -D warnings

# Frontend and formatting.
npm run format:check
npm run lint
npm test
npm run build
npm run ipc:check
cargo fmt --all -- --check

# Whole workspace; build frontend assets before compiling the desktop shell.
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked -p vsf-cli
npm run tauri -- build --debug --no-bundle -- --locked
```

`npm run build` includes strict Svelte/TypeScript checking; `npm run check` runs that step alone. `npm run format` and `cargo fmt --all` apply formatting. Prettier covers frontend and configuration files; Markdown is excluded; validate documentation links and run `git diff --check` separately.

The desktop plugin owner runs the `vsf` CLI beside its own executable as a bounded
helper. Normal `tauri dev` builds that CLI before starting Vite, and the desktop
test script and maintained package workflows already build/include it. For a
custom Cargo target, target directory or profile, build `vsf-cli` with matching
flags so the two binaries are siblings. The desktop never searches PATH for a
plugin runner. See the [desktop plugin workflow](docs/plugin-desktop.md) for
current controls and remaining acceptance gates.

## Focused validation

Choose checks for the changed behavior; a zero-test harness or successful build
is not feature validation. Build frontend assets before any desktop compilation,
including IPC generation and native-feature tests.

| Change or investigation | Canonical commands and scope |
| --- | --- |
| IPC definitions | Run `npm run ipc:generate`, `npm run check` and relevant browser workflows; `npm run ipc:check` detects drift without rewriting files. Both IPC commands compile the desktop harness. See [generated IPC](src/lib/README.md#generated-ipc). |
| Browser interactions | [Browser workflow checks](tests/desktop/README.md#browser-workflow-checks): pinned Playwright, prerequisites, ports, receipts and failure controls. These use modeled IPC. |
| Desktop persistence and feature workflows | [Real desktop tests](tests/desktop/README.md): `npm run build:desktop-test` then `npm run test:desktop`, Linux wrappers and optional format/scene/duration/statistics/plugin/10k lanes. Supplied file selections exclude real choosers. |
| FFmpeg analysis and headless CLI | [Analysis commands](tests/fixtures/README.md#analysis-regression-commands) and [resync corpus](tests/fixtures/resync/README.md): fixtures, ignored integrations and Windows console cancellation. |
| Native policy and diagnostic tools | [Native controls](tests/native/README.md#native-oracle-controls) and [trace interpretation](tests/native/README.md#workflow-performance-observation). Synthetic controls do not establish device behavior or presentation latency. |
| Native builds and playback | [Mac build guide](docs/build-macos.md), [Windows](docs/native-runtime.md#windows-native-playback-experiment), [Linux](docs/native-runtime.md#linux-native-playback-experiment), and [runtime selection](docs/native-runtime.md#development-runtime-selection-notes). Keep exact SDK/runtime identities and adoption limits. |
| Documentation | Check relative paths and section anchors against tracked files plus intended additions, not local notes; run `git diff --check`. Preserve prerequisites, arguments and limitations when moving commands. |

CI definitions live in [application checks](.github/workflows/checks.yml) and
[native build contracts](.github/workflows/native.yml). Configured jobs alone
are not evidence of hosted execution. Consult [release preparation](docs/releases.md)
for dated hosted/package results and [signing setup](docs/macos-signing.md) for
macOS credentials. Package creation does not establish release support.

For code navigation, start with the [architecture walkthrough](ARCHITECTURE.md#two-end-to-end-paths)
and [frontend owners](src/lib/README.md). Native devices, composition, focus,
real dialogs and physical input require separately identified desktop evidence.
