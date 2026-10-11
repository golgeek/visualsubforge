// Diagnostic capture of explicitly linked test-stream ports, never a default
// microphone/sink. The Python wrapper verifies the player PID and creates the
// sole link. All callbacks run on the main loop; no RT_PROCESS flag is used.
// Bounded buffers are allocated before connection, written only after teardown.
#include <errno.h>
#include <inttypes.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>
#include <spa/param/buffers.h>

#define RATE 48000
#define BLOCKS 8192
struct block {
    uint64_t offset, frames, cycle;
    int64_t received;
    int time_result;
    struct pw_time time;
    bool header;
    struct spa_meta_header meta;
};
struct capture {
    struct pw_main_loop *loop;
    struct pw_stream *stream;
    float *pcm;
    struct block *blocks;
    size_t capacity, frames;
    unsigned count, dropped;
    bool format_ok, failed;
};
static void finish(void *opaque, uint64_t expirations) {
    (void)expirations;
    struct capture *c = opaque;
    pw_main_loop_quit(c->loop);
}
static void interrupted(void *opaque, int number) {
    (void)number;
    finish(opaque, 0);
}
static void state_changed(void *opaque, enum pw_stream_state old,
                          enum pw_stream_state state, const char *error) {
    (void)old;
    struct capture *c = opaque;
    if (state == PW_STREAM_STATE_ERROR) {
        fprintf(stderr, "stream error: %s\n", error ? error : "unknown");
        c->failed = true;
        pw_main_loop_quit(c->loop);
    }
}
static void format_changed(void *opaque, uint32_t id, const struct spa_pod *param) {
    struct capture *c = opaque;
    if (id != SPA_PARAM_Format) return;
    if (!param) { c->format_ok = false; return; }
    struct spa_audio_info_raw raw = {0};
    c->format_ok = spa_format_audio_raw_parse(param, &raw) >= 0 &&
        raw.format == SPA_AUDIO_FORMAT_F32 && raw.rate == RATE && raw.channels == 1;
    if (!c->format_ok) {
        c->failed = true;
        pw_main_loop_quit(c->loop);
        return;
    }
    uint8_t bytes[256];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(bytes, sizeof bytes);
    const struct spa_pod *metadata = spa_pod_builder_add_object(&builder,
        SPA_TYPE_OBJECT_ParamMeta, SPA_PARAM_Meta,
        SPA_PARAM_META_type, SPA_POD_Id(SPA_META_Header),
        SPA_PARAM_META_size, SPA_POD_Int(sizeof(struct spa_meta_header)));
    if (pw_stream_update_params(c->stream, &metadata, 1) < 0) {
        c->failed = true;
        pw_main_loop_quit(c->loop);
    }
}
static void process(void *opaque) {
    struct capture *c = opaque;
    struct pw_buffer *b;
    while ((b = pw_stream_dequeue_buffer(c->stream))) {
        struct spa_buffer *buf = b->buffer;
        if (!c->format_ok || buf->n_datas != 1 || !buf->datas[0].data || !buf->datas[0].chunk) {
            c->failed = true;
        } else {
            struct spa_data *data = &buf->datas[0];
            uint32_t offset = data->chunk->offset, size = data->chunk->size;
            if (offset > data->maxsize || size > data->maxsize - offset || size % sizeof(float)) {
                c->failed = true;
            } else if (size) {
                size_t frames = size / sizeof(float);
                if (c->count == BLOCKS || frames > c->capacity - c->frames) {
                    c->dropped++;
                    c->failed = true;
                } else {
                    struct block *s = &c->blocks[c->count++];
                    struct timespec received;
                    clock_gettime(CLOCK_MONOTONIC, &received);
                    s->received = (int64_t)received.tv_sec * 1000000000 + received.tv_nsec;
                    s->offset = c->frames;
                    s->frames = frames;
                    s->cycle = b->time;
                    s->time_result = pw_stream_get_time_n(c->stream, &s->time, sizeof s->time);
                    struct spa_meta_header *meta = spa_buffer_find_meta_data(buf, SPA_META_Header, sizeof *meta);
                    s->header = meta != NULL;
                    if (meta) s->meta = *meta;
                    memcpy(c->pcm + c->frames, (char *)data->data + offset, size);
                    c->frames += frames;
                }
            }
        }
        pw_stream_queue_buffer(c->stream, b);
        if (c->failed) {
            pw_main_loop_quit(c->loop);
            break;
        }
    }
}
static const struct pw_stream_events events = {
    PW_VERSION_STREAM_EVENTS,
    .state_changed = state_changed, .param_changed = format_changed, .process = process,
};
int main(int argc, char **argv) {
    if (argc != 3) { fprintf(stderr, "usage: pipewire-stream-capture SECONDS OUTPUT.f32\n"); return 2; }
    char *end;
    long seconds = strtol(argv[1], &end, 10);
    if (*end || seconds < 1 || seconds > 60) return 2;
    FILE *pcm = fopen(argv[2], "wbx");
    if (!pcm) { perror("PCM exclusive create"); return 2; }
    char *json_path = NULL;
    if (asprintf(&json_path, "%s.json", argv[2]) < 0) { fclose(pcm); return 2; }
    FILE *json = fopen(json_path, "wx");
    free(json_path);
    if (!json) { perror("JSON exclusive create"); fclose(pcm); return 2; }
    struct capture c = {.capacity = (size_t)(seconds + 2) * RATE};
    c.pcm = calloc(c.capacity, sizeof(float));
    c.blocks = calloc(BLOCKS, sizeof(struct block));
    if (!c.pcm || !c.blocks) { fclose(pcm); fclose(json); free(c.pcm); free(c.blocks); return 2; }
    pw_init(NULL, NULL);
    c.loop = pw_main_loop_new(NULL);
    if (!c.loop) { c.failed = true; goto cleanup; }
    struct pw_loop *loop = pw_main_loop_get_loop(c.loop);
    pw_loop_add_signal(loop, SIGTERM, interrupted, &c);
    pw_loop_add_signal(loop, SIGINT, interrupted, &c);
    struct spa_source *timer = pw_loop_add_timer(loop, finish, &c);
    struct timespec timeout = {.tv_sec = seconds};
    if (!timer || pw_loop_update_timer(loop, timer, &timeout, NULL, false) < 0) {
        c.failed = true; goto cleanup;
    }
    c.stream = pw_stream_new_simple(loop, "VSF isolated graph capture",
        pw_properties_new(PW_KEY_MEDIA_TYPE, "Audio", PW_KEY_MEDIA_CATEGORY, "Capture",
            PW_KEY_MEDIA_ROLE, "Production", PW_KEY_NODE_NAME, "vsf-graph-capture",
            PW_KEY_NODE_AUTOCONNECT, "false", PW_KEY_NODE_PASSIVE, "true", NULL), &events, &c);
    if (!c.stream) { c.failed = true; goto cleanup; }
    uint8_t bytes[512];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(bytes, sizeof bytes);
    const struct spa_pod *format = spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat,
        &SPA_AUDIO_INFO_RAW_INIT(.format = SPA_AUDIO_FORMAT_F32, .rate = RATE, .channels = 1,
                                .position = {SPA_AUDIO_CHANNEL_MONO}));
    if (pw_stream_connect(c.stream, PW_DIRECTION_INPUT, PW_ID_ANY,
            PW_STREAM_FLAG_MAP_BUFFERS | PW_STREAM_FLAG_DONT_RECONNECT, &format, 1) < 0) {
        c.failed = true; goto cleanup;
    }
    // No AUTOCONNECT: only explicit links from the wrapper can supply samples.
    pw_main_loop_run(c.loop);
cleanup:
    if (c.stream) pw_stream_destroy(c.stream);
    if (c.loop) pw_main_loop_destroy(c.loop);
    pw_deinit();
    if (!c.frames) c.failed = true;
    if (fwrite(c.pcm, sizeof(float), c.frames, pcm) != c.frames) c.failed = true;
    if (fclose(pcm)) c.failed = true;
    fprintf(json, "{\"capture_kind\":\"pipewire-explicit-stream-graph\",\"format\":\"float32-native\",\"sample_rate\":%d,"
        "\"channels\":1,\"frames\":%zu,\"dropped\":%u,\"failed\":%s,\"blocks\":[\n",
        RATE, c.frames, c.dropped, c.failed ? "true" : "false");
    for (unsigned i = 0; i < c.count; ++i) {
        struct block *s = &c.blocks[i];
        fprintf(json, "%s{\"offset\":%"PRIu64",\"frames\":%"PRIu64",\"cycle_ns\":%"PRIu64
            ",\"received_ns\":%"PRId64",\"time_result\":%d,\"now\":%"PRId64
            ",\"ticks\":%"PRIu64",\"delay\":%"PRId64",\"rate_num\":%u,\"rate_denom\":%u,"
            "\"header\":%s,\"pts\":%"PRId64",\"seq\":%"PRIu64",\"flags\":%u}",
            i ? ",\n" : "", s->offset, s->frames, s->cycle, s->received, s->time_result,
            s->time.now, s->time.ticks, s->time.delay, s->time.rate.num, s->time.rate.denom,
            s->header ? "true" : "false", s->meta.pts, s->meta.seq, s->meta.flags);
    }
    fprintf(json, "\n]}\n");
    if (ferror(json)) c.failed = true;
    if (fclose(json)) c.failed = true;
    free(c.pcm); free(c.blocks);
    return c.failed ? 1 : 0;
}
