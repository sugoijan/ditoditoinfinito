#![cfg(feature = "opus")]
//! Decoder interoperability, sample-accurate container trimming and cancellation.
use ddi_danoni_export::{Error, audio::*};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn tone(rate: u32, channels: usize, frames: usize) -> Pcm16 {
    let mut seed = 123456u32;
    let mut samples = Vec::new();
    for n in 0..frames {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let t = n as f64 / rate as f64;
        let v = if n < frames.min(rate as usize / 8) / 2 {
            0
        } else {
            ((t * (440.0 + t * 91.0) * std::f64::consts::TAU).sin() * 10000.0) as i16
                + (seed >> 20) as i16
                - 2048
        };
        samples.push(v);
        if channels == 2 {
            samples.push(-v / 2);
        }
    }
    Pcm16 {
        rate,
        channels,
        samples,
    }
}

#[test]
fn deterministic_opus_keeps_exact_frames_and_measured_origin() {
    for channels in [1, 2] {
        let pcm = tone(48000, channels, 48000 * 3 + 17);
        let (a, proof) = encode_opus(&pcm, 0.5, &mut || Ok(())).unwrap();
        assert_eq!(a, encode_opus(&pcm, 0.5, &mut || Ok(())).unwrap().0);
        assert_eq!(proof["preSkipFrames"], 120);
        let decoded = decode(&a, &DecodeOptions::default(), &mut || Ok(())).unwrap();
        assert_eq!(decoded.pcm.frames(), pcm.frames());
        assert_eq!(decoded.pcm.channels, channels);
        let matched = origin_match(&pcm, &decoded.pcm, 0, None, &mut || Ok(())).unwrap();
        assert!(matched.lag_samples.abs() <= 1, "{matched:?}");
        for bad in [
            a[..a.len() - 1].to_vec(),
            [a.as_slice(), b"junk"].concat(),
            {
                let mut b = a.clone();
                let i = b.len() - 10;
                b[i] ^= 1;
                b
            },
        ] {
            assert!(decode(&bad, &DecodeOptions::default(), &mut || Ok(())).is_err());
        }
    }
}

#[test]
fn invalid_inputs_and_mid_stream_cancellation_fail() {
    let pcm = tone(48000, 1, 9600);
    assert!(encode_opus(&pcm, f32::NAN, &mut || Ok(())).is_err());
    assert!(encode_opus(&tone(44100, 1, 1000), 0.5, &mut || Ok(())).is_err());
    let mut calls = 0;
    let error = encode_opus(&pcm, 0.5, &mut || {
        calls += 1;
        if calls == 4 {
            Err(Error {
                code: "cancelled",
                message: "test".into(),
            })
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error.code, "cancelled");
    let encoded = encode_opus(&pcm, 0.5, &mut || Ok(())).unwrap().0;
    let mut calls = 0;
    assert_eq!(
        decode(&encoded, &DecodeOptions::default(), &mut || {
            calls += 1;
            if calls == 6 {
                Err(Error {
                    code: "cancelled",
                    message: "test".into(),
                })
            } else {
                Ok(())
            }
        })
        .err()
        .unwrap()
        .code,
        "cancelled"
    );
}

#[test]
fn independent_ffmpeg_decode_checks_resampling_skip_and_end_trim() {
    let Some(ffmpeg) = std::env::var_os("DDI_DANONI_FFMPEG") else {
        eprintln!("SKIP: set DDI_DANONI_FFMPEG for the independent codec gate");
        return;
    };
    // Single samples, sub-packets, exact packet boundaries and odd endings;
    // 44.1 kHz and legacy rates exercise delay-compensated resampling as well.
    for (rate, frames) in [
        (48000, 1),
        (44100, 1),
        (8000, 1),
        (5512, 7),
        (48000, 119),
        (48000, 840),
        (48000, 960),
        (48000, 961),
        (48000, 1920),
        (44100, 44100 * 3 + 17),
        (22050, 22050 * 3 + 7),
        (8000, 8000 * 3 + 1),
        (5512, 5512 * 3 + 1),
        (96000, 96000 * 3 + 1),
    ] {
        for channels in [1, 2] {
            let original = tone(rate, channels, frames);
            let (pcm, _) = resample(&original, 48000, false, &mut || Ok(())).unwrap();
            assert_eq!(
                pcm.frames(),
                (frames as u64 * 48000).div_ceil(rate as u64) as usize
            );
            let encoded = encode_opus(&pcm, 0.5, &mut || Ok(())).unwrap().0;
            let own = decode(&encoded, &DecodeOptions::default(), &mut || Ok(()))
                .unwrap()
                .pcm;
            let mut child = Command::new(&ffmpeg)
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-threads",
                    "1",
                    "-i",
                    "pipe:0",
                    "-c:a",
                    "pcm_s16le",
                    "-f",
                    "s16le",
                    "pipe:1",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut stdin = child.stdin.take().unwrap();
            let out = std::thread::scope(|scope| {
                let writer = scope.spawn(move || stdin.write_all(&encoded));
                let out = child.wait_with_output().unwrap();
                writer.join().unwrap().unwrap();
                out
            });
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                out.stdout.len(),
                pcm.samples.len() * 2,
                "{rate}/{channels}/{frames}"
            );
            let independent = Pcm16 {
                rate: 48000,
                channels,
                samples: out
                    .stdout
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                    .collect(),
            };
            assert_eq!(own.frames(), independent.frames());
            let max_error = own
                .samples
                .iter()
                .zip(&independent.samples)
                .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs())
                .max()
                .unwrap();
            assert!(
                max_error <= 2,
                "Rust/libopus disagreement: {max_error} at {rate}/{channels}/{frames}"
            );
            if pcm.frames() > 48000 {
                let matched = origin_match(&pcm, &independent, 0, None, &mut || Ok(())).unwrap();
                assert!(matched.lag_samples.abs() <= 1, "{matched:?}");
            }
            eprintln!(
                "opus-independent: source={rate} Hz, channels={channels}, input={frames}, output={} frames, max PCM difference={max_error}",
                pcm.frames()
            );
        }
    }
}
