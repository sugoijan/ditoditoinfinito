//! The decoders against references: generated fixtures with known
//! content, the D9 rules for MP3 against Symphonia's own gapless decode,
//! and (where the `ffmpeg` command is installed) the bundled songs
//! against libopus's float decode.
//!
//! Fixtures (our own work, MIT like the rest), made with ffmpeg 9.0.2 and
//! libmp3lame from `SRC='aevalsrc=0.25*sin(2*PI*441*t):s=44100:d=0.25'`
//! (common flags `-ac 1 -fflags +bitexact -flags:a +bitexact
//! -map_metadata -1`, plus `-id3v2_version 0` for the MP3s):
//!
//! - `sine-441-info.mp3`: `-c:a libmp3lame -b:a 64k` (CBR: an `Info` tag
//!   with LAME's delay and padding);
//! - `sine-441-bare.mp3`: the same with `-write_xing 0` (no tag frame);
//! - `sine-441-xing.mp3`: `-c:a libmp3lame -q:a 4` (VBR: a `Xing` tag);
//! - `sine-441.flac`: `-c:a flac -sample_fmt s16`.

use super::*;

const OPUS: &[u8] = include_bytes!("../../tests/fixtures/sine-440.opus");
const VORBIS: &[u8] = include_bytes!("../../tests/fixtures/sine-440l-880r.ogg");
const MP3_INFO: &[u8] = include_bytes!("../../tests/fixtures/sine-441-info.mp3");
const MP3_BARE: &[u8] = include_bytes!("../../tests/fixtures/sine-441-bare.mp3");
const MP3_XING: &[u8] = include_bytes!("../../tests/fixtures/sine-441-xing.mp3");
const FLAC: &[u8] = include_bytes!("../../tests/fixtures/sine-441.flac");

/// The fixtures' signal at frame `n`.
fn sine(n: usize) -> f64 {
    0.25 * (std::f64::consts::TAU * 441.0 * n as f64 / 44_100.0).sin()
}

/// Amplitude and phase of `freq` in `x` (from frame `first` of a signal
/// at `rate`).
fn component(x: &[f32], freq: f64, rate: f64, first: usize) -> (f64, f64) {
    let (mut s, mut c) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = std::f64::consts::TAU * freq * (first + i) as f64 / rate;
        s += f64::from(v) * w.sin();
        c += f64::from(v) * w.cos();
    }
    let k = 2.0 / x.len() as f64;
    ((s * s + c * c).sqrt() * k, c.atan2(s))
}

/// Symphonia's own decode of an MP3, gapless (the LAME tag's delay and
/// padding trimmed, the tag frame dropped): what browsers give.
fn gapless_mp3(bytes: &[u8]) -> Vec<f32> {
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let reader = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    decode_symphonia(reader, Range::default(), true)
        .unwrap()
        .channels
        .remove(0)
}

/// The lag of `b` in `a` (`a[i + lag] == b[i]`) by exact match of a
/// stretch in the middle of `b`.
fn lag_of(a: &[f32], b: &[f32]) -> Option<usize> {
    let probe = &b[b.len() / 2..b.len() / 2 + 256];
    (0..a.len().saturating_sub(probe.len()))
        .find(|&i| &a[i..i + probe.len()] == probe)
        .map(|i| i - b.len() / 2)
}

#[test]
fn opus_drops_the_pre_skip_and_ends_at_the_granule() {
    let audio = decode(OPUS, Some("opus")).unwrap();
    assert_eq!(audio.sample_rate, 48_000);
    assert_eq!(audio.channels.len(), 1);
    // 0.1 s: the last granule less the pre-skip.
    assert_eq!(audio.frames(), 4_800);
    let mid = &audio.channels[0][1_200..3_600];
    let (amp, phase) = component(mid, 440.0, 48_000.0, 1_200);
    // ffmpeg's `sine` source: amplitude 1/8.
    assert!((amp - 0.125).abs() < 0.01, "{amp}");
    // The phase libopus's decode has there (`ffmpeg -c:a libopus
    // -request_sample_fmt flt`: 0.14935 rad, the codec's own shift at 16
    // kb/s): the pre-skip dropped as libopus drops it (a frame off would
    // be 0.058 rad at 440 Hz).
    assert!((phase - 0.149_35).abs() < 0.005, "phase {phase} rad");
}

#[test]
fn a_range_is_a_prefix_of_the_whole() {
    for (bytes, ext) in [
        (OPUS, "opus"),
        (VORBIS, "ogg"),
        (MP3_INFO, "mp3"),
        (FLAC, "flac"),
    ] {
        let whole = decode(bytes, Some(ext)).unwrap();
        let part = decode_range(
            bytes,
            Some(ext),
            Range {
                until: Some(0.05),
                ..Range::default()
            },
        )
        .unwrap();
        let n = (0.05 * f64::from(whole.sample_rate)).ceil() as usize;
        assert_eq!(part.frames(), n, "{ext}");
        for (p, w) in part.channels.iter().zip(&whole.channels) {
            assert_eq!(p[..], w[..n], "{ext}");
        }
    }
}

#[test]
fn vorbis_matches_the_web_fallback() {
    let all = decode(VORBIS, Some("ogg")).unwrap();
    let web = crate::decode::decode(VORBIS).unwrap();
    assert_eq!(all.channels, web.channels);
    assert_eq!(all.sample_rate, 44_100);
}

#[test]
fn flac_is_lossless() {
    let audio = decode(FLAC, Some("flac")).unwrap();
    assert_eq!(audio.sample_rate, 44_100);
    assert_eq!(audio.frames(), 11_025);
    for (n, &s) in audio.channels[0].iter().enumerate() {
        let want = (sine(n) * 32_768.0).round() / 32_768.0;
        assert!(
            (f64::from(s) - want).abs() <= 1.0 / 32_768.0,
            "{n}: {s} {want}"
        );
    }
}

#[test]
fn wav_is_read() {
    // A 16-bit mono WAV of the fixtures' signal, written here.
    let samples: Vec<i16> = (0..4_410)
        .map(|n| (sine(n) * 32_767.0).round() as i16)
        .collect();
    let data_len = samples.len() as u32 * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&44_100u32.to_le_bytes());
    wav.extend_from_slice(&88_200u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in &samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    let audio = decode(&wav, Some("wav")).unwrap();
    assert_eq!(audio.frames(), samples.len());
    for (got, want) in audio.channels[0].iter().zip(&samples) {
        assert_eq!(*got, f32::from(*want) / 32_768.0);
    }
}

#[test]
fn mp3_tags_are_found_as_stepmania_reads_them() {
    assert_eq!(mp3::silent_tag_frame(MP3_INFO), Some(1152));
    assert_eq!(mp3::silent_tag_frame(MP3_XING), None);
    assert_eq!(mp3::silent_tag_frame(MP3_BARE), None);
    // An ID3v2 tag before the frame is skipped.
    let mut tagged = b"ID3\x04\0\0\0\0\0\x05hello".to_vec();
    tagged.extend_from_slice(MP3_INFO);
    assert_eq!(mp3::silent_tag_frame(&tagged), Some(1152));
    assert_eq!(mp3::silent_tag_frame(b""), None);
    assert_eq!(mp3::silent_tag_frame(b"ID3\x04\0\0\x7f\x7f\x7f\x7f"), None);
}

#[test]
fn mp3_is_found_where_the_probe_would_not_look() {
    let whole = decode(MP3_INFO, Some("mp3")).unwrap();
    // A WAV of format 0x55 (MP3 data), and zeros before the first frame.
    let mut riff = b"RIFF\0\0\0\0WAVEfmt \x1e\0\0\0\x55\0\x01\0".to_vec();
    riff.resize(12 + 8 + 30, 0);
    riff.extend_from_slice(b"data");
    riff.extend_from_slice(&(MP3_INFO.len() as u32).to_le_bytes());
    riff.extend_from_slice(MP3_INFO);
    let mut zeros = vec![0u8; 417];
    zeros.extend_from_slice(MP3_INFO);
    for (b, ext) in [(&riff, "wav"), (&zeros, "mp3")] {
        assert_eq!(
            decode(b, Some(ext)).unwrap().channels,
            whole.channels,
            "{ext}"
        );
    }
}

#[test]
fn mp3_keeps_the_delay_and_an_info_frame_as_silence() {
    // LAME's delay (576) and the decoder's (529) are kept, as StepMania
    // 5.1 keeps them; an `Info` frame plays as 1152 frames of silence; a
    // `Xing` frame is skipped.
    for (bytes, shift) in [
        (MP3_INFO, 1152 + 576 + 529),
        (MP3_XING, 576 + 529),
        (MP3_BARE, 0),
    ] {
        let ours = decode(bytes, Some("mp3")).unwrap();
        assert_eq!(ours.sample_rate, 44_100);
        let ours = &ours.channels[0];
        let browser = gapless_mp3(bytes);
        assert_eq!(lag_of(ours, &browser), Some(shift), "shift {shift}");
        if shift > 1152 {
            assert!(ours[..1152].iter().all(|&s| s == 0.0));
        }
        // The music is there (at its delay).
        let at = shift.max(1105) + 2_000;
        let (amp, _) = component(&ours[at..at + 4_410], 441.0, 44_100.0, 0);
        assert!((amp - 0.25).abs() < 0.02, "{amp}");
    }
}

#[test]
fn resampling_keeps_pitch_length_and_time() {
    let n = 44_100;
    let audio = DecodedAudio {
        sample_rate: 44_100,
        channels: vec![(0..n).map(|i| sine(i) as f32).collect()],
        skipped_packets: 0,
    };
    let out = resample(audio, 48_000).unwrap();
    assert_eq!(out.sample_rate, 48_000);
    assert!(
        (out.frames() as i64 - 48_000).abs() <= 1,
        "{}",
        out.frames()
    );
    // Same tone, same phase: no delay added (rubato trims its own).
    let x = &out.channels[0][12_000..36_000];
    let (amp, phase) = component(x, 441.0, 48_000.0, 12_000);
    assert!((amp - 0.25).abs() < 0.001, "{amp}");
    let err = phase.rem_euclid(std::f64::consts::TAU);
    let err = err.min(std::f64::consts::TAU - err);
    assert!(err < 0.01, "phase off by {err} rad");
    // Equal rates: nothing to do.
    let same = DecodedAudio {
        sample_rate: 48_000,
        channels: vec![vec![0.5; 10]],
        skipped_packets: 0,
    };
    assert_eq!(resample(same.clone(), 48_000).unwrap(), same);
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(decode(b"", None).is_err());
    assert!(decode(b"not audio at all, not even close", Some("mp3")).is_err());
    assert!(decode(&OPUS[..60], Some("opus")).is_err());
    let mut cut = MP3_INFO.to_vec();
    cut.truncate(cut.len() / 2);
    assert!(decode(&cut, Some("mp3")).is_ok());
}

/// The bundled songs (all Ogg Opus) against libopus's float decode by the
/// `ffmpeg` command (`-request_sample_fmt flt`, else it decodes to
/// 16-bit), clamped to ±1 as `rusty-opus` clamps. Skipped without
/// `ffmpeg`. libopus 1.4 (Ubuntu 24.04, CI) and 1.6.1 agree with us
/// above 135 dB; 1.5.2 (Debian 13) decodes Sweeteners 86 dB away (its
/// own float path), so the bar is what every version meets and a frame
/// out of place would fail (under 40 dB).
#[test]
fn bundled_songs_match_libopus() {
    let songs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/songs");
    let mut checked = 0;
    for dir in std::fs::read_dir(&songs).unwrap().flatten() {
        let Some(file) = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "opus"))
        else {
            continue;
        };
        let reference = match std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-c:a",
                "libopus",
                "-request_sample_fmt",
                "flt",
                "-i",
            ])
            .arg(&file)
            .args(["-f", "f32le", "-"])
            .output()
        {
            Ok(o) if o.status.success() => o.stdout,
            _ => {
                eprintln!("no ffmpeg with libopus: bundled songs not checked");
                return;
            }
        };
        let reference: Vec<f32> = reference
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b).clamp(-1.0, 1.0))
            .collect();
        let ours = decode(&std::fs::read(&file).unwrap(), Some("opus")).unwrap();
        assert_eq!(ours.channels.len(), 2);
        let frames = reference.len() / 2;
        // ffmpeg ends the pre-skip's length early; we end at the granule.
        assert!(
            ours.frames() >= frames && ours.frames() - frames <= 312,
            "{}: {} against {frames}",
            file.display(),
            ours.frames()
        );
        let (mut err, mut sig, mut max) = (0f64, 0f64, 0f32);
        for i in 0..frames {
            for c in 0..2 {
                let d = ours.channels[c][i] - reference[i * 2 + c];
                max = max.max(d.abs());
                err += f64::from(d) * f64::from(d);
                sig += f64::from(reference[i * 2 + c]).powi(2);
            }
        }
        let snr = 10.0 * (sig / err.max(1e-30)).log10();
        assert!(
            snr > 80.0 && max < 2e-4,
            "{}: SNR {snr:.1} dB, max {max}",
            file.display()
        );
        checked += 1;
    }
    assert_eq!(checked, 4);
}

/// The MP3s against FFmpeg's decode with its trimming off (`-flags2
/// +skip_manual`, which drops the tag frame as Symphonia does): the same
/// samples from the same first frame, so the D9 shift is all that
/// differs from a browser. Skipped without `ffmpeg`.
#[test]
fn mp3_untrimmed_matches_ffmpeg() {
    for (bytes, name, silence) in [
        (MP3_INFO, "sine-441-info.mp3", 1152),
        (MP3_XING, "sine-441-xing.mp3", 0),
        (MP3_BARE, "sine-441-bare.mp3", 0),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let reference = match std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-flags2", "+skip_manual", "-i"])
            .arg(&path)
            .args(["-f", "f32le", "-"])
            .output()
        {
            Ok(o) if o.status.success() => o.stdout,
            _ => {
                eprintln!("no ffmpeg: MP3s not checked against it");
                return;
            }
        };
        let reference: Vec<f32> = reference
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        let ours = decode(bytes, Some("mp3")).unwrap().channels.remove(0);
        assert_eq!(ours.len(), reference.len() + silence, "{name}");
        let max = ours[silence..]
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(max < 1e-4, "{name}: {max}");
    }
}

/// Ogg's CRC-32 (polynomial 0x04c11db7, unreflected, zero init) over a
/// page, its checksum field read as zero.
fn ogg_crc(page: &[u8]) -> u32 {
    let mut crc = 0u32;
    for (i, &byte) in page.iter().enumerate() {
        let byte = if (22..26).contains(&i) { 0 } else { byte };
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[test]
fn opus_output_gain_is_applied() {
    // The fixture with its OpusHead's output gain set to −6 dB (Q7.8),
    // the first page's CRC made again.
    let mut b = OPUS.to_vec();
    assert_eq!(&b[..4], b"OggS");
    let segments = usize::from(b[26]);
    let body = 27 + segments;
    assert_eq!(&b[body..body + 8], b"OpusHead");
    let q8 = (-6i16 * 256).to_le_bytes();
    b[body + 16..body + 18].copy_from_slice(&q8);
    let page_len = body + b[27..body].iter().map(|&x| usize::from(x)).sum::<usize>();
    let crc = ogg_crc(&b[..page_len]);
    b[22..26].copy_from_slice(&crc.to_le_bytes());
    let plain = decode(OPUS, Some("opus")).unwrap();
    let quiet = decode(&b, Some("opus")).unwrap();
    assert_eq!(plain.frames(), quiet.frames());
    let energy = |a: &DecodedAudio| {
        a.channels[0]
            .iter()
            .map(|x| f64::from(*x).powi(2))
            .sum::<f64>()
    };
    let db = 10.0 * (energy(&quiet) / energy(&plain)).log10();
    assert!((db + 6.0).abs() < 0.01, "{db} dB");
}

#[test]
fn a_range_keeps_only_its_part() {
    for (bytes, ext) in [
        (OPUS, "opus"),
        (VORBIS, "ogg"),
        (MP3_INFO, "mp3"),
        (FLAC, "flac"),
    ] {
        let whole = decode(bytes, Some(ext)).unwrap();
        let r = whole.sample_rate;
        let part = decode_range(
            bytes,
            Some(ext),
            Range {
                from: 0.02,
                until: Some(0.07),
                ..Range::default()
            },
        )
        .unwrap();
        let (a, b) = (frames_at(0.02, r), limit(Some(0.07), r).unwrap());
        assert_eq!(part.frames(), b - a, "{ext}");
        for (p, w) in part.channels.iter().zip(&whole.channels) {
            assert_eq!(p[..], w[a..b], "{ext}");
        }
    }
}

#[test]
fn garbage_around_the_music_is_read_past() {
    let mut x = 0x9e37_79b9u32;
    let mut noise = |n: usize| -> Vec<u8> {
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    };
    // Ogg (Opus and Vorbis) with garbage after: the music, whole.
    for (bytes, ext) in [(OPUS, "opus"), (VORBIS, "ogg")] {
        let whole = decode(bytes, Some(ext)).unwrap();
        let mut b = bytes.to_vec();
        b.extend(noise(65_536));
        let got = decode(&b, Some(ext)).unwrap();
        assert_eq!(got.channels, whole.channels, "{ext}");
    }
    // MP3 after junk StepMania would sync through (4 KB), and not past
    // 32 KB.
    let whole = decode(MP3_INFO, Some("mp3")).unwrap();
    let mut b = noise(4096);
    b.retain(|&x| x != 0xff);
    b.extend_from_slice(MP3_INFO);
    assert_eq!(decode(&b, Some("mp3")).unwrap().channels, whole.channels);
    let mut far = vec![0u8; mp3::RESYNC + 10];
    far.extend_from_slice(MP3_INFO);
    assert!(decode(&far, Some("mp3")).is_err());
}

#[test]
fn a_corrupt_flac_frame_keeps_the_rest_in_time() {
    let whole = decode(FLAC, Some("flac")).unwrap();
    let mut b = FLAC.to_vec();
    let m = b.len() / 2;
    for x in &mut b[m..m + 600] {
        *x = 0x55;
    }
    let got = decode(&b, Some("flac")).unwrap();
    assert_eq!(got.frames(), whole.frames());
    // The end is the same samples at the same frames.
    let n = whole.frames();
    assert_eq!(got.channels[0][n - 2000..], whole.channels[0][n - 2000..]);
}

/// A Dancing☆Onigiri work's MP3 starts where Chrome's decode has it:
/// StepMania's decode with Chromium's lag cut from its start (the tag
/// frame StepMania plays as silence, LAME's delay and the decoder's: 2257
/// samples here), its ranges on that timeline; a simfile's is untouched.
#[test]
fn a_works_mp3_starts_where_chromes_decode_has_it() {
    use crate::mp3::{Browser, Target, first_frame};
    for bytes in [MP3_INFO, MP3_XING, MP3_BARE] {
        let whole = decode(bytes, Some("mp3")).unwrap();
        let lag = first_frame(bytes).unwrap().expected(Browser::Chromium) as usize;
        let work = decode_range(
            bytes,
            Some("mp3"),
            Range {
                target: Target::Chromium,
                ..Range::default()
            },
        )
        .unwrap();
        assert_eq!(work.frames(), whole.frames() - lag);
        assert_eq!(work.channels[0][..], whole.channels[0][lag..]);
        let part = decode_range(
            bytes,
            Some("mp3"),
            Range {
                from: 0.02,
                until: Some(0.07),
                target: Target::Chromium,
            },
        )
        .unwrap();
        let r = f64::from(whole.sample_rate);
        let (a, b) = ((0.02 * r).round() as usize, (0.07 * r).ceil() as usize);
        assert_eq!(part.channels[0][..], work.channels[0][a..b]);
    }
    assert_eq!(
        first_frame(MP3_INFO).unwrap().expected(Browser::Chromium),
        1152 + 576 + 529
    );
    assert_eq!(
        first_frame(MP3_BARE).unwrap().target_lag(Target::Chromium),
        0
    );
    assert_eq!(
        first_frame(MP3_INFO).unwrap().target_lag(Target::StepMania),
        0
    );
}
