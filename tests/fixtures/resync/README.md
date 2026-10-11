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

Use `--extended` for a held-out seed, resize/FPS and speed/ad cases, a 250 ms
replacement, ambiguous deletion inside a static tail, and three ten-minute
programme blocks. These cases were added after the initial cut matcher and its
thresholds were fixed. CI runs this extended synthetic corpus.

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
- Decoded positive cases: every accepted mapping, including segment endpoints, must be within the
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

## Run and interpret the experiment

```sh
cargo test --locked -p media-analysis
cargo test --locked -p media-analysis --example resync-evaluate
cargo test --locked -p media-analysis --test visual -- --ignored --nocapture --test-threads=1
cargo run --locked -p media-analysis --example resync-evaluate > tests/fixtures/generated/resync/evaluation.json
```

The example reads `truth.json`, analyzes each video once and reports measured
coverage, worst/median error, unresolved spans and ordering contradictions. Exit
2 means an acceptance criterion failed; JSON remains available for inspection.
This is a research executable, not the product CLI. It never writes subtitles.
The configured Linux CI job generates/evaluates synthetic cases and retains its
truth/results. No reference movie is downloaded by that job.

The experiment reuses the exact Standard cut filter in two sequential
FFmpeg passes: timestamp collection and 16×16 grayscale signatures. Signature
output is capped at 5,120,000 bytes. It requires mutual, distinctive appearance
matches (mean absolute pixel error at most 12 and next-best margin at least 6),
then at least three consecutive matching anchors with timing residual at most
50 ms and positive scale in 0.8–1.25. These constants are provisional. Matching
and fitting each have a 25-million-comparison limit with deadline/cancellation
checks; no matrix is allocated. Media IO remains on the calling analysis worker.

A matched shot can extend to the next observed cut only when its source/target
durations agree with the established scale to 1 ms. Unknown tails and unmatched
shots remain uncovered. No accepted document, application settings, player or
frontend state is involved.

The verifier adds two sequential passes collecting small frames near a 500 ms
grid and their **actual** integer PTS. It does not synthesize timestamps from FPS
or pair images after discarding pre-origin frames. At most 30,000 samples and
7,680,000 image bytes per video are retained. Cut-derived models provide scale
and offset; samples can establish additional intervals inside that model's
neighborhood. The verifier requires spatial and local temporal detail, image
error at most 6, and a better match than alternative offsets at ±2/4 seconds.
Static or repeating images cannot alone establish timing. At least two seconds
of consecutive evidence are required, and unmatched cuts between samples split
the interval. Unknown gaps are not merged; cancellation publishes no partial
refinement. Sampling remains heuristic: sub-sample edits without a detected cut
and unseen visual distortions remain quality risks, not proven capabilities.

Algorithm `scene-visual-sampled-v2` also leaves the interval between differing
neighboring cut-derived models unresolved. Half-second image samples searched
within 300 ms cannot locate a shorter edit between those models. Allowing both
models to extend there could cause the conflict filter to discard trusted cut
evidence as well as the weak extensions. A focused regression now protects both
trusted intervals and requires abstention throughout the uncertain gap.

Initial development Mac evidence (FFmpeg 9.0.2): the ten synthetic cases passed,
with 93.7–94.9% interior coverage on positive cases and zero measured mapping
error; unrelated/repeated/flat cases abstain and reordering is identified. The
reference edit retains 82.8% measured interior coverage with zero measured error,
so its 90% evaluation target failed. Accepted intervals were 47.708–199.833,
220.500–399.708 and 404.958–486.292 seconds of the source. Inspection of the tail
confirms long credits/low-cut content. More evidence between cuts and near trims
was needed; the threshold was retained while adding visual evidence.

After sample verification, all 16 cases generated by `--reference --extended`
pass: reference interior coverage is 95.2%, held-out speed/ad coverage 99.2%,
held-out resize and long-block coverage 100%, with zero measured error in
accepted intervals. The static-deletion negative case retains only the
unambiguous prefix; its tail stays unresolved. All original negative cases
still abstain. These are measured corpus results, not a universal accuracy claim.

The original reference result/truth are preserved locally as
`evaluation-reference-baseline.json` and `truth-reference-baseline.json` in the
generated directory. Regenerating with `--reference` reproduces the wider case
set. The new result is retained locally as `evaluation-extended.json`. Fade,
crop/logo and additional representative-content checks remain open; this
experiment is not release acceptance.

With the v2 discontinuity guard, the same 16 cases pass; reference coverage is
94.3% (previously 95.2%), with zero measured accepted error. The evaluator now
checks both endpoints of every accepted interval against the independent edit
list in addition to its quarter-second interior grid. Its three oracle tests
cover short intervals between grid samples, affine endpoint errors and an
interval crossing deleted footage. CI explicitly runs these example tests.

A new, still experimental 120-second low-cut case deletes 120 ms inside a
ten-second shot. Its original result lost the trustworthy source 10–50 second
interval: coverage was 50.6%. V2 retains that interval and reaches 85.3%, with
zero measured error, but still fails the unchanged 90% target. Source 50–60
seconds remains uncertain around the edit, and the final shot's motion repeats
exactly every 1.76 seconds (three decoded frame hashes agree). This is evidence
of a coverage limitation, not a passing release gate. The experiment and its
original failure remain in the current robustness work; broader quality
validation is still in progress.
