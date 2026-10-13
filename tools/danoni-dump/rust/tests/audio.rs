use ddi_danoni_export::{Error, audio::*};
fn signal(rate: u32, channels: usize, seconds: usize) -> Pcm16 {
    let mut seed = 123456u32;
    let mut samples = Vec::new();
    for i in 0..rate as usize * seconds {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let v = ((seed >> 16) as i16) / 3;
        let v = if i < rate as usize / 3 { 0 } else { v };
        samples.push(v);
        if channels == 2 {
            samples.push(-v);
        }
    }
    Pcm16 {
        rate,
        channels,
        samples,
    }
}
#[test]
fn integer_wav_decodes_without_sample_or_channel_changes() {
    let mut p = signal(44100, 2, 2);
    p.samples.splice(0..6, [i16::MIN, i16::MAX, -1, 0, 1, 0]);
    let bytes = p.wav().unwrap();
    let copy = bytes.clone();
    let d = decode(&bytes, &DecodeOptions::default(), &mut || Ok(())).unwrap();
    assert_eq!(d.pcm, p);
    assert_eq!(copy, bytes);
    assert_eq!(d.report["skippedPackets"], 0);
    assert!(
        decode(
            &bytes,
            &DecodeOptions {
                track_id: Some(999)
            },
            &mut || Ok(())
        )
        .is_err()
    );
}
#[test]
fn malformed_truncated_and_cancelled_decodes_fail() {
    let b = signal(8000, 1, 1).wav().unwrap();
    for data in [
        Vec::new(),
        b[..b.len() - 1].to_vec(),
        b[..20].to_vec(),
        b"not audio".to_vec(),
    ] {
        assert!(decode(&data, &DecodeOptions::default(), &mut || Ok(())).is_err());
    }
    let mut count = 0;
    let e = decode(&b, &DecodeOptions::default(), &mut || {
        count += 1;
        if count == 2 {
            Err(Error {
                code: "cancelled",
                message: "test".into(),
            })
        } else {
            Ok(())
        }
    })
    .err()
    .unwrap();
    assert_eq!(e.code, "cancelled");
    let p = Pcm16 {
        rate: 44100,
        channels: 0,
        samples: vec![1],
    };
    assert!(p.validate().is_err());
}
#[test]
fn resampling_removes_filter_delay_and_preserves_duration() {
    for (a, b) in [(22050, 44100), (44100, 48000), (5512, 44100)] {
        let mut p = Pcm16 {
            rate: a,
            channels: 1,
            samples: vec![0; a as usize],
        };
        p.samples[a as usize / 2] = 20000;
        let (r, proof) = resample(&p, b, true, &mut || Ok(())).unwrap();
        assert_eq!(
            r.frames(),
            (p.frames() as u64 * b as u64).div_ceil(a as u64) as usize
        );
        assert_eq!(r.channels, 2);
        assert!(r.samples.as_chunks::<2>().0.iter().all(|v| v[0] == v[1]));
        let peak = r
            .samples
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| v[0].unsigned_abs())
            .unwrap()
            .0;
        let wanted = (a as usize / 2) as f64 * b as f64 / a as f64;
        assert!((peak as f64 - wanted).abs() <= 1.0);
        assert!(proof["removedOutputDelayFrames"].as_u64().unwrap() > 0);
    }
    let p = signal(8000, 2, 1);
    assert_eq!(resample(&p, 8000, true, &mut || Ok(())).unwrap().0, p);
}
#[test]
fn origin_correlation_measures_signed_lead_in_and_ignores_beat_assumptions() {
    let p = signal(8000, 2, 4);
    for shift in [37i64, -23, 0] {
        let d = p.shifted(-shift).unwrap();
        let mut native = vec![0; 800 * 2];
        native.extend(&p.samples);
        let native = Pcm16 {
            rate: 8000,
            channels: 2,
            samples: native,
        };
        let m = origin_match(&native, &d, 800, None, &mut || Ok(())).unwrap();
        assert_eq!(m.lag_samples, shift);
        assert!(m.windows.len() >= 2);
        assert_eq!(m.maximum_spread_samples, 0);
        assert!(m.windows.iter().all(|w| w.correlation > 0.999));
        if shift >= 0 {
            assert_eq!(d.shifted(m.lag_samples).unwrap(), p);
        }
    }
}
#[test]
fn absent_silent_inconsistent_and_cancelled_origin_evidence_reject() {
    let p = signal(8000, 2, 3);
    let mut silence = p.clone();
    silence.samples.fill(0);
    assert!(origin_match(&silence, &p, 0, None, &mut || Ok(())).is_err());
    assert!(origin_match(&p, &p, p.frames(), None, &mut || Ok(())).is_err());
    let mut mixed = p.clone();
    mixed.samples[..16000].copy_from_slice(&p.shifted(-40).unwrap().samples[..16000]);
    assert!(origin_match(&p, &mixed, 0, None, &mut || Ok(())).is_err());
    assert_eq!(
        origin_match(&p, &p, 0, None, &mut || Err(Error {
            code: "cancelled",
            message: "test".into()
        }))
        .unwrap_err()
        .code,
        "cancelled"
    );
    assert!(p.shifted(8000).is_err());
}

#[cfg(feature = "vorbis")]
#[test]
fn ogg_round_trip_retains_frames_phase_and_is_deterministic() {
    let rate = 44100;
    let samples = (0..rate * 3)
        .flat_map(|i| {
            let t = i as f64 / rate as f64;
            let left = ((t * 439.3 * std::f64::consts::TAU).sin() * 9000.0
                + (t * t * 79.0).sin() * 4000.0) as i16;
            let right = ((t * 871.7 * std::f64::consts::TAU).sin() * 7000.0) as i16;
            [left, right]
        })
        .collect();
    let p = Pcm16 {
        rate,
        channels: 2,
        samples,
    };
    let a = encode_ogg(&p, 0.5, &mut || Ok(())).unwrap();
    let b = encode_ogg(&p, 0.5, &mut || Ok(())).unwrap();
    assert_eq!(a, b);
    let d = decode(&a, &DecodeOptions::default(), &mut || Ok(())).unwrap();
    assert_eq!(d.pcm.frames(), p.frames());
    assert_eq!(d.pcm.channels, 2);
    let m = origin_match(&p, &d.pcm, 0, None, &mut || Ok(())).unwrap();
    assert_eq!(m.lag_samples, 0);
    assert!(encode_ogg(&p, f32::NAN, &mut || Ok(())).is_err());
    assert_eq!(
        encode_ogg(&p, 0.5, &mut || Err(Error {
            code: "cancelled",
            message: "test".into()
        }))
        .unwrap_err()
        .code,
        "cancelled"
    );
    let mut truncated = a.clone();
    truncated.truncate(a.len() - 500);
    assert!(decode(&truncated, &DecodeOptions::default(), &mut || Ok(())).is_err());
}
#[derive(Default)]
struct Memory(std::collections::BTreeMap<String, Vec<u8>>);
impl ddi_danoni_export::assets::AssetSource for Memory {
    fn read(&self, _: &str, p: &str, m: usize) -> ddi_danoni_export::Result<Vec<u8>> {
        let b = &self.0[p];
        assert!(b.len() <= m);
        Ok(b.clone())
    }
}
#[test]
fn codec_receipt_binds_the_source_and_feeds_song_preparation() {
    use ddi_danoni_export::{assets::*, work::*};
    use serde_json::json;
    let p = signal(8000, 1, 3);
    let wav = p.wav().unwrap();
    let hash = sha256(&wav);
    let manifest=serde_json::to_vec(&json!({"schema":"ddi-normalized-work/experimental-1","metadata":{"title":"Synthetic","artist":"Test"},"music":{"path":"music.wav","sha256":hash},"charts":[{"id":"test","mode":"5","lanes":(0..5).map(|i|json!({"tapsSeconds":if i==0{vec![1.0,2.0]}else{vec![]},"holdsSeconds":[]})).collect::<Vec<_>>()}]})).unwrap();
    let binding = |path: &str, b: &[u8]| Binding {
        namespace: "input".into(),
        path: path.into(),
        sha256: sha256(b),
    };
    let manifest_binding = binding("source.json", &manifest);
    let audio = binding("music.wav", &wav);
    let mut source = Memory(std::collections::BTreeMap::from([
        ("source.json".into(), manifest),
        ("music.wav".into(), wav),
    ]));
    let r = MediaRequest {
        manifest: manifest_binding.clone(),
        audio: audio.clone(),
        codec: Codec::Wav,
        decode: DecodeOptions::default(),
        quality: 0.5,
    };
    let result = prepare_media(&r, &source, &mut || Ok(())).unwrap();
    assert_eq!(result.manifest["sampleFrames"], 24000);
    for f in &result.files {
        source.0.insert(f.path.clone(), f.bytes.as_ref().clone());
    }
    let receipt = serde_json::to_vec(&result.manifest).unwrap();
    let proof = binding("receipt.json", &receipt);
    source.0.insert("receipt.json".into(), receipt);
    let pcm_bytes = &source.0["pcm.wav"];
    let pcm = binding("pcm.wav", pcm_bytes);
    let music = binding("music.wav", &source.0["music.wav"]);
    let request = WorkRequest {
        manifest: manifest_binding,
        reviews: ReviewBindings::default(),
        media: vec![SuppliedMedia {
            source_audio_sha256: audio.sha256,
            audio: music,
            pcm,
            codec: Codec::Wav,
            provenance: proof,
        }],
        options: WorkOptions::default(),
    };
    let prepared = prepare_work(&request, &source, &mut |_| Ok(())).unwrap();
    assert_eq!(prepared.manifest["songs"][0]["title"], "Synthetic");
    #[cfg(feature = "native")]
    {
        let dir = std::env::temp_dir().join(format!(
            "ddi-media-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        ddi_danoni_export::native::write_media(&result, &dir, &mut || Ok(())).unwrap();
        assert!(dir.join("manifest.json").is_file());
        assert!(ddi_danoni_export::native::write_media(&result, &dir, &mut || Ok(())).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn mp3_whole_frame_inventory_detects_truncation_and_metadata_frames() {
    let frame = || {
        let mut b = vec![0u8; 417];
        b[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0]);
        b
    };
    let mut raw = Vec::new();
    for _ in 0..5 {
        raw.extend(frame());
    }
    let decoded = decode(&raw, &DecodeOptions::default(), &mut || Ok(())).unwrap();
    assert_eq!(decoded.pcm.frames(), 5 * 1152);
    assert_eq!(decoded.report["mp3Inventory"]["mpegFrames"], 5);
    assert_eq!(decoded.report["gapless"], false);
    raw.pop();
    assert!(decode(&raw, &DecodeOptions::default(), &mut || Ok(())).is_err());
    let mut tag = frame();
    tag[1] = 0xfa;
    tag[36..40].copy_from_slice(b"Info");
    let mut tagged = tag;
    for _ in 0..5 {
        tagged.extend(frame());
    }
    let d = decode(&tagged, &DecodeOptions::default(), &mut || Ok(())).unwrap();
    assert_eq!(d.pcm.frames(), 5 * 1152);
    assert_eq!(d.report["mp3Inventory"]["metadataFrame"], true);
    let mut id3 = b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec();
    id3.extend(&tagged);
    assert_eq!(
        decode(&id3, &DecodeOptions::default(), &mut || Ok(()))
            .unwrap()
            .pcm,
        d.pcm
    );
}
