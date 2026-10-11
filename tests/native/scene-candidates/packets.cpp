// Bounded H.264/AVCC experiment: read packet sizes and slice types, never decode.
// All FFmpeg objects are local to this process/thread and freed before exit.
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <stdexcept>
#include <string>
extern "C" {
#include <libavformat/avformat.h>
}

struct Bits {
    const uint8_t* data;
    size_t size, byte = 0;
    unsigned bit = 0, zeros = 0;
    uint8_t current = 0;
    unsigned read() {
        if (!bit) {
            if (byte >= size) throw std::runtime_error("Truncated slice header");
            if (zeros >= 2 && data[byte] == 3) {
                ++byte;
                zeros = 0;
                if (byte >= size) throw std::runtime_error("Truncated escape");
            }
            current = data[byte++];
            zeros = current == 0 ? zeros + 1 : 0;
            bit = 8;
        }
        return (current >> --bit) & 1;
    }
    unsigned ue() {
        unsigned leading = 0;
        while (!read()) if (++leading > 30) throw std::runtime_error("Oversized Golomb code");
        unsigned value = 1;
        for (unsigned i = 0; i < leading; ++i) value = (value << 1) | read();
        return value - 1;
    }
};

int main(int argc, char** argv) {
    if (argc != 2) { std::fprintf(stderr, "Usage: scene-packets VIDEO\n"); return 2; }
    AVFormatContext* format = nullptr;
    AVPacket* packet = nullptr;
    int result = 1;
    try {
        av_log_set_level(AV_LOG_ERROR);
        if (avformat_open_input(&format, argv[1], nullptr, nullptr) < 0)
            throw std::runtime_error("Cannot open input");
        // The experiment needs only container stream metadata, no find_stream_info
        // decoder probing. Unsupported/container-incomplete inputs fail explicitly.
        int stream = -1;
        for (unsigned i = 0; i < format->nb_streams; ++i)
            if (format->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO &&
                !(format->streams[i]->disposition & AV_DISPOSITION_ATTACHED_PIC)) {
                stream = static_cast<int>(i); break;
            }
        if (stream < 0) throw std::runtime_error("No real video stream");
        auto* par = format->streams[stream]->codecpar;
        if (par->codec_id != AV_CODEC_ID_H264 || par->extradata_size < 5 || par->extradata[0] != 1)
            throw std::runtime_error("Experiment requires H.264 with AVC configuration (MP4/MKV)");
        unsigned length_bytes = (par->extradata[4] & 3) + 1;
        packet = av_packet_alloc();
        if (!packet) throw std::runtime_error("Packet allocation failed");
        std::puts("pts,size,key,type,reference");
        size_t count = 0;
        int status;
        while ((status = av_read_frame(format, packet)) >= 0) {
            if (packet->stream_index == stream) {
                if (++count > 2000000) throw std::runtime_error("Packet limit exceeded");
                if (packet->pts == AV_NOPTS_VALUE) throw std::runtime_error("Missing presentation timestamp");
                size_t offset = 0;
                int kind = -1, reference = 0, first_slices = 0;
                while (offset < static_cast<size_t>(packet->size)) {
                    if (packet->size - offset < length_bytes) throw std::runtime_error("Truncated NAL length");
                    uint32_t length = 0;
                    for (unsigned i = 0; i < length_bytes; ++i) length = (length << 8) | packet->data[offset++];
                    if (!length || length > packet->size - offset) throw std::runtime_error("Invalid NAL length");
                    unsigned nal = packet->data[offset] & 31;
                    if (nal == 1 || nal == 5) {
                        Bits bits{packet->data + offset + 1, length - 1};
                        if (bits.ue() == 0) ++first_slices;
                        unsigned slice = bits.ue();
                        if (slice > 9) throw std::runtime_error("Invalid slice type");
                        int current = static_cast<int>(slice % 5);
                        if (current > 2) throw std::runtime_error("Switching slices unsupported");
                        if (kind >= 0 && kind != current) throw std::runtime_error("Mixed slice types unsupported");
                        kind = current;
                        reference |= (packet->data[offset] >> 5) & 3;
                    }
                    offset += length;
                }
                if (kind < 0 || first_slices != 1) throw std::runtime_error("Expected one complete picture per packet");
                double pts = packet->pts * av_q2d(format->streams[stream]->time_base);
                if (!std::isfinite(pts)) throw std::runtime_error("Invalid timestamp");
                std::printf("%.9f,%d,%d,%c,%d\n", pts, packet->size,
                            !!(packet->flags & AV_PKT_FLAG_KEY), "PBI"[kind], !!reference);
            }
            av_packet_unref(packet);
        }
        if (status != AVERROR_EOF) throw std::runtime_error("Input read failed");
        if (std::ferror(stdout)) throw std::runtime_error("Output write failed");
        result = 0;
    } catch (const std::exception& e) {
        std::fprintf(stderr, "%s\n", e.what());
    }
    av_packet_free(&packet);
    avformat_close_input(&format);
    return result;
}
