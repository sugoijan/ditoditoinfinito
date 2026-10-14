//! Original synthetic bitstreams: no acquired movies or encoded music.
use ddi_danoni_export::{Error, swf::*};
use std::io::Write;
fn packet() -> Vec<u8> {
    let mut b = vec![0; 417];
    b[..4].copy_from_slice(&[255, 251, 144, 196]);
    b
}
fn tag(code: u16, payload: &[u8]) -> Vec<u8> {
    let mut b = ((code << 6) | 63).to_le_bytes().to_vec();
    b.extend((payload.len() as u32).to_le_bytes());
    b.extend(payload);
    b
}
fn movie(tags: &[u8], compressed: bool) -> Vec<u8> {
    let mut body = vec![8, 0]; // RECT with four 1-bit zero coordinates.
    body.extend((60u16 * 256).to_le_bytes());
    body.extend(4u16.to_le_bytes());
    body.extend(tags);
    let mut b = if compressed {
        b"CWS\x08".to_vec()
    } else {
        b"FWS\x08".to_vec()
    };
    b.extend((body.len() as u32 + 8).to_le_bytes());
    if compressed {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&body).unwrap();
        b.extend(z.finish().unwrap());
    } else {
        b.extend(body)
    };
    b
}
fn head(latency: i16) -> Vec<u8> {
    let mut b = vec![14, 46];
    b.extend(735u16.to_le_bytes());
    b.extend(latency.to_le_bytes());
    tag(45, &b)
}
fn block(count: u16, seek: i16, bytes: &[u8]) -> Vec<u8> {
    let mut b = count.to_le_bytes().to_vec();
    b.extend(seek.to_le_bytes());
    b.extend(bytes);
    tag(19, &b)
}
fn regular(latency: i16, bad_seek: bool) -> Vec<u8> {
    let mut t = head(latency);
    for (count, seek, bytes) in [
        (0, 0, vec![]),
        (1152, if bad_seek { 734 } else { 735 }, packet()),
        (0, 0, vec![]),
        (1152, 1053, packet()),
    ] {
        t.extend(block(count, seek, &bytes));
        t.extend(tag(1, &[]));
    }
    t.extend(tag(0, &[]));
    t
}
#[test]
fn fws_cws_long_empty_blocks_and_seek_metadata_match() {
    for compressed in [false, true] {
        let original = movie(&regular(0, false), compressed);
        let copy = original.clone();
        let m = Movie::read(&original, &mut || Ok(())).unwrap();
        let d = m.demux("root", None, &mut || Ok(())).unwrap();
        assert_eq!(original, copy);
        assert_eq!(d.bytes, packet().repeat(2));
        assert_eq!(d.report["declaredSampleFrames"], 2304);
        assert_eq!(d.report["continuousZeroLatencyEligible"], true);
        assert_eq!(d.report["ruffleContinuousPayloadEligible"], true);
        assert_eq!(d.report["blocks"][1]["seekSamples"], 735);
        assert_eq!(d.report["blocks"][1]["seekResidualSamples"], 0.0);
    }
    for (latency, bad) in [(-1, false), (1, false), (0, true)] {
        let d = Movie::read(&movie(&regular(latency, bad), false), &mut || Ok(()))
            .unwrap()
            .demux("root", None, &mut || Ok(()))
            .unwrap();
        assert_eq!(d.report["continuousZeroLatencyEligible"], false);
        assert_eq!(d.bytes, packet().repeat(2));
    }
}
#[test]
fn runtime_slice_selects_exact_container_and_excludes_menu_audio() {
    let mut nested = 7u16.to_le_bytes().to_vec();
    nested.extend(1u16.to_le_bytes());
    nested.extend(head(0));
    nested.extend(block(1152, 0, &packet()));
    nested.extend(tag(1, &[]));
    nested.extend(tag(0, &[]));
    let mut t = head(0);
    t.extend(block(1152, 0, &packet()));
    t.extend(tag(1, &[]));
    t.extend(tag(39, &nested));
    t.extend(tag(0, &[]));
    let m = Movie::read(&movie(&t, true), &mut || Ok(())).unwrap();
    let start = ObservedStart {
        slice_start: 0,
        slice_end: t.len(),
        sample_rate: 44100,
        stereo: false,
        latency_seek: 0,
    };
    let d = m.observed_stream(&start, &mut || Ok(())).unwrap();
    assert_eq!(d.report["path"], "root");
    assert_eq!(d.bytes, packet());
    let mut start = start;
    start.slice_end -= 1;
    assert!(m.observed_stream(&start, &mut || Ok(())).is_err());
    start.slice_end = usize::MAX;
    assert!(m.observed_stream(&start, &mut || Ok(())).is_err());
    let mut t = head(0);
    t.extend(block(1152, 0, &packet()));
    t.extend(tag(1, &[]));
    let first = t.len();
    t.extend(block(1152, 0, &packet()));
    t.extend(tag(1, &[]));
    t.extend(tag(0, &[]));
    let m = Movie::read(&movie(&t, false), &mut || Ok(())).unwrap();
    let d = m
        .observed_stream(
            &ObservedStart {
                slice_start: first,
                slice_end: t.len(),
                sample_rate: 44100,
                stereo: false,
                latency_seek: 0,
            },
            &mut || Ok(()),
        )
        .unwrap();
    assert_eq!(d.bytes, packet());
    assert_eq!(d.report["selection"]["excludedBlocks"], 1);
    assert_eq!(d.report["blocks"][0]["sourceMp3Offset"], 417);
    assert_eq!(d.report["blocks"][0]["mp3Offset"], 0);
}
#[test]
fn gaps_truncation_frame_counts_and_ambiguous_heads_reject() {
    let mut t = head(0);
    t.extend(block(1152, 0, &packet()));
    for _ in 0..3 {
        t.extend(tag(1, &[]));
    }
    t.extend(block(1152, 0, &packet()));
    t.extend(tag(0, &[]));
    let m = Movie::read(&movie(&t, false), &mut || Ok(())).unwrap();
    let d = m.demux("root", None, &mut || Ok(())).unwrap();
    assert_eq!(d.report["ruffleContinuousPayloadEligible"], false);
    assert_eq!(d.report["firstRufflePayloadGapMovieFrame"], 3);
    assert!(
        m.observed_stream(
            &ObservedStart {
                slice_start: 0,
                slice_end: t.len(),
                sample_rate: 44100,
                stereo: false,
                latency_seek: 0
            },
            &mut || Ok(())
        )
        .is_err()
    );
    for (count, data) in [
        (1151, packet()),
        (1152, packet()[..416].to_vec()),
        (1152, b"ID3invalid".to_vec()),
    ] {
        let mut t = head(0);
        t.extend(block(count, 0, &data));
        t.extend(tag(0, &[]));
        assert!(
            Movie::read(&movie(&t, false), &mut || Ok(()))
                .unwrap()
                .inventory(&mut || Ok(()))
                .is_err()
        );
    }
    let mut t = head(0);
    t.extend(block(1152, 0, &packet()));
    t.extend(tag(1, &[]));
    t.extend(head(0));
    t.extend(block(1152, 0, &packet()));
    t.extend(tag(0, &[]));
    assert!(
        Movie::read(&movie(&t, false), &mut || Ok(()))
            .unwrap()
            .demux("root", None, &mut || Ok(()))
            .is_err()
    );
    for compressed in [false, true] {
        let good = movie(&regular(0, false), compressed);
        let mut tail = good.clone();
        tail.push(0);
        let mut short = good.clone();
        short.pop();
        let mut small = good.clone();
        small[4..8].copy_from_slice(&12u32.to_le_bytes());
        for bad in [tail, short, small, b"ZWSinvalid!!".to_vec()] {
            assert!(Movie::read(&bad, &mut || Ok(())).is_err());
        }
    }
    assert!(Movie::read(&movie(&tag(1, &[]), false), &mut || Ok(())).is_err());
    let cancelled = || {
        Err(Error {
            code: "cancelled",
            message: "test".into(),
        })
    };
    assert_eq!(
        Movie::read(&movie(&regular(0, false), true), &mut { cancelled })
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
    let m = Movie::read(&movie(&regular(0, false), false), &mut || Ok(())).unwrap();
    assert_eq!(
        m.observed_stream(
            &ObservedStart {
                slice_start: 0,
                slice_end: regular(0, false).len(),
                sample_rate: 44100,
                stereo: false,
                latency_seek: 0
            },
            &mut { cancelled }
        )
        .err()
        .unwrap()
        .code,
        "cancelled"
    );
}
#[derive(Default)]
struct Memory(std::collections::BTreeMap<String, Vec<u8>>);
impl ddi_danoni_export::assets::AssetSource for Memory {
    fn read(&self, _: &str, path: &str, max: usize) -> ddi_danoni_export::Result<Vec<u8>> {
        let b = &self.0[path];
        assert!(b.len() <= max);
        Ok(b.clone())
    }
}
#[test]
fn source_bound_demux_requires_successful_run_format_and_complete_audit() {
    use ddi_danoni_export::{assets::*, extraction::CaptureBindings};
    use serde_json::json;
    fn bind(memory: &Memory, path: &str) -> Binding {
        Binding {
            namespace: "test".into(),
            path: path.into(),
            sha256: sha256(&memory.0[path]),
        }
    }
    let tags = regular(0, false);
    let swf = movie(&tags, true);
    let config = json!({"movie":"movie.swf","resources":[]});
    let mut memory = Memory::default();
    memory.0.insert("movie.swf".into(), swf.clone());
    memory
        .0
        .insert("config.json".into(), serde_json::to_vec(&config).unwrap());
    let run = json!({"schema":"ddi-ruffle-run/experimental-1","exitCode":0,"termination":null,"inputsUnchanged":true,"configSha256":sha256(&memory.0["config.json"])});
    let capture = json!({"schema":"ddi-ruffle-probe/experimental-1","ruffleRevision":"89a7049965f3090e706265670df309a5128233ac","source":{"config":config,"movieSha256":sha256(&swf)},"swfHeaderFps":60,"avm1Halted":false,"audit":{"truncated":false,"droppedEntries":0,"truncatedDetails":0,"runtimeErrors":0,"entries":[]},"audio":{"streamStarts":[{"compression":"Mp3","sliceStart":0,"sliceEnd":tags.len(),"sampleRate":44100,"stereo":false,"latencySeek":0,"samplesPerMovieFrame":735}]}});
    memory
        .0
        .insert("run.json".into(), serde_json::to_vec(&run).unwrap());
    memory.0.insert(
        "snapshot.json".into(),
        serde_json::to_vec(&capture).unwrap(),
    );
    let request = DemuxRequest {
        movie: bind(&memory, "movie.swf"),
        capture: CaptureBindings {
            snapshot: bind(&memory, "snapshot.json"),
            run: bind(&memory, "run.json"),
            config: bind(&memory, "config.json"),
        },
        stream_start_index: 0,
        optional_color: None,
    };
    let p = prepare_demux(&request, &memory, &mut || Ok(())).unwrap();
    assert_eq!(p.bytes, packet().repeat(2));
    assert_eq!(p.manifest["sourceInputs"].as_object().unwrap().len(), 4);
    let mut changed = request.clone();
    changed.movie.sha256 = "0".repeat(64);
    assert_eq!(
        prepare_demux(&changed, &memory, &mut || Ok(()))
            .err()
            .unwrap()
            .code,
        "changed-asset"
    );
    let mut failed_run = run.clone();
    failed_run["exitCode"] = json!(1);
    let mut errors = capture.clone();
    errors["audit"]["runtimeErrors"] = json!(1);
    let mut wrong_format = capture.clone();
    wrong_format["audio"]["streamStarts"][0]["stereo"] = json!(true);
    let mut wrong_budget = capture.clone();
    wrong_budget["audio"]["streamStarts"][0]["samplesPerMovieFrame"] = json!(736);
    let mut wrong_fps = capture.clone();
    wrong_fps["swfHeaderFps"] = json!(30);
    for (path, value) in [
        ("run.json", failed_run),
        ("snapshot.json", errors),
        ("snapshot.json", wrong_format),
        ("snapshot.json", wrong_fps),
        ("snapshot.json", wrong_budget),
    ] {
        memory
            .0
            .insert("run.json".into(), serde_json::to_vec(&run).unwrap());
        memory.0.insert(
            "snapshot.json".into(),
            serde_json::to_vec(&capture).unwrap(),
        );
        memory
            .0
            .insert(path.into(), serde_json::to_vec(&value).unwrap());
        let mut r = request.clone();
        r.capture.run = bind(&memory, "run.json");
        r.capture.snapshot = bind(&memory, "snapshot.json");
        assert!(prepare_demux(&r, &memory, &mut || Ok(())).is_err());
    }
    memory.0.insert(
        "snapshot.json".into(),
        serde_json::to_vec(&capture).unwrap(),
    );
    assert_eq!(
        prepare_demux(&request, &memory, &mut || Err(Error {
            code: "cancelled",
            message: "test".into()
        }))
        .err()
        .unwrap()
        .code,
        "cancelled"
    );
}

#[test]
fn fingerprints_cover_nested_actions_without_executing_or_exporting_bytecode() {
    let action = [0x96, 0x01, 0x00, 0x00]; // opaque original fixture bytes
    let mut nested = 17u16.to_le_bytes().to_vec();
    nested.extend(2u16.to_le_bytes());
    nested.extend(tag(1, &[]));
    nested.extend(tag(12, &action));
    nested.extend(tag(0, &[]));
    let mut tags = tag(12, &action);
    tags.extend(tag(39, &nested));
    tags.extend(tag(26, &[1, 2, 3]));
    tags.extend(tag(0, &[]));
    for compressed in [false, true] {
        let m = Movie::read(&movie(&tags, compressed), &mut || Ok(())).unwrap();
        let rows = m.action_fingerprints(&mut || Ok(())).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].path, "root");
        assert_eq!(rows[1].path, "root/sprite17");
        assert_eq!(rows[1].frame, 2);
        assert_eq!(rows[0].sha256, rows[1].sha256);
        assert_eq!(rows[0].sha256, ddi_danoni_export::assets::sha256(&action));
        assert_eq!(rows[2].code, 26);
        assert!(
            m.action_fingerprints(&mut || Err(Error {
                code: "cancelled",
                message: "test".into()
            }))
            .is_err()
        );
    }
}
