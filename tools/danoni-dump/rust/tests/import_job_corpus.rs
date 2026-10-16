#![cfg(all(
    feature = "native",
    any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64"))
))]
//! Opt-in offline URL job: cancel after admission, restart from token, install.
use ddi_danoni_export::{
    assets::*, import_host as host, import_job::*, managed_store::Store, native::DirectorySource,
    network::NetworkCancel, prepared_import, worker_build,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf};
#[test]
fn local_url_job_can_restart_after_planning_without_another_worker_or_fetch() {
    let Some(job) = std::env::var_os("DDI_DANONI_IMPORT_URL_JOB") else {
        return;
    };
    let bundle = std::env::var_os("DDI_DANONI_WORKER_BUNDLE").expect("Installed worker required");
    let root = PathBuf::from(
        std::env::var_os("DDI_DANONI_IMPORT_JOB_ROOT")
            .expect("Fresh external review root required"),
    );
    let job = PathBuf::from(job);
    let dir = job.parent().unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&job).unwrap()).unwrap();
    assert_eq!(doc["schema"], "ddi-danoni-import-job/1");
    let request: Request = serde_json::from_value(doc["request"].clone()).unwrap();
    assert!(
        request.preparation.source.fetch.offline,
        "Never fetch a real work in this gate"
    );
    let cache = PathBuf::from(doc["cache"].as_str().unwrap());
    let cache = if cache.is_absolute() {
        cache
    } else {
        dir.join(cache)
    };
    let roots: BTreeMap<String, PathBuf> = serde_json::from_value(doc["roots"].clone()).unwrap();
    let assets = DirectorySource::new(
        roots
            .into_iter()
            .map(|(k, p)| (k, if p.is_absolute() { p } else { dir.join(p) }))
            .collect(),
    )
    .unwrap();
    let worker = worker_build::load_bundle(&PathBuf::from(bundle), &mut || Ok(())).unwrap();
    // Exclusive fresh creation: neither this gate nor a typo can adopt a library.
    fs::create_dir(&root).unwrap();
    let mut store = Store::create(&root.join("library")).unwrap();
    let output = root.join("candidate");
    let cancel = NetworkCancel::default();
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut planned = None;
    let result = execute(
        &request,
        &Services {
            cache: &cache,
            worker: &worker,
            assets: &assets,
        },
        &output,
        &mut store,
        &cancel,
        &mut |_| panic!("Offline job requested live access"),
        &mut |p| {
            let stage = match p {
                Progress::Preparing { .. } => "preparing",
                Progress::Validating { .. } => "validating",
                Progress::Planned { review } => {
                    planned = Some(review.clone());
                    cancel.cancel();
                    "planned"
                }
                Progress::Installing { .. } => "installing",
            };
            *counts.entry(stage).or_default() += 1;
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err().code, "request-cancelled");
    assert!(planned.is_some());
    assert!(!counts.contains_key("installing"));
    assert!(store.verify().unwrap().songs.is_empty());
    drop(store);
    let candidate = Candidate::read(&output.join("import-candidate.json")).unwrap();
    let source: Value =
        serde_json::from_slice(&fs::read(output.join("source/manifest.json")).unwrap()).unwrap();
    assert_eq!(source["transfers"]["requests"], 0);
    // New coordinator invocation; no worker/cache/font services are supplied.
    let mut store = Store::open(&root.join("library")).unwrap();
    let request = Resume::new(candidate);
    let installed = resume(&request, &mut store, &NetworkCancel::default(), &mut |p| {
        assert!(!matches!(p, Progress::Preparing { .. }));
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        installed.installation,
        host::Outcome::Installed { .. }
    ));
    let duplicate = resume(&request, &mut store, &NetworkCancel::default(), &mut |_| {
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        duplicate.installation,
        host::Outcome::Unchanged { .. }
    ));
    let catalog = store.verify().unwrap();
    for song in &catalog.songs {
        for file in &song.files {
            store.read_asset(song, &file.path, MEDIA_LIMIT).unwrap();
        }
    }
    if let Some(expected) = std::env::var_os("DDI_DANONI_IMPORT_EXPECTED_READY") {
        let expected =
            prepared_import::load_ready(&PathBuf::from(expected), &mut |_| Ok(())).unwrap();
        assert_eq!(expected.summary().digest, installed.review.import.digest);
    }
    let report = json!({"schema":"ddi-danoni-import-job-url-corpus/1","passed":true,"cancelledAtPlan":true,"outerReadyRetained":true,"progressCounts":counts,"transfers":source["transfers"],"installed":installed,"duplicate":duplicate,"catalog":catalog,"scope":"Offline confined native source run and fresh local managed library only"});
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}
