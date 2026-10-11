# Compressed scene candidates — bounded experiment

This is an offline H.264 experiment, not an application backend or accepted dependency.
It tests non-keyframe I pictures and packet-size outliers without reconstructing
video pixels. [Task evidence](../../../docs/tasks/008-compressed-scene-candidates.md)
records the actual speed, sampled precision, reference agreement and failures.

## Build and check

Requires a C++17 compiler and development headers/libraries for libavformat,
libavcodec and libavutil. On the development Mac those already come from FFmpeg.
No legacy source or runtime is used.

```sh
clang++ -std=c++17 -Wall -Wextra -Werror tests/native/scene-candidates/packets.cpp $(pkg-config --cflags --libs libavformat libavcodec libavutil) -o tests/fixtures/generated/scene-packets
python3 tests/native/scene-candidates/check.py tests/fixtures/generated/scene-packets tests/fixtures/generated
```

`check.py` compares packet PTS, picture type and key flags with actual decoded
FFprobe frames on the fixed-GOP and continuous-motion fixtures. It checks explicit
unsupported/audio/missing-input failures and one-to-one reference matching. Generate
the fixtures with `scripts/scene-fixtures.py` if missing. The fixed-GOP fixture is
also an intentional quality failure: the shortcut misses both real cuts when
scene-based intra-frame decisions are disabled.

## Study

First create a matching standard-detail baseline with the existing benchmark:

```sh
python3 scripts/scene-benchmark.py /path/video.mkv --seconds 0 --repeats 1 --cases detect-640-28 --output tests/fixtures/generated/candidate-baseline.json
python3 tests/native/scene-candidates/study.py /path/video.mkv --extractor tests/fixtures/generated/scene-packets --baseline tests/fixtures/generated/candidate-baseline.json --output tests/fixtures/generated/candidate-study --verify
```

The study validates matching stream/format metadata, but not a content hash: pass
the SAME input for both commands. Output retains metadata, byte size, FFmpeg
version, three extraction timings, exact candidate/reference matches, size-ratio
experiments and local verification scores. It omits the input path. Timings
include local reads with uncontrolled OS cache; no cold-cache promise. Raw packets
stay compressed and local. Comparison with another detector is not ground truth.

Optional `--verify` uses eight subprocess workers by default (`--workers 1..8`),
two decoder threads each, seeking to 120 ms before each candidate and reading
280 ms. Prerequisite frames are decoded from the earlier random-access point.
It compares 320-wide images and records scdet scores, including adjacent context;
threshold 5 was selected exploratorily in this study. Candidate timestamps must
have two preceding decoded images and a unique score. Timeouts/errors invalidate
the run; no partial result is called successful. This verifier is explicitly
limited to zero-origin CFR25 input. General VFR/origin handling is future work.

## Visual review

`review-sample.json` uses seed 8003, sampling 20 standard-matching candidates and
40 additional candidates without replacement (or all if fewer). This deliberately
oversamples disagreement; it is not an unweighted random sample of all cuts.
To render adjacent frames for inspection, with Pillow available:

```sh
python3 tests/native/scene-candidates/review.py /path/video.mkv tests/fixtures/generated/candidate-study/review-sample.json tests/fixtures/generated/candidate-study/review
```

Run in a new output directory; FFmpeg will not overwrite existing images.
The renderer assumes one packet per decoded frame and zero-origin CFR25. Its
frame indices were cross-checked against decoded FFprobe output on this workload.
The second review used seed 8004 and 10/30 disjoint candidates; its manifest and
manual labels are retained with ignored evidence. No user images are source assets.

## Ownership and limitations

`packets.cpp` is a single-threaded, process-local libavformat reader. The format
context and packet are released on every success/exception path; packets are
unreferenced between reads. It never opens a decoder or calls decoder probing.
It accepts H.264 AVC configuration with length-prefixed NAL units, parses only
first-macroblock and slice-type Exp-Golomb fields, and combines consistent slices
within one picture. It rejects missing PTS, unsupported/mixed slice types, multiple
pictures per packet, incomplete NALs, oversized Golomb codes and over two million
video packets. The Python runner adds a 60-second extraction timeout and rejects
duplicate/nonmonotonic presentation timestamps after sorting. This is not a
hardened arbitrary-media parser or an in-process VSF dependency.

All-I does not mean keyframe: non-IDR I pictures can occur between periodic seek
checkpoints. Conversely encoders may put every cut on a keyframe, or never choose
extra I pictures. No heuristic here can certify its own recall from the presence
or absence of candidates. Low candidate count must not be reported as confirmed
absence of cuts. H.265/AV1/Annex-B/interlaced and platform/runtime coverage remain
unverified or unsupported. No native app, project schema or production tests changed.
