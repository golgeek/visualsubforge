# Bounded audio completion candidates

These three changes implement the additional scope approved in decision 0007.
They are explicitly selected with `prepare-mpv-clock.py --audio-candidates` and
apply after the ten-patch baseline. Their manifest and patch bytes are pinned;
they are not yet the default or an accepted cross-platform runtime.

1. `0010-scaletempo-eof.patch` checks remaining output time when deciding whether
   stretched final audio is available. Existing final-packet clipping remains.
2. `0011-queued-eof-resume.patch` remembers pull streaming state across hardware
   pause, including when submitted output remains after software EOF. It shifts
   the presentation deadline by the logical pause interval while holding the
   buffer lock. This fixes transport completion; it does not establish seamless
   AVFoundation captured content.
3. `0012-coreaudio-continuation.patch` exposes a read-only local continuation
   endpoint and capability marker. At an exact loop discontinuity it chooses the
   end of the submitted interval, not the start of the next unsubmitted wrap.
   The existing audible-clock lookup keeps its original right-side behavior.

## Continuation API and ownership

`vsf-audio-continuation-version` is the read-only string `1`.
`vsf-audio-continuation-pts` returns source seconds only during initialized,
paused CoreAudio PCM playback with known queue history. It is unavailable while
playing, before load, after teardown, on other output backends, for non-PCM
formats, or when required history is unknown/expired. An untouched queue maps
to its known start. It does not estimate speaker presentation.

The existing core owner maps queued source intervals. The audio buffer mutex
protects the pending frame and asynchronous queue sample count during the
snapshot; paused pull callbacks cannot consume more data afterward. The read
allocates nothing, retains no resources and does not call native driver code
under that mutex. Existing flush/replacement resets invalidate the history.

VSF's existing media control worker pauses output, reads the continuation
endpoint, clears both loop endpoints, performs an exact seek, and restores the
original pause state. This path is selected only for CoreAudio with capability
version 1. Missing continuation is an error with endpoint/pause restoration;
there is no fallback to the earlier audible position. Follow and the visible
playhead still read `audio-pts`. Other backends/older libraries keep their
existing policy and its recorded limitations; the new endpoint is not treated
as a portable flush contract.

## Checks

From the repository root, using a privately built candidate and same-base
baseline. The synthetic reference is the existing two-second stereo 48 kHz
PCM16 fixture from task 005; choose new output directories.

```sh
python3 tests/native/audio-stretch-eof-regression-test.py
python3 tests/native/audio-stretch-eof-regression.py --mpv CANDIDATE --control BASELINE --reference REFERENCE.wav --output-dir NEW_STRETCH_DIRECTORY
python3 tests/native/audio-eof-regression.py --mpv CANDIDATE --ao coreaudio --paused-reset --output-dir NEW_EOF_DIRECTORY
python3 tests/native/audio-continuation-state.py --mpv CANDIDATE --reference REFERENCE.wav --backend coreaudio --output-dir NEW_STATE_DIRECTORY
rustc --edition=2024 --test --cfg 'feature="media-prototype"' tests/native/media-transport-test.rs -o /tmp/vsf-media-transport-test
/tmp/vsf-media-transport-test
```

The stretch oracle preserves every control sample and compares added tails to
the original algorithm's delayed-EOF reference. Unchanged outputs require exact
control equality; short-input final rounding can differ from the extended input
even in the control. These differences remain reported. Negative controls reject
zero padding, truncation, repetition, changed prefixes and incomplete frames.
It is not an independent WSOLA quality or loop-transition oracle.

The state probe checks three rates, unavailable/paused/read-only behavior,
clear/EOF/stop and unsupported-backend rejection. The Rust harness compiles the
actual application policy against a fake client to test transaction rollback
and separation from the Follow clock. Neither checks captured audio. Task 005
records the separate process captures, application smoke, failures and remaining
rate/content/device/platform gates. Do not promote these candidates based only
on a successful build or property/state test.
