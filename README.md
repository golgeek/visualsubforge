# VisualSubForge (VSF)

VisualSubForge is a subtitle editor inspired by VisualSubSync's waveform-centered
workflow, built with Rust, TypeScript, Svelte and Tauri. It is a learning project
with experimental native media integration, not a release-ready editor.

The editor provides:

- UTF-8 SRT, ASS/SSA and WebVTT editing, cue operations, Undo/Redo, atomic saves
  and accepted-state recovery.
- [Automatic cue editing](docs/cue-editor.md), fixed-height issues and conservative single-cue fixes.
- Waveform navigation, timing gestures, scene detection and native subtitle preview.
- Reviewed timing repairs, batch shifts/alignment, tag-aware text tools, quality
  checks, statistics and optional English/French language tools.
- ASS/SSA style and font authoring, reviewed separate-file SRT/ASS/SSA conversion,
  and experimental visual resynchronization through the desktop or CLI.
- Projects, portable settings/profiles, reviewed VisualSubSync project import,
  manual translation references and a bounded JavaScript checker/fix plugin subset.

See the [current feature/platform matrix](docs/status.md) for supported scope,
linked feature contracts, test instructions and remaining gates.

## Start here

- [Set up and run the default editor](CONTRIBUTING.md#install-and-launch), then
  use the [pinned checks](CONTRIBUTING.md#checks) for development.
- [Build native playback on Mac](docs/build-macos.md), or follow the experimental
  [Windows](docs/native-runtime.md#windows-native-playback-experiment) and
  [Linux](docs/native-runtime.md#linux-native-playback-experiment) recipes.
- [Understand the architecture](ARCHITECTURE.md) and trace
  [frontend ownership](src/lib/README.md).
- [Run actual desktop workflows](tests/desktop/README.md), generate
  [synthetic fixtures](tests/fixtures/README.md), or inspect
  [native diagnostic tools](tests/native/README.md).
- [Prepare preview packages](docs/releases.md) and review
  [native adoption requirements](docs/native-runtime.md#remaining-adoption-gates).

The default desktop build edits subtitles without native playback or a legacy
checkout. Browser previews use modeled IPC in explicit test fixtures. Rust owns
accepted documents and history; unapplied frontend drafts remain separate.
Recovery restores accepted content, not drafts or Undo history, and requires Save As.

Native Mac ANGLE/Metal, Windows D3D11 and Linux GTK playback remain opt-in
experiments. Builds, VM trials and bounded native checks do not establish physical
GPU/audio, input, composition, latency or distribution acceptance. Format-specific
preservation and preview limits remain visible in the [status matrix](docs/status.md).

## Independence and licensing

The application builds without VisualSubSync. Its checkout is an optional,
unchanged behavior reference, not a build or runtime dependency. No legacy code
or assets have been imported.

VisualSubForge's original code is licensed under the GNU General Public License,
version 3 or (at your option) any later version. See [LICENSE](LICENSE). It comes
without any warranty. Third-party code and dependency patches retain their own
licenses, notices and provenance; a complete inventory of dependencies actually
included in each release remains required.
