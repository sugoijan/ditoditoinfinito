/* Bounded demux-only ABI for OS decoders. See packets.md. No decoder is
 * opened here; parsing and all FFmpeg allocations stay inside wasm. */
#include <inttypes.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libavformat/avformat.h>
#include <libavutil/pixdesc.h>

#define EXPORT __attribute__((visibility("default"), used))
#define PACKET_LIMIT (8 * 1024 * 1024)
#define CONFIG_LIMIT (1024 * 1024)
#define SIDE_LIMIT (1024 * 1024)
#define SIDE_COUNT_LIMIT 32
#define INDEX_LIMIT 1000000
#define INDEX_PAGE 256
#define HANDLE_LIMIT 8

/* wasm32 is little endian. Fixed records contain no pointers or bitfields. */
typedef struct PacketInfo {
    uint32_t size, flags, data_size, side_size;
    int64_t pts, dts, duration, position;
    uint64_t sequence;
    uint32_t epoch, configuration, side_count, reserved;
} PacketInfo;
typedef struct IndexEntry {
    int64_t timestamp, position;
    int32_t size;
    uint32_t flags;
    int32_t min_distance;
    uint32_t reserved;
} IndexEntry;
_Static_assert(sizeof(PacketInfo) == 72, "packet ABI size");
_Static_assert(offsetof(PacketInfo, pts) == 16 && offsetof(PacketInfo, epoch) == 56,
               "packet ABI offsets");
_Static_assert(sizeof(IndexEntry) == 32, "index ABI size");

typedef struct Demux {
    AVFormatContext *format;
    AVPacket *packet;
    int stream, failed, ended;
    uint8_t *configuration, *initial_configuration, *side;
    uint32_t configuration_size, initial_size;
    PacketInfo info;
    IndexEntry index[INDEX_PAGE];
    char description[4096], error[256];
} Demux;
static unsigned handles;

static void clear_packet(Demux *d) {
    av_packet_unref(d->packet);
    free(d->side);
    d->side = NULL;
    uint64_t sequence = d->info.sequence;
    uint32_t epoch = d->info.epoch, configuration = d->info.configuration;
    d->info = (PacketInfo){ .size = sizeof(PacketInfo), .pts = AV_NOPTS_VALUE,
        .dts = AV_NOPTS_VALUE, .position = -1, .sequence = sequence,
        .epoch = epoch, .configuration = configuration };
}
static int fail(Demux *d, const char *message) {
    clear_packet(d);
    snprintf(d->error, sizeof(d->error), "%s", message);
    d->failed = 1;
    return -1;
}
static void destroy(Demux *d) {
    if (!d) return;
    av_packet_free(&d->packet);
    avformat_close_input(&d->format);
    free(d->configuration);
    free(d->initial_configuration);
    free(d->side);
    free(d);
}
static int copy_configuration(Demux *d, const uint8_t *data, size_t size) {
    if (size > CONFIG_LIMIT || (size && !data)) return -1;
    uint8_t *copy = size ? malloc(size) : NULL;
    if (size && !copy) return -1;
    if (size) memcpy(copy, data, size);
    free(d->configuration);
    d->configuration = copy;
    d->configuration_size = size;
    return 0;
}
/* Integer timestamps are JSON strings: JavaScript numbers lose i64 ticks. */
static const char *tick(int64_t n, char out[32]) {
    if (n == AV_NOPTS_VALUE) return "null";
    snprintf(out, 32, "\"%" PRId64 "\"", n);
    return out;
}
static int describe(Demux *d) {
    AVStream *s = d->format->streams[d->stream];
    AVCodecParameters *c = s->codecpar;
    AVRational rate = av_guess_frame_rate(d->format, s, NULL);
    const AVPixFmtDescriptor *pix = av_pix_fmt_desc_get(c->format);
    char start[32], duration[32], frames[32];
    /* Names come from FFmpeg's static codec/format tables, never file tags. */
    int n = snprintf(d->description, sizeof(d->description),
        "{\"abi\":1,\"codec\":\"%s\",\"container\":\"%s\","
        "\"stream\":%d,\"codec_tag\":%u,\"width\":%d,\"height\":%d,"
        "\"profile\":%d,\"level\":%d,\"pixel_format\":%d,\"bit_depth\":%d,"
        "\"field_order\":%d,\"video_delay\":%d,"
        "\"time_base\":[%d,%d],\"frame_rate\":[%d,%d],"
        "\"start\":%s,\"duration\":%s,\"frames\":%s,"
        "\"aspect_ratio\":[%d,%d],\"color_range\":%d,\"color_space\":%d,"
        "\"color_primaries\":%d,\"color_transfer\":%d,\"chroma_location\":%d,"
        "\"configuration_size\":%u}",
        avcodec_get_name(c->codec_id), d->format->iformat->name, d->stream,
        c->codec_tag, c->width, c->height, c->profile, c->level, c->format,
        pix ? pix->comp[0].depth : c->bits_per_raw_sample, c->field_order,
        c->video_delay, s->time_base.num, s->time_base.den, rate.num, rate.den,
        tick(s->start_time, start), tick(s->duration, duration),
        tick(s->nb_frames ? s->nb_frames : AV_NOPTS_VALUE, frames),
        c->sample_aspect_ratio.num, c->sample_aspect_ratio.den,
        c->color_range, c->color_space, c->color_primaries, c->color_trc,
        c->chroma_location, d->configuration_size);
    return n >= 0 && (size_t)n < sizeof(d->description) ? 0 : -1;
}

EXPORT int ddi_demux_abi(void) { return 1; }
EXPORT Demux *ddi_demux_open(const char *path) {
    if (handles >= HANDLE_LIMIT) return NULL;
    Demux *d = calloc(1, sizeof(*d));
    if (!d) return NULL;
    d->format = avformat_alloc_context();
    d->packet = av_packet_alloc();
    if (!d->format || !d->packet) goto bad;
    d->format->max_streams = 32;
    d->format->probesize = 1024 * 1024;
    d->format->max_analyze_duration = 5 * AV_TIME_BASE;
    d->format->max_probe_packets = 2500;
    d->format->max_index_size = 32 * 1024 * 1024;
    if (avformat_open_input(&d->format, path, NULL, NULL) < 0 ||
        avformat_find_stream_info(d->format, NULL) < 0) goto bad;
    /* NULL decoder_ret deliberately allows codecs not built into the module. */
    d->stream = av_find_best_stream(d->format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (d->stream < 0) goto bad;
    AVStream *s = d->format->streams[d->stream];
    AVCodecParameters *c = s->codecpar;
    if (s->time_base.num <= 0 || s->time_base.den <= 0 || c->width <= 0 ||
        c->height <= 0 || c->extradata_size < 0 ||
        copy_configuration(d, c->extradata, c->extradata_size) < 0) goto bad;
    d->initial_size = d->configuration_size;
    if (d->initial_size) {
        d->initial_configuration = malloc(d->initial_size);
        if (!d->initial_configuration) goto bad;
        memcpy(d->initial_configuration, d->configuration, d->initial_size);
    }
    for (unsigned i = 0; i < d->format->nb_streams; i++)
        if ((int)i != d->stream) d->format->streams[i]->discard = AVDISCARD_ALL;
    clear_packet(d);
    if (describe(d) < 0) goto bad;
    handles++;
    return d;
bad:
    destroy(d);
    return NULL;
}
EXPORT void ddi_demux_close(Demux *d) {
    if (!d) return;
    destroy(d);
    handles--;
}
EXPORT const char *ddi_demux_info(Demux *d) { return d->description; }
EXPORT const char *ddi_demux_error(Demux *d) { return d->error; }
EXPORT const uint8_t *ddi_demux_config(Demux *d) { return d->configuration; }
EXPORT unsigned ddi_demux_config_size(Demux *d) { return d->configuration_size; }
EXPORT const PacketInfo *ddi_demux_packet_info(Demux *d) { return &d->info; }
EXPORT const uint8_t *ddi_demux_data(Demux *d) { return d->info.data_size ? d->packet->data : NULL; }
EXPORT const uint8_t *ddi_demux_side_data(Demux *d) { return d->side; }

EXPORT int ddi_demux_next(Demux *d) {
    if (d->failed) return -1;
    clear_packet(d);
    if (d->ended) return 0;
    int found = 0;
    for (int skipped = 0; skipped < 4096; skipped++) {
        int rc = av_read_frame(d->format, d->packet);
        if (rc == AVERROR_EOF) { d->ended = 1; return 0; }
        if (rc < 0) return fail(d, "cannot read the next packet");
        if (d->packet->stream_index == d->stream) { found = 1; break; }
        av_packet_unref(d->packet);
    }
    if (!found) return fail(d, "too many packets before the next video packet");
    AVPacket *p = d->packet;
    if (p->size < 0 || p->size > PACKET_LIMIT || (p->size && !p->data))
        return fail(d, "compressed packet exceeds the byte limit");
    if (p->side_data_elems < 0 || p->side_data_elems > SIDE_COUNT_LIMIT)
        return fail(d, "packet exceeds the side-data count limit");
    size_t total = 0;
    int changed = 0;
    for (int i = 0; i < p->side_data_elems; i++) {
        AVPacketSideData *s = &p->side_data[i];
        if (s->size > SIDE_LIMIT - 8 || total > SIDE_LIMIT - 8 - s->size ||
            (s->size && !s->data)) return fail(d, "packet exceeds the side-data byte limit");
        total += 8 + s->size;
        if (s->type == AV_PKT_DATA_NEW_EXTRADATA || s->type == AV_PKT_DATA_PARAM_CHANGE)
            changed = 1;
        if (s->type == AV_PKT_DATA_NEW_EXTRADATA && s->size > CONFIG_LIMIT)
            return fail(d, "new configuration exceeds the byte limit");
    }
    if (d->info.sequence == UINT64_MAX || (changed && d->info.configuration == UINT32_MAX))
        return fail(d, "packet generation exhausted");
    d->side = total ? malloc(total) : NULL;
    if (total && !d->side) return fail(d, "cannot allocate packet side data");
    size_t at = 0;
    for (int i = 0; i < p->side_data_elems; i++) {
        AVPacketSideData *s = &p->side_data[i];
        uint32_t kind = s->type, size = s->size;
        memcpy(d->side + at, &kind, 4);
        memcpy(d->side + at + 4, &size, 4);
        if (size) memcpy(d->side + at + 8, s->data, size);
        at += 8 + size;
        if (s->type == AV_PKT_DATA_NEW_EXTRADATA &&
            copy_configuration(d, s->data, s->size) < 0)
            return fail(d, "cannot copy the new configuration");
    }
    d->info.flags = ((p->flags & AV_PKT_FLAG_KEY) ? 1 : 0) |
        ((p->flags & AV_PKT_FLAG_CORRUPT) ? 2 : 0) |
        ((p->flags & AV_PKT_FLAG_DISCARD) ? 4 : 0) | (changed ? 8 : 0);
    d->info.data_size = p->size;
    d->info.side_size = total;
    d->info.pts = p->pts;
    d->info.dts = p->dts;
    d->info.duration = p->duration;
    d->info.position = p->pos;
    /* First packet is sequence 1. */
    d->info.sequence++;
    d->info.configuration += changed;
    d->info.side_count = p->side_data_elems;
    return 1;
}
EXPORT int ddi_demux_seek(Demux *d, int64_t timestamp) {
    if (timestamp == AV_NOPTS_VALUE || d->info.epoch == UINT32_MAX)
        return fail(d, "invalid seek timestamp or exhausted generation");
    if (av_seek_frame(d->format, d->stream, timestamp, AVSEEK_FLAG_BACKWARD) < 0)
        return fail(d, "cannot seek to a key packet");
    if (copy_configuration(d, d->initial_configuration, d->initial_size) < 0)
        return fail(d, "cannot restore the initial configuration");
    clear_packet(d);
    d->info.epoch++;
    d->info.sequence = 0;
    d->info.configuration = 0;
    d->failed = d->ended = 0;
    d->error[0] = 0;
    return 0;
}
EXPORT int ddi_demux_index_count(Demux *d) {
    int n = avformat_index_get_entries_count(d->format->streams[d->stream]);
    return n >= 0 && n <= INDEX_LIMIT ? n : -1;
}
EXPORT int ddi_demux_index(Demux *d, unsigned first, unsigned count) {
    int n = ddi_demux_index_count(d);
    if (n < 0 || first > (unsigned)n || count > INDEX_PAGE) return -1;
    if (count > (unsigned)n - first) count = (unsigned)n - first;
    memset(d->index, 0, sizeof(d->index));
    for (unsigned i = 0; i < count; i++) {
        const AVIndexEntry *e = avformat_index_get_entry(d->format->streams[d->stream], first + i);
        if (!e) return -1;
        d->index[i] = (IndexEntry){ .timestamp = e->timestamp, .position = e->pos,
            .size = e->size, .flags = ((e->flags & AVINDEX_KEYFRAME) ? 1 : 0) |
            ((e->flags & AVINDEX_DISCARD_FRAME) ? 2 : 0), .min_distance = e->min_distance };
    }
    return count;
}
EXPORT const IndexEntry *ddi_demux_index_data(Demux *d) { return d->index; }
