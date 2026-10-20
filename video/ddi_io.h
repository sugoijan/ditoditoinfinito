// Movie bytes for the native helper (`video/helper`): FFmpeg reads through
// an AVIOContext whose reads the helper answers by asking the game for the
// bytes (`ddi_io_read`), so the helper never opens a file. Built only with
// DDI_NATIVE; the wasm module reads files through WASI instead.
#ifndef DDI_IO_H
#define DDI_IO_H
#include <stdint.h>
#include <libavformat/avformat.h>

// Bytes `[offset, offset + len)` of the movie `opaque` names into `buf`:
// how many were read (fewer at the end, 0 at or past it), or < 0 on failure.
typedef int (*ddi_read_fn)(void *opaque, uint64_t offset, uint8_t *buf, int len);

typedef struct DdiIo {
    void *opaque;
    uint64_t size; // the movie's length in bytes
    uint64_t pos;
} DdiIo;

// Installed once by the helper before any open.
void ddi_io_set_read(ddi_read_fn read);
// Decoder threads for every decoder opened from now on (1: as StepMania).
void ddi_set_threads(int threads);
int ddi_threads(void);

// A reading, seekable AVIOContext over `io` (its position reset to 0), or
// NULL when out of memory. Freed with `ddi_io_close`.
AVIOContext *ddi_io_open(DdiIo *io);
void ddi_io_close(AVIOContext **pb);

// A format context over `io` opened and probed, `hint` (a file name, for
// the extension) helping the probe as the module's `/v/<id>.<ext>` did;
// `bounded` gives it the demux ABI's probe limits (`packets.md`), otherwise
// FFmpeg's defaults, as the decoder has them. NULL on failure (nothing
// left allocated).
AVFormatContext *ddi_io_open_input(DdiIo *io, const char *hint, int bounded);
// Closes a context from `ddi_io_open_input` and its AVIOContext.
void ddi_io_close_input(AVFormatContext **fmt);

#endif
