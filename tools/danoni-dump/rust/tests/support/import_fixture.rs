//! Synthetic source/preparation receipts shared by admission and store tests.
use ddi_danoni_export::{
    Error, Result, assets::*, audio::Pcm16, prepared_import as admission, work::*,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Default, Clone)]
pub struct Memory {
    pub files: BTreeMap<(String, String), Vec<u8>>,
}
impl Memory {
    pub fn put(&mut self, ns: &str, path: &str, bytes: Vec<u8>) -> Binding {
        let b = Binding {
            namespace: ns.into(),
            path: path.into(),
            sha256: sha256(&bytes),
        };
        self.files.insert((ns.into(), path.into()), bytes);
        b
    }
    pub fn json(&mut self, ns: &str, path: &str, value: Value) -> Binding {
        self.put(ns, path, serde_json::to_vec(&value).unwrap())
    }
}
impl AssetSource for Memory {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        let b = self
            .files
            .get(&(ns.into(), path.into()))
            .ok_or_else(|| Error {
                code: "missing",
                message: format!("{ns}:{path}"),
            })?;
        if b.len() > max {
            return Err(Error {
                code: "limit",
                message: path.into(),
            });
        }
        Ok(b.clone())
    }
}
pub fn fixture(selectors: &[usize], split_music: bool, title: &str) -> (Memory, Binding) {
    let mut m = Memory::default();
    let mut media = Vec::new();
    let mut audio = Vec::new();
    let count = if split_music { selectors.len() } else { 1 };
    for i in 0..count {
        let wav = Pcm16 {
            rate: 8000,
            channels: 1,
            samples: vec![(i as i16 + 1) * 1000; 8000 * 4],
        }
        .wav()
        .unwrap();
        let b = m.put("work", &format!("music-{i}.wav"), wav);
        audio.push(b);
    }
    let charts=selectors.iter().enumerate().map(|(i,s)|json!({"id":format!("chart-{s}"),"mode":"5","difficultyStars":4,
        "music":{"path":audio[if split_music{i}else{0}].path,"sha256":audio[if split_music{i}else{0}].sha256},
        "lanes":(0..5).map(|l|json!({"tapsSeconds":if l==0{vec![0.5,1.0,1.5]}else{vec![]},"holdsSeconds":if l==1{vec![[2.0,2.5]]}else{vec![]}})).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let normalized = json!({"schema":"ddi-normalized-work/experimental-1","sourceSwfSha256":sha256(b"original synthetic SWF"),"metadata":{"title":title,"artist":""},"charts":charts});
    let manifest = m.json("work", "manifest.json", normalized.clone());
    for b in &audio {
        let proof=m.json("work",&format!("{}.json",b.path),json!({"schema":"ddi-danoni-prepared-media/1","sourceManifestSha256":manifest.sha256,"sourceAudioSha256":b.sha256,"playbackSha256":b.sha256,"pcmSha256":b.sha256,"sampleRate":8000,"channels":1,"sampleFrames":32000,"codec":"wav"}));
        media.push(SuppliedMedia {
            source_audio_sha256: b.sha256.clone(),
            audio: b.clone(),
            pcm: b.clone(),
            codec: Codec::Wav,
            provenance: proof,
        });
    }
    let motions = selectors
        .iter()
        .map(|s| {
            (
                format!("chart-{s}"),
                json!({"baseArrowHeightsPerSecond":3.5}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let motion=m.json("work","motion.json",json!({"schema":"ddi-reviewed-native-scroll/experimental-1","sourceManifestSha256":manifest.sha256,"charts":motions}));
    let request = WorkRequest {
        manifest: manifest.clone(),
        reviews: ReviewBindings {
            motion: Some(motion),
            ..Default::default()
        },
        media,
        options: WorkOptions {
            display_bpms: selectors
                .iter()
                .map(|s| (format!("chart-{s}"), "*".into()))
                .collect(),
            ..Default::default()
        },
    };
    let mut work = prepare_work(&request, &m, &mut |_| Ok(())).unwrap();
    let proof=serde_json::to_vec(&json!({"schema":"ddi-danoni-fresh-preparation/1","sourceManifestSha256":manifest.sha256,"tempo":{"status":"exact-source-fallback"}})).unwrap();
    work.manifest["preparation"] = json!({"path":"provenance/preparation.json","sha256":sha256(&proof),"tempoStatus":"exact-source-fallback","musicalQuality":"provisional"});
    work.files.push(Artifact {
        path: "provenance/preparation.json".into(),
        bytes: Arc::new(proof),
    });
    work.manifest["artifacts"] = json!(
        work.files
            .iter()
            .map(|f| json!({"path":f.path,"sha256":sha256(&f.bytes),"bytes":f.bytes.len()}))
            .collect::<Vec<_>>()
    );
    for f in work.files {
        m.put(
            "candidate",
            &format!("prepared/{}", f.path),
            (*f.bytes).clone(),
        );
    }
    let prepared = m.json("candidate", "prepared/manifest.json", work.manifest);
    let normalized_binding = m.json("candidate", "source/normalized/manifest.json", normalized);
    assert_eq!(normalized_binding.sha256, manifest.sha256);
    let origins=selectors.iter().enumerate().map(|(i,s)|json!({"id":format!("chart-{s}"),"sourceMp3Sha256":sha256(if split_music&&i>0{b"second encoded audio"}else{b"first encoded audio"}),"canonicalSha256":audio[if split_music{i}else{0}].sha256,"logicalClockAgreement":true})).collect::<Vec<_>>();
    let origin=m.json("candidate","source/normalized/normalization.json",json!({"schema":"ddi-danoni-native-normalization/1","normalizedManifestSha256":manifest.sha256,"charts":origins}));
    let context=m.json("candidate","source/acquisition/context.json",json!({"sourceUrl":"https://example.org/work/song.swf?mode=5","pageUrl":"https://example.org/work/index.html","flashVars":{"name":"num001"}}));
    let normalized_refs = json!({"manifest":"normalized/manifest.json","manifestSha256":manifest.sha256,"normalization":"normalized/normalization.json","normalizationSha256":origin.sha256});
    let profile=m.json("candidate","source/profile.json",json!({"schema":"ddi-danoni-native-profile-run/1","ready":true,"failure":null,"normalized":normalized_refs}));
    let runtime=m.json("candidate","source/runtime.json",json!({"schema":"ddi-danoni-native-profile-dependency-run/1","ready":true,"failure":null,
        "completed":{"receipt":{"path":"profile.json","sha256":profile.sha256},"normalized":normalized_refs}}));
    let source=m.json("candidate","source/manifest.json",json!({"schema":"ddi-danoni-url-native-run/1","ready":true,"failure":null,"inputs":[{"namespace":"url","path":"context.json","sha256":context.sha256}],"completed":{"receipt":{"path":"runtime.json","sha256":runtime.sha256},"normalized":normalized_refs}}));
    let ready=m.json("candidate","manifest.json",json!({"schema":"ddi-danoni-url-prepared-run/1","ready":true,"failure":null,"completed":{"prepared":{"path":prepared.path,"sha256":prepared.sha256},"source":{"path":source.path,"sha256":source.sha256},"songs":count,"charts":selectors.len()}}));
    (m, ready)
}
pub fn validated(m: &Memory, b: &Binding) -> admission::PreparedImport {
    admission::validate(b, m, &mut |_| Ok(())).unwrap()
}

/// Synthetic receipted page metadata; playback bytes and identities stay fixed.
#[allow(dead_code)] // The separate desktop probe uses only the unlabeled fixture.
pub fn with_page_pack(m: &mut Memory, ready: &Binding, title: &str) -> Binding {
    let discovery = m.json(
        "candidate",
        "source/acquisition/discovery.json",
        json!({
            "schema":"ddi-danoni-page-discovery/1",
            "source":{"logicalUrl":"https://example.org/work/index.html"},
            "packTitle":title
        }),
    );
    let mut source: Value =
        serde_json::from_slice(&m.files[&("candidate".into(), "source/manifest.json".into())])
            .unwrap();
    source["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"namespace":"url","path":"discovery.json","sha256":discovery.sha256}));
    let source = m.json("candidate", "source/manifest.json", source);
    let mut outer: Value =
        serde_json::from_slice(&m.files[&(ready.namespace.clone(), ready.path.clone())]).unwrap();
    outer["completed"]["source"]["sha256"] = json!(source.sha256);
    m.json("candidate", &ready.path, outer)
}
