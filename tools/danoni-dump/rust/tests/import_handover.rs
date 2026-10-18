use ddi_danoni_export::{
    Error, Result, assets::*, import_host as host, prepared_import as admission,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[path = "support/import_fixture.rs"]
mod import_fixture;
use import_fixture::*;
fn error(message: &str) -> Error {
    Error {
        code: "test-failure",
        message: message.into(),
    }
}
#[test]
fn source_identity_and_partial_selection_do_not_depend_on_staging_or_titles() {
    let (m, b) = fixture(&[0, 1], false, "Title A");
    let a = validated(&m, &b);
    assert_eq!(a.songs().len(), 1);
    let s = &a.summary().songs[0];
    assert_eq!(s.entry.artist, "Unknown artist");
    assert_eq!(s.entry.bpm, "");
    assert_eq!(s.entry.charts.len(), 2);
    assert_eq!(s.selectors, [0, 1]);
    assert!(
        s.entry
            .charts
            .iter()
            .all(|c| c.meter == 4 && c.difficulty == "Medium")
    );
    let (m, b) = fixture(&[1], false, "Changed title");
    let partial = validated(&m, &b);
    assert_eq!(partial.summary().songs[0].entry.id, s.entry.id);
    assert_ne!(
        partial.summary().songs[0].revision_sha256,
        s.revision_sha256
    );
    assert_eq!(
        host::plan(
            &partial,
            &snapshot(&a, "0", true),
            host::DuplicatePolicy::Replace
        )
        .err()
        .unwrap()
        .code,
        "library-chart-removal"
    );
    let mut id = a.summary().source.clone();
    let original = id.digest().unwrap();
    assert_eq!(
        original,
        "927e6709e6f4ef15d7864d52926f9629db4f2bfcecff536d95d37bdb7d91a59f"
    );
    id.flash_vars.insert("name".into(), "num002".into());
    assert_ne!(id.digest().unwrap(), original);
    id = a.summary().source.clone();
    id.movie_url.push_str("&other=1");
    assert_ne!(id.digest().unwrap(), original);
    id.movie_url = "https://user@example.org/a.swf".into();
    assert!(id.digest().is_err());
}
#[test]
fn changed_bytes_incomplete_parent_and_extra_inventory_do_not_admit() {
    let (mut m, b) = fixture(&[0], false, "A");
    m.files
        .get_mut(&("candidate".into(), "prepared/songs/chart-0/song.ssc".into()))
        .unwrap()
        .push(b' ');
    assert_eq!(
        admission::validate(&b, &m, &mut |_| Ok(()))
            .err()
            .unwrap()
            .code,
        "changed-asset"
    );
    let (mut m, _) = fixture(&[0], false, "A");
    let b = m.json(
        "candidate",
        "manifest.json",
        json!({"schema":"ddi-danoni-url-prepared-run/1","ready":false,"completed":null}),
    );
    assert!(admission::validate(&b, &m, &mut |_| Ok(())).is_err());
    let (m, b) = fixture(&[0], false, "A");
    let e = admission::validate(&b, &m, &mut |p| {
        if matches!(p.stage, admission::Stage::Replay) {
            Err(error("cancel"))
        } else {
            Ok(())
        }
    })
    .err()
    .unwrap();
    assert_eq!(e.code, "test-failure");
    let mut forged = m.clone();
    let mut doc: Value = serde_json::from_slice(
        &forged.files[&("candidate".into(), "prepared/manifest.json".into())],
    )
    .unwrap();
    let other = forged.put("candidate", "prepared/provenance/extra", vec![1]);
    doc["artifacts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"provenance/extra","sha256":other.sha256,"bytes":1}));
    let p = forged.json("candidate", "prepared/manifest.json", doc);
    let mut outer: Value =
        serde_json::from_slice(&m.files[&("candidate".into(), "manifest.json".into())]).unwrap();
    outer["completed"]["prepared"]["sha256"] = json!(p.sha256);
    let b = forged.json("candidate", "manifest.json", outer);
    assert!(admission::validate(&b, &forged, &mut |_| Ok(())).is_err());
}
#[derive(Default)]
struct Store {
    revision: u32,
    visible: BTreeMap<String, String>,
    events: Vec<String>,
    fail_stage: bool,
    stale_at_commit: bool,
    uncertain: bool,
    publish_before_uncertain: bool,
    fail_rollback: bool,
}
struct Tx<'a> {
    store: &'a mut Store,
    expected: String,
    pending: BTreeMap<String, String>,
    before: BTreeMap<String, String>,
}
impl host::LibraryStore for Store {
    type Transaction<'a> = Tx<'a>;
    fn check_revision(&mut self, expected: &str) -> Result<()> {
        if expected == self.revision.to_string() {
            Ok(())
        } else {
            Err(error("stale snapshot"))
        }
    }
    fn begin<'a>(&'a mut self, p: &host::PlanSummary) -> Result<Tx<'a>> {
        self.check_revision(&p.expected_catalog_revision)?;
        self.events.push("begin".into());
        let before = self.visible.clone();
        Ok(Tx {
            store: self,
            before,
            expected: p.expected_catalog_revision.clone(),
            pending: BTreeMap::new(),
        })
    }
}
impl host::Transaction for Tx<'_> {
    fn stage_evidence(&mut self, import: &admission::PreparedImport) -> Result<()> {
        assert!(!import.evidence().is_empty());
        self.store.events.push("evidence".into());
        Ok(())
    }
    fn stage_song(&mut self, s: &admission::PreparedSong, _: &host::Operation) -> Result<()> {
        assert!(!s.files().is_empty());
        assert_eq!(self.store.visible, self.before);
        self.pending.insert(
            s.summary().entry.id.clone(),
            s.summary().revision_sha256.clone(),
        );
        self.store.events.push("song".into());
        if self.store.fail_stage && self.pending.len() == 2 {
            Err(error("second song failed"))
        } else {
            Ok(())
        }
    }
    fn commit(&mut self) -> host::CommitOutcome {
        self.store.events.push("commit".into());
        if self.store.stale_at_commit {
            self.store.revision += 1;
        }
        if self.store.revision.to_string() != self.expected {
            return host::CommitOutcome::Rejected {
                error: error("stale commit"),
            };
        }
        if self.store.uncertain {
            if self.store.publish_before_uncertain {
                self.store.visible.append(&mut self.pending);
                self.store.revision += 1;
            }
            return host::CommitOutcome::NeedsRecovery {
                transaction_id: "test-journal".into(),
                reason: "acknowledgement lost".into(),
            };
        }
        self.store.visible.append(&mut self.pending);
        self.store.revision += 1;
        host::CommitOutcome::Committed {
            catalog_revision: self.store.revision.to_string(),
        }
    }
    fn rollback(&mut self) -> Result<()> {
        self.store.events.push("rollback".into());
        self.pending.clear();
        if self.store.fail_rollback {
            Err(error("cleanup failed"))
        } else {
            Ok(())
        }
    }
}
fn snapshot(import: &admission::PreparedImport, revision: &str, same: bool) -> host::Snapshot {
    host::Snapshot {
        revision: revision.into(),
        songs: import
            .summary()
            .songs
            .iter()
            .map(|s| host::InstalledSong {
                id: s.entry.id.clone(),
                source_identity_sha256: s.source_identity_sha256.clone(),
                identity_sha256: s.identity_sha256.clone(),
                selectors: s.selectors.clone(),
                revision_sha256: if same {
                    s.revision_sha256.clone()
                } else {
                    sha256(b"old revision")
                },
            })
            .collect(),
    }
}
#[test]
fn duplicate_policy_and_stale_noop_preserve_catalog_preconditions() {
    let (m, b) = fixture(&[0, 1], false, "A");
    let import = validated(&m, &b);
    let old = snapshot(&import, "0", false);
    assert_eq!(
        host::plan(&import, &old, host::DuplicatePolicy::Reject)
            .err()
            .unwrap()
            .code,
        "library-import-conflict"
    );
    let keep = host::plan(&import, &old, host::DuplicatePolicy::KeepExisting).unwrap();
    assert_eq!(
        keep.summary().operations[0].decision,
        host::Decision::KeepExisting
    );
    let replace = host::plan(&import, &old, host::DuplicatePolicy::Replace).unwrap();
    assert_eq!(
        replace.summary().operations[0].decision,
        host::Decision::Replace
    );
    let p = host::plan(
        &import,
        &snapshot(&import, "0", true),
        host::DuplicatePolicy::Reject,
    )
    .unwrap();
    let mut replacing = Store::default();
    replacing.visible.insert(
        import.summary().songs[0].entry.id.clone(),
        old.songs[0].revision_sha256.clone(),
    );
    assert!(matches!(
        host::install(&import, &replace, &mut replacing, &mut |_| Ok(())).unwrap(),
        host::Outcome::Installed { replaced: 1, .. }
    ));
    assert_eq!(
        replacing.visible[&import.summary().songs[0].entry.id],
        import.summary().songs[0].revision_sha256
    );
    let mut store = Store::default();
    assert!(matches!(
        host::install(&import, &p, &mut store, &mut |_| Ok(())).unwrap(),
        host::Outcome::Unchanged { .. }
    ));
    assert!(store.events.is_empty());
    store.revision = 1;
    assert!(host::install(&import, &p, &mut store, &mut |_| Ok(())).is_err());
    let (m, b) = fixture(&[0], false, "Different");
    let other = validated(&m, &b);
    assert!(host::install(&other, &p, &mut store, &mut |_| Ok(())).is_err());
}
#[test]
fn commit_is_atomic_and_cancellation_rolls_back_all_staged_songs() {
    let (m, b) = fixture(&[0, 1], true, "Two songs");
    let import = validated(&m, &b);
    let p = host::plan(
        &import,
        &host::Snapshot {
            revision: "0".into(),
            songs: vec![],
        },
        host::DuplicatePolicy::Reject,
    )
    .unwrap();
    let mut store = Store::default();
    let mut checkpoints = Vec::new();
    let out = host::install(&import, &p, &mut store, &mut |p| {
        checkpoints.push(p.stage);
        Ok(())
    })
    .unwrap();
    assert!(matches!(out, host::Outcome::Installed { added: 2, .. }));
    assert_eq!(store.visible.len(), 2);
    assert_eq!(checkpoints.last(), Some(&host::Stage::ReadyToCommit));
    for stop in [
        host::Stage::Begin,
        host::Stage::Evidence,
        host::Stage::Songs,
        host::Stage::ReadyToCommit,
    ] {
        let mut store = Store::default();
        assert!(
            host::install(&import, &p, &mut store, &mut |p| if p.stage == stop {
                Err(error("cancel"))
            } else {
                Ok(())
            })
            .is_err()
        );
        assert!(store.visible.is_empty());
        assert!(!store.events.contains(&"commit".into()));
    }
    let mut store = Store {
        fail_stage: true,
        ..Default::default()
    };
    assert!(host::install(&import, &p, &mut store, &mut |_| Ok(())).is_err());
    assert!(store.visible.is_empty());
    assert_eq!(store.events.last().unwrap(), "rollback");
}
#[test]
fn rejected_commit_rolls_back_but_uncertain_commit_requires_recovery() {
    let (m, b) = fixture(&[0], false, "A");
    let import = validated(&m, &b);
    let p = host::plan(
        &import,
        &host::Snapshot {
            revision: "0".into(),
            songs: vec![],
        },
        host::DuplicatePolicy::Reject,
    )
    .unwrap();
    let mut store = Store {
        stale_at_commit: true,
        ..Default::default()
    };
    assert!(host::install(&import, &p, &mut store, &mut |_| Ok(())).is_err());
    assert_eq!(store.events.last().unwrap(), "rollback");
    assert!(store.visible.is_empty());
    let mut store = Store {
        uncertain: true,
        ..Default::default()
    };
    assert!(matches!(
        host::install(&import, &p, &mut store, &mut |_| Ok(())).unwrap(),
        host::Outcome::NeedsRecovery { .. }
    ));
    assert!(!store.events.contains(&"rollback".into()));
    let mut acknowledged_late = Store {
        uncertain: true,
        publish_before_uncertain: true,
        ..Default::default()
    };
    assert!(matches!(
        host::install(&import, &p, &mut acknowledged_late, &mut |_| Ok(())).unwrap(),
        host::Outcome::NeedsRecovery { .. }
    ));
    assert_eq!(acknowledged_late.visible.len(), 1);
    assert!(!acknowledged_late.events.contains(&"rollback".into()));
    let mut store = Store {
        fail_rollback: true,
        ..Default::default()
    };
    let e = host::install(&import, &p, &mut store, &mut |p| {
        if p.stage == host::Stage::ReadyToCommit {
            Err(error("cancel"))
        } else {
            Ok(())
        }
    })
    .err()
    .unwrap();
    assert_eq!(e.code, "library-rollback-failed");
}

fn update(m: &mut Memory, path: &str, change: impl FnOnce(&mut Value)) -> Binding {
    let mut doc: Value =
        serde_json::from_slice(&m.files[&("candidate".into(), path.into())]).unwrap();
    change(&mut doc);
    m.json("candidate", path, doc)
}
#[test]
fn failed_bound_child_and_case_aliases_are_rejected_even_when_rewrapped() {
    let (mut m, _) = fixture(&[0], false, "A");
    let profile = update(&mut m, "source/profile.json", |v| v["ready"] = json!(false));
    let run = update(&mut m, "source/runtime.json", |v| {
        v["completed"]["receipt"]["sha256"] = json!(profile.sha256)
    });
    let source = update(&mut m, "source/manifest.json", |v| {
        v["completed"]["receipt"]["sha256"] = json!(run.sha256)
    });
    let ready = update(&mut m, "manifest.json", |v| {
        v["completed"]["source"]["sha256"] = json!(source.sha256)
    });
    assert_eq!(
        admission::validate(&ready, &m, &mut |_| Ok(()))
            .err()
            .unwrap()
            .code,
        "invalid-prepared-import"
    );
    let (mut m, _) = fixture(&[0], false, "A");
    let p = update(&mut m, "prepared/manifest.json", |v| {
        let mut row = v["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["path"] == "songs/chart-0/song.ssc")
            .unwrap()
            .clone();
        row["path"] = json!("songs/chart-0/Song.ssc");
        v["artifacts"].as_array_mut().unwrap().push(row);
    });
    let ready = update(&mut m, "manifest.json", |v| {
        v["completed"]["prepared"]["sha256"] = json!(p.sha256)
    });
    assert_eq!(
        admission::validate(&ready, &m, &mut |_| Ok(()))
            .err()
            .unwrap()
            .code,
        "invalid-prepared-import"
    );
}
