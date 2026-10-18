#![cfg(feature = "native")]
//! Local-only host handover. No source execution, fetch, or real library writes.
use ddi_danoni_export::{Error, Result, assets::*, import_host as host, prepared_import::*};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf};
#[derive(Default)]
struct Catalog {
    revision: u32,
    songs: BTreeMap<String, SongSummary>,
}
struct Tx<'a> {
    catalog: &'a mut Catalog,
    expected: String,
    staged: BTreeMap<String, SongSummary>,
}
impl host::LibraryStore for Catalog {
    type Transaction<'a> = Tx<'a>;
    fn check_revision(&mut self, e: &str) -> Result<()> {
        if self.revision.to_string() == e {
            Ok(())
        } else {
            Err(Error {
                code: "stale-test-catalog",
                message: e.into(),
            })
        }
    }
    fn begin<'a>(&'a mut self, p: &host::PlanSummary) -> Result<Tx<'a>> {
        self.check_revision(&p.expected_catalog_revision)?;
        Ok(Tx {
            catalog: self,
            expected: p.expected_catalog_revision.clone(),
            staged: BTreeMap::new(),
        })
    }
}
impl host::Transaction for Tx<'_> {
    fn stage_evidence(&mut self, p: &PreparedImport) -> Result<()> {
        assert!(p.evidence().iter().any(|f| f.path == "manifest.json"));
        Ok(())
    }
    fn stage_song(&mut self, s: &PreparedSong, _: &host::Operation) -> Result<()> {
        let summary = s.summary();
        for f in s.files() {
            let expected = summary.files.iter().find(|e| e.path == f.path).unwrap();
            assert_eq!(expected.sha256, sha256(&f.bytes));
            assert_eq!(expected.bytes, f.bytes.len());
        }
        let body = s
            .files()
            .iter()
            .find(|f| f.path == summary.entry.chart)
            .unwrap();
        let parsed =
            ddi_chart::formats::sm::parse_ssc(std::str::from_utf8(&body.bytes).unwrap()).unwrap();
        assert_eq!(parsed.charts.len(), summary.entry.charts.len());
        let all = s.files().iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        let assets = ddi_library::pack::resolve_assets(&parsed, "", &all);
        assert_eq!(assets.music.as_ref(), Some(&summary.entry.music));
        assert_eq!(assets.banner, summary.entry.banner);
        self.staged
            .insert(summary.entry.id.clone(), summary.clone());
        Ok(())
    }
    fn commit(&mut self) -> host::CommitOutcome {
        if self.expected != self.catalog.revision.to_string() {
            return host::CommitOutcome::Rejected {
                error: Error {
                    code: "stale-test-catalog",
                    message: self.expected.clone(),
                },
            };
        }
        self.catalog.songs.append(&mut self.staged);
        self.catalog.revision += 1;
        host::CommitOutcome::Committed {
            catalog_revision: self.catalog.revision.to_string(),
        }
    }
    fn rollback(&mut self) -> Result<()> {
        self.staged.clear();
        Ok(())
    }
}
fn snapshot(c: &Catalog) -> host::Snapshot {
    host::Snapshot {
        revision: c.revision.to_string(),
        songs: c
            .songs
            .values()
            .map(|s| host::InstalledSong {
                id: s.entry.id.clone(),
                source_identity_sha256: s.source_identity_sha256.clone(),
                identity_sha256: s.identity_sha256.clone(),
                selectors: s.selectors.clone(),
                revision_sha256: s.revision_sha256.clone(),
            })
            .collect(),
    }
}
#[test]
fn local_ready_outputs_install_into_one_host_catalog_without_collisions() {
    let Some(cases) = std::env::var_os("DDI_DANONI_IMPORT_CASES") else {
        return;
    };
    let doc: Value = serde_json::from_slice(&fs::read(cases).unwrap()).unwrap();
    let mut catalog = Catalog::default();
    let mut receipts = Vec::new();
    for case in doc["cases"].as_array().unwrap() {
        let path = PathBuf::from(case["ready"].as_str().unwrap());
        if case["expected"] == "not-ready" {
            assert!(load_ready(&path, &mut |_| Ok(())).is_err());
            receipts.push(json!({"ready":path,"rejected":true}));
            continue;
        }
        let p = load_ready(&path, &mut |_| Ok(())).unwrap();
        let planned = host::plan(&p, &snapshot(&catalog), host::DuplicatePolicy::Reject).unwrap();
        let outcome = host::install(&p, &planned, &mut catalog, &mut |_| Ok(())).unwrap();
        if case["expected"] == "unchanged" {
            assert!(matches!(outcome, host::Outcome::Unchanged { .. }));
        } else {
            assert!(matches!(outcome, host::Outcome::Installed { .. }));
        }
        receipts.push(
            json!({"ready":path,"summary":p.summary(),"plan":planned.summary(),"outcome":outcome}),
        );
    }
    let songs = catalog.songs.values().collect::<Vec<_>>();
    assert_eq!(songs.len() as u64, doc["songs"].as_u64().unwrap());
    assert_eq!(
        songs.iter().map(|s| s.entry.charts.len()).sum::<usize>() as u64,
        doc["charts"].as_u64().unwrap()
    );
    let result = json!({"schema":"ddi-danoni-import-handover-corpus/1","passed":true,"songs":songs,"runs":receipts,"libraryWrites":"in-memory test host only","scope":"Bound source/prepared replay and main library APIs; not durable desktop-store acceptance"});
    if let Some(out) = std::env::var_os("DDI_DANONI_IMPORT_REPORT") {
        fs::write(out, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
    println!("{}", result);
}
