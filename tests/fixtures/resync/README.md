# Resynchronization experiment corpus

Generate the original synthetic corpus with:

```sh
python3 scripts/resync-fixtures.py
```

The script defaults to `tests/fixtures/generated/resync/`; `--output` selects a
different directory inside `tests/fixtures/generated/` to preserve earlier
experiments. It uses the configured
`VSF_FFMPEG`/`VSF_FFPROBE` or PATH, with a five-minute bound per subprocess and two
encoder threads. Inputs, commands, tool versions, media SHA-256 values and rational
edit-list ground truth are recorded in `truth.json`. Generated binaries are ignored.
Truth is derived before matching, never from detector or alignment output.

Use `--extended` for a held-out seed, resize/FPS and speed/ad cases, a 250 ms
replacement, ambiguous deletion inside a static tail, and three ten-minute
programme blocks. It also includes a deletion of one whole repeating-motion
cycle in the first or final shot, which must remain unresolved. These cases were
added after the initial cut matcher and its thresholds were fixed. CI runs
`--robustness` on all headless OS runners and `--extended --robustness` in the
analysis lane (24 and 29 synthetic cases respectively).

Procedural spatial patterns and moving objects are original test material. The
default shots last two seconds deliberately: equally spaced scene cuts are insufficient
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
of evidence are required, and unmatched cuts between samples split
the interval. Unknown gaps are not merged; cancellation publishes no partial
refinement. Sampling remains heuristic: sub-sample edits without a detected cut
and unseen visual distortions remain quality risks, not proven capabilities.

Algorithm `scene-visual-neighborhoods-v3` additionally decodes every frame in
merged ±4.3-second target neighborhoods around predicted times outside trusted
intervals. Two sequential passes retain at most 30,000 extra images, with an
8 MiB metadata cap. File-backed selection expressions avoid command-line limits.
The central comparison uses the nearest presentation timestamp, not the best
image anywhere within 300 ms; that broader search can hide short edits. Dense
observations check alternative offsets around ±2/4 seconds. Sparse fallback
requires those alternatives to exceed the image-error limit. One matching but
ambiguous observation may join distinctive neighbors; it cannot start/end an
extension, bridge a second ambiguity or bypass an unmatched cut.

Each observed range retains its original cut-derived model. Merging ranges
requires models to agree across their union and uses one original model, preferring
the fit supported by more cuts. Blending two fits' endpoints can invent a new
slope and erase trusted evidence during conflict checking; a regression test
covers the demonstrated live-action failure.

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
set. The new result is retained locally as `evaluation-extended.json`. At that
stage, fade, crop/logo and additional representative-content checks remained
open; subsequent results are below. This experiment is not release acceptance.

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

## Additional robustness and live-action evaluation

```sh
python3 scripts/resync-fixtures.py --robustness --output tests/fixtures/generated/resync-robustness
cargo run --locked -p media-analysis --example resync-evaluate -- tests/fixtures/generated/resync-robustness > tests/fixtures/generated/resync-robustness/evaluation.json
python3 scripts/fetch-reference-movie.py --film tears-of-steel
python3 scripts/resync-fixtures.py --live-action --output tests/fixtures/generated/resync-live-action
cargo run --locked -p media-analysis --example resync-evaluate -- tests/fixtures/generated/resync-live-action > tests/fixtures/generated/resync-live-action/evaluation.json
```

`--robustness` adds fractional 24000/1001 FPS with and without actual slowdown,
negative presentation timestamps, fades, crop/logo/brightness changes and three
low-cut/short-deletion cases. Fractional rates and timestamp origins retain the
90% coverage requirement. Visual distortions use `safe_partial_mapping`: there
is no promised coverage, but every accepted interval must satisfy the same
correctness criteria. These expectations were chosen before their first results.
The periodic-motion case still fails the unchanged 90% gate, so the robustness
command currently returns exit 2. It is not yet a passing CI lane.

The generator checks actual presentation timestamps and final frame duration;
it does not assume `format.duration` includes the timestamp origin. MPEG-TS
retains the negative-origin case; its first decoded PTS and container origin
must both equal -5 seconds. Tool versions and decoded intervals are recorded.

Development Mac, FFmpeg 9.0.2, algorithm v2 (2026-10-11):

| Added case | Interior coverage | Worst accepted endpoint error | Result |
| --- | ---: | ---: | --- |
| Fractional FPS, unchanged programme speed | 99.2% | 55.000 ms | Pass within decoded-frame tolerance |
| Fractional slowdown | 99.2% | 15.813 ms | Pass |
| Crop | 94.1% | 20.000 ms | Correct partial mapping |
| Logo | 99.2% | 20.000 ms | Correct partial mapping |
| Brightness/contrast change | 0% | No accepted mapping | Safe abstention; unsupported coverage |
| Fades with slowdown | 99.2% | 15.813 ms | Correct partial mapping |
| Low-cut periodic motion, 120 ms deletion | 85.3% | 0 ms | Fails coverage target |
| Low-cut nonrepeating pan, 120 ms deletion | 85.6% | 0 ms | Fails coverage target |
| Negative timestamp origin | 100% | 0 ms | Pass |
| Live-action slowdown/ad edits | 95.8% | 5.385 ms | Pass |

The second low-cut case shows that repetition is not the only limitation. The
source frame at 112 seconds matches target 111.88 seconds with mean grayscale
error 0.316, but the nearest retained half-second target sample (112 seconds)
has error 12.078, above the unchanged limit of 6. The cut-supported source
0–50 and 60–110 second intervals survive; the uncertain edit interval and the
tail remain unresolved. More precise sampling needs investigation; do not relax
the image threshold or count these failures as successes.

The live-action source is **Tears of Steel**, attribution **(CC) Blender Foundation
| mango.blender.org**, licensed [CC BY 3.0 under the film's sharing instructions](https://mango.blender.org/sharing/).
The downloader pins the unchanged official MOV's SHA-256, supports bounded
resumable downloads and records provenance beside it. The local test derivatives
remove audio, resize, change playback speed and replace sections with synthetic
ads. The separately published soundtrack's license is not used for these edits.
Both reference movies and derivatives remain ignored. Two films still do not
establish general broadcast-programme accuracy.

Local receipts use `resync-robustness-v3/` and `resync-live-action-v1/` in the
generated directory. A three-cue real CLI run across the live-action case's
three retained blocks also succeeded: all three input hashes, text, cue labels
and CRLF formatting were preserved; maximum output endpoint error after final
millisecond rounding was 5.552 ms. This checks actual CLI publication against the
independent edit list, not GUI interaction or audible synchronization.

## Dense-neighborhood verification results (v3)

Development Mac, FFmpeg 9.0.2, 2026-10-11: the original 16-case corpus passes,
with animated-reference coverage 97.9%; the 11-case live-action corpus passes,
with live-action coverage 96.6% and worst accepted endpoint error 5.385 ms.
An intermediate 80.9% live-action result exposed the endpoint-blending defect
described above, rather than an image-threshold problem. That failure remains
in `resync-live-action-v1/evaluation-v3.json`; the corrected result is
`evaluation-v3-preserved-models.json` in the same directory.

The expanded robustness corpus has 21 cases and passes 20. Fractional-rate,
crop/logo/brightness/fade and negative-origin results retain the v2 table's
outcomes. Updated/new low-cut results:

| Case | Interior coverage | Worst accepted error | Result |
| --- | ---: | ---: | --- |
| Periodic motion, 120 ms deletion | 77.8% | 0 ms | Still fails 90% coverage target |
| Nonrepeating pan, 120 ms deletion | 92.9% | 0 ms | Pass |
| Nonrepeating pan, 120 ms tail deletion | 95.7% | 0 ms | Pass |
| Repeating tail, one 1,760 ms cycle deleted | 87.7% | 0 ms | Pass: ambiguous tail remains unresolved |

The cycle-deletion negative previously had a 1,760 ms wrong accepted mapping
under v2. Dense alternatives now reject that evidence. This case also runs in
the `--extended` CI corpus; it is not subject to the positive-case coverage gate.
The positive periodic-motion case remains an open gate, not silently reclassified
as passing. Receipts are in `resync-robustness-v5/`; old failures remain intact.
The same v3 sampler is used by the evaluator and shared CLI/GUI operation.


## Observed video-edge verification (v4)

`scene-visual-edges-v4` retains the v3 cut fits and dense neighborhood comparisons.
It additionally observes final decoded frames through bounded, sequential EOF
passes. First frames already belong to the whole-video sample set. A first/last
shot with locally repeating motion can extend to an observed video edge only
when the cut model predicts that edge within 1 ms, edge images match, and every
intervening sample supports the model. Static images, missing samples, unmatched
cuts or a materially better image at an alternative offset prevent extension.
The last observed PTS is the limit, not container duration or a guessed final
frame duration. Empty EOF video supplies no boundary; this can occur when audio
outlasts video or a demuxer's duration includes a positive origin.

This is additional content/timestamp evidence, not an assumption that matching
end pictures prove unchanged footage. Equal-duration 120 ms replacements inside
first/last shots and deleted 1,760 ms repeating cycles are explicit controls.
The gap between different cut-derived models stays unresolved. Balanced edits
that are visually indistinguishable between observations remain a limitation of
sampled verification; no universal short-edit detection is claimed.

Development Mac, FFmpeg 9.0.2, 2026-10-11:

| Case | Interior coverage | Worst accepted error | Result |
| --- | ---: | ---: | --- |
| Periodic motion, 120 ms deletion | 93.3% | 0 ms | Pass, unchanged 90% target |
| Periodic motion, 120 ms replacement near start | 94.0% | 0 ms | Pass; replacement not bridged |
| Periodic motion, 120 ms replacement near end | 94.2% | 0 ms | Pass; replacement not bridged |
| One repeating cycle deleted in first shot | 87.5% | 0 ms | Pass; ambiguous first shot unresolved |
| One repeating cycle deleted in final shot | 95.6% | 0 ms | Pass; ambiguous final shot unresolved |
| Nonrepeating pan, 120 ms deletion | 92.9% | 0 ms | Pass |
| Nonrepeating pan, 120 ms tail deletion | 95.7% | 0 ms | Pass |

All 24 robustness cases and all 29 combined extended/robustness cases pass.
The original 16-case/reference and 11-case live-action corpora still pass, with
97.9% and 96.6% reference coverage respectively; the latter's worst accepted
endpoint error remains 5.385 ms. Earlier failing receipts remain unchanged.
Current receipts are `resync-robustness-v6/evaluation-v4.json`,
`resync-combined-v4/evaluation-v4.json`, and `evaluation-v4-edges-trial.json`
in the original and live-action directories.

Reproduce the complete synthetic check:

```sh
python3 scripts/resync-fixtures.py --extended --robustness --output tests/fixtures/generated/resync-combined-v4
cargo run --locked -p media-analysis --example resync-evaluate -- tests/fixtures/generated/resync-combined-v4 > tests/fixtures/generated/resync-combined-v4/evaluation-v4.json
```

The evaluator and shared CLI/GUI operation use the same verifier and EOF observer.
These Mac results do not claim Windows/Linux execution of v4 or native playback.
