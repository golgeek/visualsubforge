# Resynchronization experiment corpus

Generate the original synthetic corpus with:

```sh
python3 scripts/resync-fixtures.py
```

The script writes only `tests/fixtures/generated/resync/`. It uses the configured
`VSF_FFMPEG`/`VSF_FFPROBE` or PATH, with a five-minute bound per subprocess and two
encoder threads. Inputs, commands, tool versions, media SHA-256 values and rational
edit-list ground truth are recorded in `truth.json`. Generated binaries are ignored.
Truth is derived before matching, never from detector or alignment output.

Procedural spatial patterns and moving objects are original test material. All
shots last two seconds deliberately: equally spaced scene cuts are insufficient
to distinguish the source from unrelated pictures. Cases cover offsets, unchanged
duration at different FPS, actual speed changes, replaced ads, trims, a partial
shot deletion, VFR with nonzero origin, repeated content, flat video and reordering.

`--reference` also creates edited derivatives of the existing licensed
`generated/big-buck-bunny.mp4`. Obtain it with the existing
`python3 scripts/fetch-reference-movie.py --resolution 180` when absent.
Attribution: **(c) copyright 2008, Blender Foundation / www.bigbuckbunny.org**.
The movie is [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/) under the
[project attribution instructions](https://peach.blender.org/about/).
These local test derivatives resize the film and remove/rearrange sections;
the unchanged original and its provenance remain in the parent fixture directory.
Do not commit the movie or derivatives. This one animated film is useful real
content, not representative evidence for all broadcast programmes.

## Criteria fixed before matcher tuning

- Pure known-map evaluation: exact rational mapping, with at most 0.5 ms final
  rounding error; invalid/collapsed time ranges remain unresolved.
- Decoded positive cases: every accepted interior mapping must be within the
  source maximum presentation interval plus target maximum presentation interval
  plus 1 ms of ground truth. The generator probes those intervals; FPS metadata
  is not used to assign the tolerance or expected speed.
- At least 90% of retained synthetic programme duration more than one second
  from an edit boundary must be covered. Report coverage and worst/median error
  separately; never improve coverage by accepting an uncertain boundary.
- Zero accepted correspondence in unrelated, repeated and flat negative cases.
  Reordering must be diagnosed, not made to appear like a complete resync.
- No map may bridge removed source footage or inserted target footage. A cue
  crossing such a boundary is unresolved even when its endpoints individually
  match. Leading/trailing unsupported footage is not extrapolated.
- Repeatability: identical inputs/configuration yield identical map/diagnostics.
  Keep generated outputs and reports for failed cases rather than editing truth.
- Experiment ceiling: four-hour inputs, at most 20,000 scene descriptors, at most
  25 million descriptor-pair comparisons, no all-pairs matrix, one decoding child
  at a time, bounded decoded-output storage and explicit timeout/cancellation.
  Exceeding a limit returns an error, not partial success.

Separate training and held-out fixtures before adjusting thresholds. The initial
synthetic cases are development cases; additional seeds, long programme blocks,
held-out real scenes, fades and encoding/crop/logo perturbations are required
before claiming the first release's measured scope. Record abstentions and false
confident matches, not just successful examples. These criteria do not constitute
evidence that a matcher already meets them.
