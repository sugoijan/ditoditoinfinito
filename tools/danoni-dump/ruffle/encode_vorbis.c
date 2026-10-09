/* SPDX-License-Identifier: MIT
 * Copyright (c) 2026 Dito Dito Infinito contributors
 * Encode bounded mono/stereo PCM16LE from stdin with the installed libvorbis.
 * Public API: https://xiph.org/vorbis/doc/vorbisenc/reference.html
 */
#include <ogg/ogg.h>
#include <vorbis/vorbisenc.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>

static int emit(ogg_page *page) {
    return fwrite(page->header, 1, (size_t)page->header_len, stdout) != (size_t)page->header_len ||
           fwrite(page->body, 1, (size_t)page->body_len, stdout) != (size_t)page->body_len;
}
int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "--version") == 0) {
        puts(vorbis_version_string()); return 0;
    }
    if (argc != 2 && argc != 3) return 2;
    char *end; long rate = strtol(argv[1], &end, 10);
    if (*end || rate < 8000 || rate > 96000) return 2;
    long channels = argc == 3 ? strtol(argv[2], &end, 10) : 1;
    if ((argc == 3 && *end) || channels < 1 || channels > 2) return 2;
    vorbis_info info; vorbis_comment comment; vorbis_dsp_state dsp;
    vorbis_block block; ogg_stream_state stream; ogg_packet packet; ogg_page page;
    vorbis_info_init(&info);
    if (vorbis_encode_init_vbr(&info, channels, rate, .5f)) return 3;
    vorbis_comment_init(&comment);
    vorbis_comment_add_tag(&comment, "ENCODER", "DDI local libvorbis repack");
    if (vorbis_analysis_init(&dsp, &info) || vorbis_block_init(&dsp, &block) ||
        ogg_stream_init(&stream, 0x444449)) return 3;
    ogg_packet identification, metadata, setup;
    if (vorbis_analysis_headerout(&dsp, &comment, &identification, &metadata, &setup)) return 3;
    ogg_stream_packetin(&stream, &identification);
    ogg_stream_packetin(&stream, &metadata);
    ogg_stream_packetin(&stream, &setup);
    while (ogg_stream_flush(&stream, &page)) if (emit(&page)) return 4;
    unsigned char bytes[4096]; size_t total = 0;
    for (;;) {
        size_t count = fread(bytes, 1, sizeof(bytes), stdin);
        if (count % (2 * (size_t)channels) || ferror(stdin) || (total += count) > 128u*1024u*1024u) return 5;
        size_t samples = count / (2 * (size_t)channels);
        if (samples) {
            float **buffer = vorbis_analysis_buffer(&dsp, (int)samples);
            if (!buffer) return 3;
            for (size_t i = 0; i < samples; ++i) {
                for (long channel = 0; channel < channels; ++channel) {
                    size_t offset = (i * (size_t)channels + (size_t)channel) * 2;
                    unsigned value = bytes[offset] | ((unsigned)bytes[offset+1] << 8);
                    int signed_value = value < 32768 ? (int)value : (int)value - 65536;
                    buffer[channel][i] = signed_value / 32768.0f;
                }
            }
        }
        if (vorbis_analysis_wrote(&dsp, (int)samples)) return 3;
        while (vorbis_analysis_blockout(&dsp, &block) == 1) {
            if (vorbis_analysis(&block, NULL) || vorbis_bitrate_addblock(&block)) return 3;
            while (vorbis_bitrate_flushpacket(&dsp, &packet)) {
                ogg_stream_packetin(&stream, &packet);
                while (ogg_stream_pageout(&stream, &page)) if (emit(&page)) return 4;
            }
        }
        if (!samples) break;
    }
    while (ogg_stream_flush(&stream, &page)) if (emit(&page)) return 4;
    ogg_stream_clear(&stream); vorbis_block_clear(&block); vorbis_dsp_clear(&dsp);
    vorbis_comment_clear(&comment); vorbis_info_clear(&info);
    return fflush(stdout) || ferror(stdout) ? 4 : 0;
}
