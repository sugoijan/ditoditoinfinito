// See ddi_io.h.
#include "ddi_io.h"
#include <stdlib.h>
#include <libavformat/avformat.h>
#include <libavutil/error.h>
#include <libavutil/mem.h>

#define IO_BUFFER (64 * 1024)

static ddi_read_fn read_fn;
static int threads = 1;

void ddi_io_set_read(ddi_read_fn read) { read_fn = read; }
void ddi_set_threads(int n) { threads = n < 1 ? 1 : n > 16 ? 16 : n; }
int ddi_threads(void) { return threads; }

static int read_packet(void *opaque, uint8_t *buf, int len) {
    DdiIo *io = opaque;
    if (!read_fn || len <= 0) return AVERROR(EIO);
    if (io->pos >= io->size) return AVERROR_EOF;
    uint64_t left = io->size - io->pos;
    if ((uint64_t)len > left) len = (int)left;
    int n = read_fn(io->opaque, io->pos, buf, len);
    if (n < 0) return AVERROR(EIO);
    if (n == 0) return AVERROR_EOF;
    io->pos += (uint64_t)n;
    return n;
}

static int64_t seek(void *opaque, int64_t offset, int whence) {
    DdiIo *io = opaque;
    if (whence & AVSEEK_SIZE) return (int64_t)io->size;
    int64_t base;
    switch (whence & ~AVSEEK_FORCE) {
    case SEEK_SET: base = 0; break;
    case SEEK_CUR: base = (int64_t)io->pos; break;
    case SEEK_END: base = (int64_t)io->size; break;
    default: return AVERROR(EINVAL);
    }
    if (offset > INT64_MAX - base) return AVERROR(EINVAL);
    int64_t to = base + offset;
    if (to < 0) return AVERROR(EINVAL);
    io->pos = (uint64_t)to;
    return to;
}

AVIOContext *ddi_io_open(DdiIo *io) {
    io->pos = 0;
    uint8_t *buffer = av_malloc(IO_BUFFER);
    if (!buffer) return NULL;
    AVIOContext *pb = avio_alloc_context(buffer, IO_BUFFER, 0, io, read_packet, NULL, seek);
    if (!pb) av_free(buffer);
    return pb;
}

void ddi_io_close(AVIOContext **pb) {
    if (!*pb) return;
    av_freep(&(*pb)->buffer);
    avio_context_free(pb);
}

AVFormatContext *ddi_io_open_input(DdiIo *io, const char *hint, int bounded) {
    AVFormatContext *fmt = avformat_alloc_context();
    if (!fmt) return NULL;
    AVIOContext *pb = ddi_io_open(io);
    if (!pb) {
        avformat_free_context(fmt);
        return NULL;
    }
    fmt->pb = pb;
    fmt->flags |= AVFMT_FLAG_CUSTOM_IO;
    if (bounded) {
        fmt->max_streams = 32;
        fmt->probesize = 1024 * 1024;
        fmt->max_analyze_duration = 5 * AV_TIME_BASE;
        fmt->max_probe_packets = 2500;
        fmt->max_index_size = 32 * 1024 * 1024;
    }
    if (avformat_open_input(&fmt, hint, NULL, NULL) < 0) {
        // On failure avformat_open_input frees the context, not the pb.
        ddi_io_close(&pb);
        return NULL;
    }
    return fmt;
}

void ddi_io_close_input(AVFormatContext **fmt) {
    if (!*fmt) return;
    AVIOContext *pb = (*fmt)->pb;
    avformat_close_input(fmt);
    ddi_io_close(&pb);
}
