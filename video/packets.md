# Sandboxed packet ABI, version 1

`video/packets.c` adds demux-only exports to the same replaceable FFmpeg
module as `ddivideo.c`. Existing decode exports and callers are unchanged.
`ddi_demux_abi()` returns 1; hosts load these exports only when requested,
so older modules can still decode movies. `desktop::video::packets::PacketReader`
checks the ABI and copies owned buffers out of a private, deadline-limited
wasmtime instance. Use it on a decoder worker. macOS H.264/HEVC MP4/Matroska
and VP9 WebM/MP4 playback now use it with a 256 MiB heap and two-second calls.

Run `node --no-warnings video/packets.mjs movie.mp4 [--json]` to inspect
metadata, packet CRCs and seek hints and verify key-packet seeks under WASI.
`desktop/tests/video_packets.rs` compares generated containers with ffprobe.

## Ownership and limits

All pointers are wasm32 offsets into the exported memory. A handle is opaque
and belongs to its instance. The trusted host passes only live handles, then
closes each once. At most eight demux handles can be open in an instance.
Getters return borrowed data, valid only until the next mutating call on
that handle. Copy it before another call; refresh memory views after calls.
A zero-length buffer can have a null pointer. No file tags enter metadata.

The limits per handle are: 8 MiB compressed packet, 1 MiB codec configuration,
1 MiB serialized side data, 32 side-data entries, 256 index entries per page,
and one million index entries. Open allows 32 streams, a 1 MiB probe budget,
a five-second analysis interval and 2,500 probe packets. Reading skips at most
4,096 other-stream packets per call. FFmpeg's index allocation budget is
32 MiB per stream. These are refusal limits, not truncation. A demuxer may
allocate internally before a limit can be checked; all such allocations stay
inside the existing 1 GiB wasm cap. Desktop enforces its existing per-call
deadline too. The host independently checks every copied size and range.

`ddi_demux_open(path)` opens the selected best video stream; 0 means failure.
Other streams are discarded. It does not open a decode session (stream
probing can use built-in parsers/decoders); codecs without a built-in decoder,
including AV1, can still be demuxed. `ddi_demux_close(handle)` releases it.

## Metadata and configuration

`ddi_demux_info(handle)` returns a NUL-terminated UTF-8 JSON string, at most
4,096 bytes including the terminator. This is the **initial** stream snapshot.
Its fields are defined by Rust's `StreamInfo`: codec/container names, codec
tag, stream number, dimensions, profile/level, pixel format/bit depth,
field order/reorder delay, time base, guessed frame rate, sample aspect ratio,
color metadata, start/duration/frame count, and initial configuration size.
Enum values are those of the pinned FFmpeg 9 release; unknown values must
be handled conservatively. Rational values are `[numerator, denominator]`.
The time base is positive. A zero denominator in an optional rational means
unknown. Integer timestamps and counts are decimal **strings** or `null`,
so JavaScript never rounds 64-bit values. `container_duration` is an additive
optional string/null field in AV_TIME_BASE microseconds, excluding bitrate
guesses. The native packet path requires reliable duration metadata; older
modules without that field retain software playback.

`ddi_demux_config(handle)` and `ddi_demux_config_size(handle)` expose the
current container extradata verbatim, not an OS-specific format conversion.
Configuration may be empty: H.264 AVI, for example, can keep SPS/PPS only in
its packets. This build does not synthesize extradata using ffprobe's optional
bitstream filters. The native consumer must parse the supported bitstream or
refuse it, and must check profile, level, dimensions, chroma, bit depth,
interlacing and in-band parameter changes independently of this snapshot.

## Packets

`ddi_demux_next(handle)` returns 1 for a packet, 0 for EOF, -1 for error.
EOF is sticky until seek; an error is sticky until a successful seek. EOF or
error clears the last packet buffers. `ddi_demux_error(handle)` returns a
NUL-terminated message of at most 256 bytes including the terminator.

`ddi_demux_packet_info(handle)` returns this 72-byte little-endian record:

| Offset | Type | Meaning |
| --- | --- | --- |
| 0 | u32 | Record size, 72 |
| 4 | u32 | Flags: key 1, corrupt 2, discard 4, configuration changed 8 |
| 8, 12 | u32 each | Compressed bytes, serialized side-data bytes |
| 16, 24 | i64 each | PTS, DTS; `INT64_MIN` means unknown |
| 32, 40 | i64 each | Duration (0 unknown), byte position (-1 unknown) |
| 48 | u64 | Sequence, starting at 1, reset by seek |
| 56, 60 | u32 each | Seek epoch, configuration generation |
| 64, 68 | u32 each | Side-data entry count, reserved (0) |

`ddi_demux_data(handle)` returns the compressed bytes, without conversion.
`ddi_demux_side_data(handle)` returns contiguous TLVs: a u32 FFmpeg 9
`AVPacketSideDataType`, a u32 byte length, then that many bytes, with no padding.
All side data is preserved, including unrecognized types. NEW_EXTRADATA and
PARAM_CHANGE set the configuration-changed flag and increment its generation
once per packet; NEW_EXTRADATA also replaces the configuration getter's bytes.
This does not detect changes carried only in-band: the OS consumer must do so.

PTS/DTS are raw signed stream ticks, including negative timestamps. Keep
both: feeding a decoder and assigning its output the game's StepMania DTS
timeline are separate operations. The module's existing frame timing uses
`(frame.pkt_dts - start) * time_base`, with its existing missing-DTS/EOF rules;
packet PTS alone does not reproduce that timeline or output order.

## Seeks and index hints

`ddi_demux_seek(handle, i64 timestamp)` seeks backward to a key packet in
**raw stream ticks**, returning 0 on success, -1 on failure. Success clears
EOF/errors, increments the epoch, resets packet sequence/configuration
generation and restores the header configuration. It does **not** reconstruct
midstream parameter changes at the target: consumers of changing streams
must reopen/preroll or fall back. This is not an exact displayed-frame seek.
The caller still needs codec-specific output ordering, drain and exact-seek
logic, as the checked AVI path already does.

`ddi_demux_index_count(handle)` returns the currently available count or -1
if over the limit. `ddi_demux_index(handle, u32 first, u32 count)` returns
the copied count, or -1 for an invalid range/count. Count is at most 256;
`first == total` gives an empty page. `ddi_demux_index_data(handle)` returns
32-byte little-endian records: i64 timestamp and byte position, i32 size,
u32 flags (key 1, discard 2), i32 minimum distance, u32 reserved (0).

These are FFmpeg demuxer index hints, which can be empty, incomplete or grow
while reading. Their timestamps follow that demuxer's index convention;
they are not necessarily DTS and are not a presentation-order frame index.
The interface deliberately makes no promise of a whole-file scan.

The pinned upstream contracts are FFmpeg's `AVPacket`, packet side data and
`AVIndexEntry`; see [FFmpeg's API documentation](https://www.ffmpeg.org/doxygen/9.0/avformat_8h.html).
