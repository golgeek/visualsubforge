#!/usr/bin/env python3
"""Copy a prepared mpv source and add bounded, deferred PipeWire timing traces.

Diagnostic only: does not alter clock results. Callback snapshots are owned by
the PipeWire process thread; AO snapshots use the existing buffer mutex. Both
arrays are dumped only at AO destruction, after the driver thread has stopped.
No per-snapshot allocation or logging runs during playback. Overflow is explicit.
"""

import argparse
import difflib
from pathlib import Path
import shutil


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    edits = {
        "audio/out/ao_pipewire.c": [
            ("struct priv {", """struct vsf_pw_snapshot {
    int64_t local_ns, stream_ns, end_ns;
    struct pw_time time;
    int requested, samples;
};

struct priv {
    struct vsf_pw_snapshot vsf_trace[2048];
    unsigned vsf_count, vsf_dropped;"""),
            ("    int64_t end_time = mp_time_ns();", """    int64_t vsf_local_ns = mp_time_ns();
    int64_t end_time = vsf_local_ns;"""),
            ("    end_time -= pw_stream_get_nsec(p->stream) - time.now;", """    int64_t vsf_stream_ns = pw_stream_get_nsec(p->stream);
    end_time -= vsf_stream_ns - time.now;"""),
            ("    b->size = samples;", """    b->size = samples;
    if (p->vsf_count < MP_ARRAY_SIZE(p->vsf_trace)) {
        p->vsf_trace[p->vsf_count++] = (struct vsf_pw_snapshot){
            vsf_local_ns, vsf_stream_ns, end_time, time, nframes, samples,
        };
    } else {
        p->vsf_dropped++;
    }"""),
            ("    spa_hook_remove(&p->stream_listener);", r'''    MP_INFO(ao, "VSF_PW_TRACE count=%u dropped=%u rate=%d\n",
            p->vsf_count, p->vsf_dropped, ao->samplerate);
    for (unsigned i = 0; i < p->vsf_count; i++) {
        struct vsf_pw_snapshot *s = &p->vsf_trace[i];
        MP_INFO(ao, "VSF_PW local=%lld stream=%lld end=%lld now=%lld "
                "delay=%lld queued=%llu buffered=%llu ticks=%llu "
                "num=%u denom=%u requested=%d samples=%d\n",
                (long long)s->local_ns, (long long)s->stream_ns,
                (long long)s->end_ns, (long long)s->time.now,
                (long long)s->time.delay, (unsigned long long)s->time.queued,
                (unsigned long long)s->time.buffered,
                (unsigned long long)s->time.ticks, s->time.rate.num,
                s->time.rate.denom, s->requested, s->samples);
    }
    spa_hook_remove(&p->stream_listener);'''),
        ],
        "audio/out/buffer.c": [
            ("struct buffer_state {", """struct vsf_ao_snapshot {
    int64_t observed_ns, pending, end_ns;
    double driver_delay;
    bool paused, playing;
};

struct buffer_state {
    struct vsf_ao_snapshot vsf_trace[8192];
    unsigned vsf_count, vsf_dropped;"""),
            ("    mp_mutex_unlock(&p->lock);\n    return driver_delay + pending / (double)ao->samplerate;", """    if (p->vsf_count < MP_ARRAY_SIZE(p->vsf_trace)) {
        p->vsf_trace[p->vsf_count++] = (struct vsf_ao_snapshot){
            mp_time_ns(), pending, p->end_time_ns, driver_delay,
            p->paused, p->playing,
        };
    } else {
        p->vsf_dropped++;
    }
    mp_mutex_unlock(&p->lock);
    return driver_delay + pending / (double)ao->samplerate;"""),
            ("    if (p) {\n        talloc_free(p->filter_root);", r'''    if (p) {
        MP_INFO(ao, "VSF_AO_TRACE count=%u dropped=%u rate=%d\n",
                p->vsf_count, p->vsf_dropped, ao->samplerate);
        for (unsigned i = 0; i < p->vsf_count; i++) {
            struct vsf_ao_snapshot *s = &p->vsf_trace[i];
            MP_INFO(ao, "VSF_AO observed=%lld pending=%lld end=%lld "
                    "driver=%.12f paused=%d playing=%d\n",
                    (long long)s->observed_ns, (long long)s->pending,
                    (long long)s->end_ns, s->driver_delay,
                    s->paused, s->playing);
        }
        talloc_free(p->filter_root);'''),
        ],
    }
    # Validate all anchors before creating output; never modify the input tree.
    changes = {}
    patch = []
    for name, replacements in edits.items():
        before = (args.source / name).read_text()
        after = before
        for old, new in replacements:
            if after.count(old) != 1:
                raise ValueError(f"Expected one source anchor in {name}: {old!r}")
            after = after.replace(old, new)
        changes[name] = after
        patch.extend(difflib.unified_diff(
            before.splitlines(True), after.splitlines(True),
            fromfile="a/" + name, tofile="b/" + name))
    args.output.mkdir(parents=True, exist_ok=False)
    shutil.copytree(args.source, args.output / "source")
    for name, after in changes.items():
        (args.output / "source" / name).write_text(after)
    (args.output / "instrumentation.patch").write_text("".join(patch))


if __name__ == "__main__":
    main()
