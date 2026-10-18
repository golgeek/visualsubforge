# Independently authored VSS contract probe

These synthetic inputs and ES3-compatible scripts are original repository code,
licensed GPL-3.0-or-later. No historical executable, DLL or plugin implementation
is included. This probe is preparation for a running-host comparison, **not a
record of historical parity**. Its VSF CLI control passes; the historical report
and fixed-file comparison remain pending.

`source.srt` has exactly three cues, UTF-8 BOM and CRLF: combined ASS/HTML tags,
accented/CJK/astral characters, a literal entity, a logical ASS newline escape,
ordinary braces and an unclosed HTML opener. `main.js` loads the encoding helper
and reports raw/stripped UTF-16 code units, endpoints, callback/core neighbors,
indexing/bounds, selection, cursor/preferences, empty scene queries/offsets,
callback receiver, scalar parameter and stateful RegExp/callback progression.
Each fix shifts the current cue by 10ms, appends `!`, and appends ` neighbour` to
the next cue. Later callbacks must observe earlier neighbor writes.

Run the VSF control from the repository root after building the CLI:

```sh
target/debug/vsf plugin check --subtitles tests/fixtures/plugins/vss-contract/source.srt \
  --package tests/fixtures/plugins/vss-contract/manifest.json \
  --context tests/fixtures/plugins/vss-contract/control-context.json --json
target/debug/vsf plugin preview --subtitles tests/fixtures/plugins/vss-contract/source.srt \
  --package tests/fixtures/plugins/vss-contract/manifest.json \
  --context tests/fixtures/plugins/vss-contract/control-context.json --json
```

Check returns 1 with three findings and no changes. Preview returns 0 with three
findings and three changes. Cue endpoints become 1010–1910, 3010–3910 and
5010–5910ms. Text gains `!` on the first cue and ` neighbour!` on the other two.
The supplied control context is deliberately explicit; its values are not
observations or assumed defaults of a historical host.

For historical execution, copy only `main.js` and `helpers/encoding.js` into the
isolated host's `jsplugin` directory. Open a writable copy of the synthetic SRT,
run Check Errors and export the error report to HTML. The finding's ASCII hex
fields avoid the historical exporter interpreting raw markup. Record the actual
selection, cursor, preferences and scene state; supply those same values to VSF
before comparing findings. Compare a fresh read-only check separately from fix
execution, then compare the saved semantic cue times/text. Do not require identical
serialization from the historical editor. Record execution-state, wrapper-identity
or other intentional differences explicitly instead of assuming full compatibility.

Pin the separately acquired historical binary/DLL hashes and provenance before
running. A dated binary is not proof that it was built from the local reference
source commit. Keep that checkout and original distribution unchanged, and keep
unresolved-license binaries/scripts out of repository fixtures.
