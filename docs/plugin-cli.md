# Local JavaScript plugin CLI

The experimental `vsf plugin` commands run local modern or VSS checker/fix
packages against UTF-8 SRT, ASS, SSA or WebVTT. Build with
`cargo build --locked -p vsf-cli`. The [API matrix](plugin-api.md) defines the
supported subset. Desktop review/Undo is implemented and passes the recorded
Mac and ARM64 VM workflows. The bounded historical checker/fix comparison also
passes with [documented differences](plugin-api.md#historical-vss-comparison--2026-10-18).
Physical platform acceptance remains open. See [desktop evidence](plugin-desktop.md).

## Check, review and publish

```sh
target/debug/vsf plugin check --subtitles captions.srt \
  --package examples/plugins/trim-trailing/manifest.json

target/debug/vsf plugin preview --subtitles captions.srt \
  --package examples/plugins/trim-trailing/manifest.json --json

target/debug/vsf plugin apply --subtitles captions.srt \
  --package examples/plugins/trim-trailing/manifest.json \
  --accept REVIEW_ID_FROM_PREVIEW --output corrected.srt
```

Check and preview never write files. Preview reports every finding and raw
before/after text and timing, including edits outside the requested scope and
markup warnings. `--json` emits the complete version-1 review. Human output also
includes context, effective settings, logs and the review ID; cue text uses escaped
strings so line endings and literal backslashes remain distinguishable.

Apply reruns the plugin with the same input files and options. Its computed
review ID must match the one explicitly supplied with `--accept`. The ID hashes
every review field with the ID field empty: input paths/content hashes, captured
package, context and settings, findings, proposed changes, logs and resulting
subtitle hash. Changed inputs or nondeterministic findings/fixes require a new
preview. The review JSON is informational; apply does not deserialize or trust
edited rows from a saved report. The output path is chosen at publication time.

Output must be a **new file of the same format**. Existing files, hard/symbolic
aliases, dangling symlinks and protected source/package/context paths are refused.
No in-place mode exists. A publication with no proposed changes is refused.
Successful JSON output is a receipt with `status: "published"`, canonical output
path, review ID and output SHA256. The original file remains available to restore;
the CLI does not persist editor Undo history across processes.

## Scope, settings and explicit context

`--cue 1,3 --cue 7` checks/fixes those one-based **file positions**, independent of
SRT labels. Omission means all cues. `--select 2,4` exposes a separate selection to
the script; omission means empty selection. Duplicate, zero and out-of-range
positions fail. Callbacks still run in document order. Plugins may fix captured
neighbors outside scope; the review identifies those changes.

`--setting removeTabs=false` overrides a declared scalar setting. Values are JSON;
quote string values for your shell. Repeated keys, undeclared settings and type
changes fail. VSS parameters use their exact names, such as `ParamMode=3`.

Default CLI preferences are the VSF default policy: 80ms gap, 800–7000ms duration
and 20 CPS. They do not load desktop settings. Cursor, scene data and VSS RS target
index are unavailable unless supplied through `--context FILE`. There is no
guessed mapping from a VSF reading-speed threshold to a VSS preset index.

Context is a strict JSON object of at most 2MiB. Each top-level field below is
optional; an explicit `preferences` object supplies the complete preference set:

```json
{
  "cursorMs": 1234,
  "preferences": {
    "minimumBlankMs": 80,
    "minimumDurationMs": 800,
    "maximumDurationMs": 7000,
    "cpsTarget": 20,
    "rsTargetIndex": 2
  },
  "scenes": {
    "cutsMs": [1500, 2500],
    "startOffsetMs": 0,
    "stopOffsetMs": 0,
    "filterOffsetMs": 0
  }
}
```

Scene cuts must already be completed and mapped to subtitle time; this command
does not analyze media. An absent scene result differs from an available empty
`cutsMs` array. Unknown fields and invalid context fail explicitly.

## Package and operation bounds

Choose a regular manifest in a dedicated local package directory. The loader
captures all lowercase `.js` files under that directory, including unused helpers,
and the selected manifest. Non-JavaScript regular files are ignored. There is no
installation, automatic discovery, download or search outside that root.

The manifest is at most 512KiB; the directory has at most 1024 entries and 16
levels. Packages permit at most 64 JavaScript files and 1MiB total script source.
Symlinks and special files anywhere in the package are refused. The pure package
validator additionally checks names, capabilities, paths, case collisions and
settings. Subtitle input retains the existing 16MiB limit. Complete reviews are
bounded to 64MiB; helper request/reply bounds remain 32MiB.

`--timeout-ms` bounds the whole operation (default 10000, range 1–60000).
`--execution-ms` bounds the helper (default 1000, range 1–10000), capped by the
remaining whole-operation budget. Cancellation and deadline checkpoints cover
reads, traversal, preparation and publication. Ordinary blocking filesystem calls
and stdout writes are not forcibly interruptible; this is not a real-time bound.
Ctrl-C requests cleanup and helper termination/reaping, rather than exiting from
the signal handler.

The CLI self-spawns in an early internal worker mode before argument parsing or
signal/application initialization. The parent sends only captured JSON and owns
the helper lifetime. [Runtime limits and isolation](plugin-host.md) apply; no
filesystem/network/native bindings are exposed to JavaScript. This is not an OS
security sandbox. Each CLI invocation runs one helper; independent CLI processes
are independent jobs. Desktop-wide admission remains a separate owner concern.

Rust validates all returned changes through the existing source-aware domain
transaction before serialization. Preparation re-reads/hashes source, package and
context after execution. Publication acquires the destination lease, prepares and
syncs a temporary file, revalidates all inputs and the output directory, then
commits without clobbering. Failed preparation drops the temporary file. The
stable sibling `.vsf-lock` lease file may remain. Late cancellation after a
successful commit cannot retract that file. Filesystem identity checks do not
claim an adversarial snapshot against arbitrary concurrent external writers.

| Exit | Meaning |
| ---: | --- |
| 0 | Check has no findings, preview complete, or publication succeeded |
| 1 | Check completed with findings |
| 2 | Invalid arguments/input/package/proposal or size/format limits |
| 4 | Script/worker/API failure, including unknown or mistyped effective settings |
| 5 | IO or publication failure |
| 6 | Inputs or reviewed proposal changed; preview again |
| 7 | Execution or operation timeout |
| 130 | Cancelled before publication |

If stdout fails after publication, the command can report IO failure although the
new file exists. Inspect that path before retrying; it will never be overwritten.

Validation commands, retained failures and scale results are recorded in the
[host evidence](plugin-host.md#cli-and-package-io-evidence--2026-10-18).
