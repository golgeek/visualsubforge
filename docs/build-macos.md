# Build and run VisualSubForge on macOS

Use the shared [native build guide](build.md#macos-setup). After one-time setup:

```sh
nvm use
make build
open target/native/macos/release/package/VisualSubForge.app
```

`make install` installs to `~/Applications`. `make debug` builds a native debugging
app at `target/native/macos/debug/package/VisualSubForge.app`.
Both include media playback, FFmpeg/ffprobe and the plugin CLI.

For individual SDK stages and runtime diagnostics, see the
[manual recipes](build-macos-manual.md).
