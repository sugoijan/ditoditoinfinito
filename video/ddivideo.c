// Video decoder shim for the browser worker; `video/build.sh` links it with
// a cut-down FFmpeg into `ddivideo.wasm` (a WASI reactor, no JavaScript of
// its own). Opens a file through libavformat, decodes the best video stream
// and hands each frame over as 8-bit YUV 4:2:0 planes in one buffer: Y
// (w × h), then U and V ((w + 1) / 2 × (h + 1) / 2 each), rows packed with
// no padding. The renderer converts to RGB with the matrix and range that
// `ddi_colorspace` and `ddi_full_range` report for the frame.
//
// Not thread-safe (FFmpeg is built without threads); one worker owns it.
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/pixdesc.h>
#include <libswscale/swscale.h>

#define EXPORT __attribute__((visibility("default"), used))

// Larger frames are refused rather than allocated (packs ship SD and 720p).
#define MAX_PIXELS (3840 * 2160)

// wasi-libc declares `clock()` but this sysroot does not define it;
// libavutil's random seed calls it.
clock_t clock(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (clock_t)(ts.tv_sec * 1000000 + ts.tv_nsec / 1000);
}

typedef struct {
    char *path;
    AVFormatContext *fmt;
    AVCodecContext *dec;
    AVPacket *pkt;
    AVFrame *frame;
    int stream;
    double time_base;  // seconds per stream tick
    int64_t start;     // stream start time in ticks, 0 when unknown
    double frame_time; // seconds per frame, from the stream's frame rate
    double last_pts;   // of the last frame decoded, -1 before the first
    double last_dts;   // of the last packet sent to the decoder, -1 unknown
    double skip_until; // after a seek: the target time, -1 when not seeking
    // While seeking: the last frame that starts by the target, kept until a
    // later one shows it is the one showing at the target; and that later
    // frame, handed over next.
    AVFrame *held, *pending;
    double held_t, held_length, pending_t;
    int has_held, has_pending;
    int seek_fresh;    // no frame decoded since the last seek
    int64_t seek_key;  // dts of the first packet read since the seek
    int seek_retries;  // seeks back to an earlier keyframe for the same target
    int seek_reopened; // the current seek decodes from a reopened file
    // The frame handed over: its planes, size and colour tags.
    uint8_t *planes;
    size_t planes_cap;
    struct SwsContext *sws; // for pixel formats copied through swscale
    int width, height;
    int colorspace; // 601 or 709
    int full_range;
} Video;

static int guess_colorspace(enum AVColorSpace cs, int height) {
    switch (cs) {
    case AVCOL_SPC_BT709:
        return 709;
    case AVCOL_SPC_BT470BG:
    case AVCOL_SPC_SMPTE170M:
        return 601;
    default:
        // Untagged: what players assume, SD is BT.601 and HD is BT.709.
        return height < 720 ? 601 : 709;
    }
}

static void close_streams(Video *v) {
    av_frame_free(&v->frame);
    av_frame_free(&v->held);
    av_frame_free(&v->pending);
    v->has_held = v->has_pending = 0;
    av_packet_free(&v->pkt);
    avcodec_free_context(&v->dec);
    avformat_close_input(&v->fmt);
}

static int open_streams(Video *v) {
    if (avformat_open_input(&v->fmt, v->path, NULL, NULL) < 0) return -1;
    if (avformat_find_stream_info(v->fmt, NULL) < 0) return -1;
    const AVCodec *codec = NULL;
    v->stream = av_find_best_stream(v->fmt, AVMEDIA_TYPE_VIDEO, -1, -1, &codec, 0);
    if (v->stream < 0 || !codec) return -1;
    // Audio and other streams are read past without being parsed.
    for (unsigned i = 0; i < v->fmt->nb_streams; i++)
        if ((int)i != v->stream) v->fmt->streams[i]->discard = AVDISCARD_ALL;
    AVStream *st = v->fmt->streams[v->stream];
    v->dec = avcodec_alloc_context3(codec);
    if (!v->dec || avcodec_parameters_to_context(v->dec, st->codecpar) < 0) return -1;
    v->dec->workaround_bugs = FF_BUG_AUTODETECT;
    v->dec->error_concealment = FF_EC_GUESS_MVS | FF_EC_DEBLOCK;
    v->dec->max_pixels = MAX_PIXELS;
    if (avcodec_open2(v->dec, codec, NULL) < 0) return -1;
    v->pkt = av_packet_alloc();
    v->frame = av_frame_alloc();
    v->held = av_frame_alloc();
    v->pending = av_frame_alloc();
    if (!v->pkt || !v->frame || !v->held || !v->pending) return -1;
    v->time_base = av_q2d(st->time_base);
    v->start = st->start_time == AV_NOPTS_VALUE ? 0 : st->start_time;
    AVRational rate = av_guess_frame_rate(v->fmt, st, NULL);
    v->frame_time = rate.num > 0 && rate.den > 0 ? av_q2d(av_inv_q(rate)) : 1.0 / 30.0;
    v->last_pts = -1.0;
    v->last_dts = -1.0;
    v->skip_until = -1.0;
    return 0;
}

// Opens `path` (inside the worker's preopened directory); NULL on failure.
EXPORT Video *ddi_open(const char *path) {
    av_log_set_level(AV_LOG_ERROR);
    Video *v = calloc(1, sizeof *v);
    if (!v) return NULL;
    v->path = strdup(path);
    if (!v->path || open_streams(v) < 0) {
        close_streams(v);
        free(v->path);
        free(v);
        return NULL;
    }
    v->colorspace = guess_colorspace(v->dec->colorspace, v->dec->height);
    v->full_range = v->dec->color_range == AVCOL_RANGE_JPEG;
    return v;
}

// Size of the last frame handed over, or the stream's before the first.
EXPORT int ddi_width(Video *v) { return v->width ? v->width : v->dec ? v->dec->width : 0; }
EXPORT int ddi_height(Video *v) { return v->height ? v->height : v->dec ? v->dec->height : 0; }
EXPORT const char *ddi_codec(Video *v) { return v->dec ? avcodec_get_name(v->dec->codec_id) : ""; }
EXPORT int ddi_colorspace(Video *v) { return v->colorspace; }
EXPORT int ddi_full_range(Video *v) { return v->full_range; }
EXPORT uint8_t *ddi_planes(Video *v) { return v->planes; }
EXPORT size_t ddi_planes_size(Video *v) {
    size_t cw = (v->width + 1) / 2, ch = (v->height + 1) / 2;
    return (size_t)v->width * v->height + 2 * cw * ch;
}

// Length of the video stream in seconds, or -1 when the container does not
// say (a raw MPEG stream's length is only a guess from its bitrate; the end
// is known when `ddi_next` reaches it).
EXPORT double ddi_duration(Video *v) {
    if (!v->fmt) return -1.0;
    if (v->fmt->duration_estimation_method == AVFMT_DURATION_FROM_BITRATE) return -1.0;
    AVStream *st = v->fmt->streams[v->stream];
    if (st->duration > 0) return st->duration * v->time_base;
    return v->fmt->duration > 0 ? v->fmt->duration / (double)AV_TIME_BASE : -1.0;
}

// Next decoded frame of the stream into `frame`: 0, AVERROR_EOF at the end,
// another negative code when a frame failed to decode (calling again goes on
// with the next one).
static int receive(Video *v) {
    for (;;) {
        int r = avcodec_receive_frame(v->dec, v->frame);
        if (r != AVERROR(EAGAIN)) return r;
        r = av_read_frame(v->fmt, v->pkt);
        if (r < 0) {
            // The end, or a read error (truncated files are common): drain
            // what the decoder holds.
            r = avcodec_send_packet(v->dec, NULL);
            if (r < 0 && r != AVERROR_EOF) return r;
            continue;
        }
        // A packet that fails to decode is skipped, as StepMania does.
        if (v->pkt->stream_index == v->stream) {
            if (v->seek_fresh && v->seek_key == AV_NOPTS_VALUE) v->seek_key = v->pkt->dts;
            if (v->pkt->dts != AV_NOPTS_VALUE) v->last_dts = (v->pkt->dts - v->start) * v->time_base;
            avcodec_send_packet(v->dec, v->pkt);
        }
        av_packet_unref(v->pkt);
    }
}

static inline unsigned sample(const uint8_t *row, int x, int wide) {
    return wide ? ((const uint16_t *)row)[x] : row[x];
}

// One plane of `sw` × `sh` samples scaled to `w` × `h` at 8 bits,
// averaging the source samples an output sample covers. `lw`/`lh`: log2 of
// the source's subsampling against the output (-1: twice as dense, 0: the
// same, 1: half as dense).
static void copy_plane(uint8_t *dst, int w, int h, const uint8_t *src, int stride, int sw, int sh,
                       int lw, int lh, int depth) {
    int wide = depth > 8;
    if (!wide && lw == 0 && lh == 0) {
        for (int y = 0; y < h; y++) memcpy(dst + (size_t)y * w, src + (size_t)y * stride, w);
        return;
    }
    unsigned shift = (lw < 0) + (lh < 0) + (depth - 8);
    unsigned half = shift ? 1u << (shift - 1) : 0;
    for (int y = 0; y < h; y++) {
        int y0 = lh < 0 ? 2 * y : y >> lh;
        if (y0 > sh - 1) y0 = sh - 1;
        int y1 = lh < 0 && y0 + 1 < sh ? y0 + 1 : y0;
        const uint8_t *r0 = src + (size_t)y0 * stride, *r1 = src + (size_t)y1 * stride;
        for (int x = 0; x < w; x++) {
            int x0 = lw < 0 ? 2 * x : x >> lw;
            if (x0 > sw - 1) x0 = sw - 1;
            int x1 = lw < 0 && x0 + 1 < sw ? x0 + 1 : x0;
            unsigned s = sample(r0, x0, wide);
            if (lw < 0) s += sample(r0, x1, wide);
            if (lh < 0) {
                s += sample(r1, x0, wide);
                if (lw < 0) s += sample(r1, x1, wide);
            }
            unsigned o = (s + half) >> shift;
            dst[(size_t)y * w + x] = o > 255 ? 255 : o;
        }
    }
}

// Whether `d` is planar YUV (or grey) at 8 to 16 bits per sample, which
// `copy_plane` handles; `lw`/`lh` get the chroma subsampling against 4:2:0.
static int copyable(const AVPixFmtDescriptor *d, int *lw, int *lh) {
    if (d->flags & (AV_PIX_FMT_FLAG_BE | AV_PIX_FMT_FLAG_PAL | AV_PIX_FMT_FLAG_BITSTREAM |
                    AV_PIX_FMT_FLAG_HWACCEL | AV_PIX_FMT_FLAG_RGB | AV_PIX_FMT_FLAG_FLOAT |
                    AV_PIX_FMT_FLAG_BAYER | AV_PIX_FMT_FLAG_XYZ))
        return 0;
    int depth = d->comp[0].depth, step = depth > 8 ? 2 : 1;
    if (depth < 8 || depth > 16) return 0;
    // One sample per `step` bytes in its own plane (not packed with others).
    for (int i = 0; i < (d->nb_components >= 3 ? 3 : 1); i++) {
        const AVComponentDescriptor *c = &d->comp[i];
        if (c->plane != i || c->step != step || c->shift || c->offset || c->depth != depth) return 0;
    }
    if (d->nb_components == 2) return 0; // grey with alpha, packed
    // Against 4:2:0: 4:4:4 is twice as dense both ways (-1), 4:2:2 the same
    // horizontally (0) and twice as dense vertically, 4:1:1 half as dense
    // horizontally (1).
    *lw = d->log2_chroma_w - 1;
    *lh = d->log2_chroma_h - 1;
    return *lw <= 1 && *lh <= 1;
}

// Copies `f` into the planes buffer as 8-bit 4:2:0: planar YUV and grey
// directly, anything else (RGB and palette images from older codecs, packed
// or semi-planar YUV) through swscale. -1 when the format cannot be read.
static int take_planes(Video *v, AVFrame *f) {
    const AVPixFmtDescriptor *d = av_pix_fmt_desc_get(f->format);
    int w = f->width, h = f->height;
    if (!d || w <= 0 || h <= 0) return -1;
    size_t cw = (w + 1) / 2, ch = (h + 1) / 2;
    size_t need = (size_t)w * h + 2 * cw * ch;
    if (need > v->planes_cap) {
        uint8_t *p = realloc(v->planes, need);
        if (!p) return -1;
        v->planes = p;
        v->planes_cap = need;
    }
    uint8_t *y = v->planes, *u = y + (size_t)w * h, *vv = u + cw * ch;
    int yuvj = strncmp(d->name, "yuvj", 4) == 0;
    int full = f->color_range == AVCOL_RANGE_JPEG || yuvj;
    int lw, lh;
    if (copyable(d, &lw, &lh)) {
        int depth = d->comp[0].depth;
        copy_plane(y, w, h, f->data[0], f->linesize[0], w, h, 0, 0, depth);
        if (d->nb_components < 3) {
            memset(u, 128, 2 * cw * ch);
        } else {
            int sw = AV_CEIL_RSHIFT(w, d->log2_chroma_w), sh = AV_CEIL_RSHIFT(h, d->log2_chroma_h);
            copy_plane(u, cw, ch, f->data[1], f->linesize[1], sw, sh, lw, lh, depth);
            copy_plane(vv, cw, ch, f->data[2], f->linesize[2], sw, sh, lw, lh, depth);
        }
        v->colorspace = guess_colorspace(f->colorspace, h);
        v->full_range = full;
    } else {
        if (!sws_isSupportedInput(f->format)) return -1;
        v->sws = sws_getCachedContext(v->sws, w, h, f->format, w, h, AV_PIX_FMT_YUV420P,
                                      SWS_BILINEAR | SWS_ACCURATE_RND, NULL, NULL, NULL);
        if (!v->sws) return -1;
        // RGB becomes BT.601 limited range; other YUV layouts keep their own
        // matrix and range (the values are only rearranged).
        int rgb = (d->flags & (AV_PIX_FMT_FLAG_RGB | AV_PIX_FMT_FLAG_PAL)) || d->nb_components < 3;
        int matrix = rgb ? 601 : guess_colorspace(f->colorspace, h);
        const int *coefficients = sws_getCoefficients(matrix == 709 ? SWS_CS_ITU709 : SWS_CS_ITU601);
        int src_full = rgb ? 1 : full, dst_full = rgb ? 0 : full;
        sws_setColorspaceDetails(v->sws, coefficients, src_full, coefficients, dst_full, 0, 1 << 16, 1 << 16);
        uint8_t *dst[4] = {y, u, vv, NULL};
        int stride[4] = {w, (int)cw, (int)cw, 0};
        if (sws_scale(v->sws, (const uint8_t *const *)f->data, f->linesize, 0, h, dst, stride) <= 0) return -1;
        v->colorspace = matrix;
        v->full_range = dst_full;
    }
    v->width = w;
    v->height = h;
    return 0;
}

// Seeks to the keyframe at or before `ts` (stream ticks), or with `ts` < 0
// reopens the file, and drops frames until the one showing at `t` seconds.
// Reopens too when the container cannot seek. 0, or -1 when the reopen
// failed.
static int seek_ticks(Video *v, int64_t ts, double t) {
    int r = v->fmt && ts >= 0 ? av_seek_frame(v->fmt, v->stream, ts, AVSEEK_FLAG_BACKWARD) : -1;
    if (r >= 0) {
        avcodec_flush_buffers(v->dec);
        av_frame_unref(v->held);
        av_frame_unref(v->pending);
        v->has_held = v->has_pending = 0;
    } else {
        v->seek_reopened = 1;
        close_streams(v);
        if (open_streams(v) < 0) {
            close_streams(v);
            return -1;
        }
    }
    v->last_pts = -1.0;
    v->last_dts = -1.0;
    v->skip_until = t;
    v->seek_fresh = 1;
    v->seek_key = AV_NOPTS_VALUE;
    return 0;
}

// Hands `f` over in the planes buffer: its time `t`, or -2 when its format
// cannot be handed over.
static double hand_over(Video *v, AVFrame *f, double t) {
    int ok = take_planes(v, f);
    av_frame_unref(f);
    return ok < 0 ? -2.0 : t;
}

// Decodes the next frame into the planes buffer and returns its time in
// seconds from the start of the stream; -1 at the end, -2 when a frame failed
// to decode or has a format the shim cannot hand over (calling again goes on
// with the next one).
//
// The time is StepMania's (`MovieDecoder_FFMpeg::DecodePacketInBuffer`,
// 5_1-new): the decode timestamp of the packet that released the frame, so a
// stream with B-frames runs a frame or two behind its own presentation
// times, as packs were timed against. A frame with none (those drained at
// the end) follows the previous frame, or the last packet, by one period.
EXPORT double ddi_next(Video *v) {
    if (!v->fmt) return -2.0;
    if (v->has_pending) {
        v->has_pending = 0;
        return hand_over(v, v->pending, v->pending_t);
    }
    for (;;) {
        int r = receive(v);
        if (r == AVERROR_EOF) {
            // A seek that reached the end without a single frame: an AVI
            // whose index is missing (a truncated file) can send the
            // demuxer there. Decode from the start instead.
            if (v->seek_fresh && !v->seek_reopened) {
                if (seek_ticks(v, -1, v->skip_until) < 0) return -2.0;
                continue;
            }
            // A seek into the last frame: it shows for its own period,
            // and past that the movie has ended.
            if (v->skip_until >= 0 && v->has_held) {
                double target = v->skip_until;
                v->skip_until = -1.0;
                v->has_held = 0;
                if (v->held_t + v->held_length > target + 1e-6) return hand_over(v, v->held, v->held_t);
                av_frame_unref(v->held);
            }
            return -1.0;
        }
        if (r < 0) return -2.0;
        AVFrame *f = v->frame;
        double t;
        if (f->pkt_dts != AV_NOPTS_VALUE) {
            // An edit list can put the first decode times below zero; such
            // frames show from the start (and negative values are codes).
            t = (f->pkt_dts - v->start) * v->time_base;
            if (t < 0) t = 0;
        } else {
            double before = v->last_pts > v->last_dts ? v->last_pts : v->last_dts;
            t = before < 0 ? 0.0 : before + v->frame_time;
        }
        v->last_pts = t;
        if (v->seek_fresh) {
            v->seek_fresh = 0;
            // The first frame starts after the target: the seek landed past
            // it. Either the target was a B-frame coded after the keyframe
            // but referring to the GOP before (an open GOP), which the
            // decoder drops, or the container has no index and the seek
            // landed between keyframes (MPEG program streams: the decoder
            // waits for the next one). Go back from where it landed, further
            // each time, and in the end decode from the start.
            if (t > v->skip_until + 1e-6 && !v->seek_reopened) {
                av_frame_unref(f);
                int64_t landed = v->seek_key != AV_NOPTS_VALUE ? v->seek_key : v->start;
                int64_t ts = landed - (int64_t)ceil(0.5 * (1 << v->seek_retries) / v->time_base);
                if (ts < v->start) ts = v->start;
                int give_up = v->seek_retries >= 4 || landed <= v->start;
                v->seek_retries++;
                if (seek_ticks(v, give_up ? -1 : ts, v->skip_until) < 0) return -2.0;
                continue;
            }
        }
        if (v->skip_until >= 0) {
            if (t <= v->skip_until + 1e-6) {
                // Starts by the target: the frame showing at it, unless a
                // later one does too (frames may be missing in between, a
                // dropped frame holding the one before).
                av_frame_unref(v->held);
                av_frame_move_ref(v->held, f);
                v->held_t = t;
                v->held_length = v->held->duration > 0 ? v->held->duration * v->time_base : v->frame_time;
                v->has_held = 1;
                continue;
            }
            v->skip_until = -1.0;
            if (v->has_held) {
                // This frame starts after the target: the held one is
                // showing at it, and this one comes next.
                av_frame_move_ref(v->pending, f);
                v->pending_t = t;
                v->has_pending = 1;
                v->has_held = 0;
                return hand_over(v, v->held, v->held_t);
            }
        }
        return hand_over(v, f, t);
    }
}

// Moves to `t` seconds: the next `ddi_next` returns the frame showing at `t`
// (or the end), decoding from a keyframe before it. 0, or -1 when the file
// had to be reopened and could not be (the video is then unusable).
EXPORT int ddi_seek(Video *v, double t) {
    if (!(t >= 0)) t = 0;
    int64_t ts = v->start;
    if (v->fmt) {
        // Frames are stamped with the packet that releases them, up to the
        // reorder delay after the one holding them: back off by that much
        // so a keyframe between the two does not skip the frame.
        double back = (v->dec->has_b_frames + 1) * v->frame_time;
        ts += (int64_t)floor((t - back) / v->time_base);
        if (ts < v->start) ts = v->start;
    }
    v->seek_retries = 0;
    v->seek_reopened = 0;
    return seek_ticks(v, ts, t);
}

EXPORT void ddi_close(Video *v) {
    if (!v) return;
    close_streams(v);
    sws_freeContext(v->sws);
    free(v->planes);
    free(v->path);
    free(v);
}

EXPORT void *ddi_malloc(size_t n) { return malloc(n); }
EXPORT void ddi_free(void *p) { free(p); }
