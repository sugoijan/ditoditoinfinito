use super::*;
use crate::{
    import_fixture::fixture,
    import_job::{Candidate, Resume},
    managed_store::Store,
};
use serde_json::json;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
struct Local(PathBuf);
impl Local {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "ddi-collection-execution-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn fixture(&self) -> Candidate {
        let (m, _) = fixture(&[0], false, "Synthetic collection song");
        let root = self.0.join("candidate");
        fs::create_dir(&root).unwrap();
        for ((ns, path), b) in m.files {
            if ns == "candidate" {
                let p = root.join(path);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, b).unwrap();
            }
        }
        Candidate::bind(&root).unwrap()
    }
    fn output(&self) -> PathBuf {
        let p = self.0.join("attempt");
        fs::create_dir(&p).unwrap();
        p
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn discovered(local: &Local) -> Discovery {
    let url = "https://autumnleaves.yukishigure.com/danoni.html";
    let row = |n, cell| {
        format!(
            "<tr><td>No.{n}</td><td>{cell}</td><td>Artist</td><td>3</td><td>X</td><td>Date</td></tr>"
        )
    };
    let html = format!(
        "<table>{}{}{}</table>",
        row(1, "<a href='a/index.html'>A</a>"),
        row(2, "<a href='b/index.html'>B</a>"),
        row(3, "Removed")
    );
    Discovery {
        root: local.0.clone(),
        request: IndexRequest::new(url),
        inventory: inspect(url, html.as_bytes(), None, 512).unwrap(),
        transfers: Counts::default(),
        manifest_sha256: "0".repeat(64),
    }
}
fn count_receipt(root: &Path, job: &import_job::Request, counts: Counts) {
    fs::create_dir_all(root.join("source")).unwrap();
    fs::write(root.join("source/manifest.json"),serde_json::to_vec(&json!({"schema":"ddi-danoni-url-native-run/1","request":job.preparation.source,"transfers":counts})).unwrap()).unwrap();
}
fn run(
    local: &Local,
    request: &Request,
    cancel: &NetworkCancel,
    mut after: impl FnMut(usize, &mut import_job::Outcome, &Path, &import_job::Request),
) -> Outcome {
    let d = discovered(local);
    let root = local.output();
    let c = local.fixture();
    let mut store = Store::create(&local.0.join("store")).unwrap();
    let mut calls = 0;
    let out = run_entries(
        request,
        &d,
        &root,
        Counts::default(),
        Instant::now(),
        cancel,
        &mut |_| Ok(()),
        &mut |job, child, callback| {
            calls += 1;
            let mut result =
                import_job::resume(&Resume::new(c.clone()), &mut store, cancel, &mut |p| {
                    callback(p)
                })?;
            count_receipt(child, job, Counts::default());
            after(calls, &mut result, child, job);
            Ok(result)
        },
    )
    .unwrap();
    store.verify().unwrap();
    out
}
fn request() -> Request {
    Request::new("https://autumnleaves.yukishigure.com/danoni.html")
}
#[test]
fn independent_children_install_and_noop_while_unavailable_rows_stay_partial() {
    let local = Local::new();
    let out = run(
        &local,
        &request(),
        &NetworkCancel::default(),
        |_, _, _, _| {},
    );
    assert!(!out.complete);
    assert!(out.stop.is_none());
    assert!(out.accounting_complete);
    assert!(
        matches!(&out.entries[0].result,EntryResult::Completed {outcome} if matches!(outcome.installation,import_host::Outcome::Installed{..}))
    );
    assert!(
        matches!(&out.entries[1].result,EntryResult::Completed {outcome} if matches!(outcome.installation,import_host::Outcome::Unchanged{..}))
    );
    assert!(matches!(
        &out.entries[2].result,
        EntryResult::Skipped { .. }
    ));
}
#[test]
fn between_work_cancellation_preserves_prior_publication() {
    let local = Local::new();
    let cancel = NetworkCancel::default();
    let out = run(&local, &request(), &cancel, |_, _, _, _| cancel.cancel());
    assert_eq!(out.stop.unwrap().code, "request-cancelled");
    assert!(matches!(
        out.entries[0].result,
        EntryResult::Completed { .. }
    ));
    assert!(matches!(out.entries[1].result, EntryResult::NotAttempted));
}
#[test]
fn cancellation_after_last_selected_commit_does_not_relabel_success() {
    let local = Local::new();
    let cancel = NetworkCancel::default();
    let mut req = request();
    req.selected = Some(vec![0]);
    let out = run(&local, &req, &cancel, |_, _, _, _| cancel.cancel());
    assert!(out.complete);
    assert!(out.stop.is_none());
}
#[test]
fn failed_parent_report_after_commit_retains_installed_result() {
    let local = Local::new();
    let out = run(
        &local,
        &request(),
        &NetworkCancel::default(),
        |_, _, child, _| {
            fs::write(
                child.parent().unwrap().join(".manifest.json.tmp"),
                b"synthetic injected filesystem conflict",
            )
            .unwrap();
        },
    );
    assert!(out.report_error.is_some());
    assert!(!out.complete);
    assert!(matches!(
        out.entries[0].result,
        EntryResult::Completed { .. }
    ));
    assert!(matches!(out.entries[1].result, EntryResult::NotAttempted));
}
#[test]
fn absent_transfer_receipt_stops_without_losing_installation() {
    let local = Local::new();
    let out = run(
        &local,
        &request(),
        &NetworkCancel::default(),
        |_, _, child, _| {
            fs::remove_file(child.join("source/manifest.json")).unwrap();
        },
    );
    assert!(!out.accounting_complete);
    assert_eq!(out.stop.unwrap().code, "collection-accounting-incomplete");
    assert!(matches!(
        out.entries[0].result,
        EntryResult::Completed { .. }
    ));
    assert!(matches!(out.entries[1].result, EntryResult::NotAttempted));
}
#[test]
fn uncertain_child_stops_following_jobs() {
    let local = Local::new();
    let out = run(
        &local,
        &request(),
        &NetworkCancel::default(),
        |_, o, _, _| {
            o.installation = import_host::Outcome::NeedsRecovery {
                transaction_id: "synthetic-uncertain".into(),
                reason: "injected uncertainty".into(),
                import_digest: "0".repeat(64),
            };
        },
    );
    assert_eq!(out.stop.unwrap().code, "library-needs-recovery");
    assert!(matches!(out.entries[1].result, EntryResult::NotAttempted));
}
#[test]
fn collection_work_bound_is_partial_and_does_not_expand_from_unselected_entries() {
    let local = Local::new();
    let mut req = request();
    req.maximum_works = 1;
    let out = run(&local, &req, &NetworkCancel::default(), |_, _, _, _| {});
    assert_eq!(out.stop.unwrap().code, "collection-work-limit");
    assert!(matches!(out.entries[1].result, EntryResult::NotAttempted));
    let d = discovered(&local);
    req.selected = Some(vec![0, 0]);
    assert!(selection(&req, &d.inventory).is_err());
    req.selected = Some(vec![99]);
    assert!(selection(&req, &d.inventory).is_err());
    req.selected = Some(vec![]);
    assert!(selection(&req, &d.inventory).is_err());
}
#[test]
fn clipping_exhausted_network_budget_allows_only_cache_and_keeps_queries() {
    let mut req = request();
    req.index.fetch.maximum_requests = 2;
    req.index.fetch.maximum_wire_bytes = 20;
    req.index.fetch.maximum_decoded_bytes = 40;
    let first = clip(
        &req,
        Counts {
            requests: 1,
            wire_bytes: 11,
            decoded_bytes: 22,
        },
        "https://example.org/a?scoreId=2",
        1111,
        2222,
    )
    .unwrap();
    let f = &first.preparation.source.fetch;
    assert_eq!(f.maximum_requests, 1);
    assert_eq!(f.maximum_wire_bytes, 9);
    assert_eq!(f.maximum_decoded_bytes, 18);
    assert_eq!(f.wall_ms, 1111);
    assert_eq!(first.preparation.output_bytes, 2222);
    assert!(first.preparation.source.url.ends_with("?scoreId=2"));
    let last = clip(
        &req,
        Counts {
            requests: 2,
            wire_bytes: 20,
            decoded_bytes: 40,
        },
        "https://example.org/b",
        123,
        111,
    )
    .unwrap();
    assert!(last.preparation.source.fetch.offline);
    req.index.fetch.offline = true;
    assert!(
        clip(&req, Counts::default(), "https://example.org/c", 1000, 1000)
            .unwrap()
            .preparation
            .source
            .fetch
            .offline
    );
}
#[test]
fn accounted_independent_failure_can_continue_to_next_candidate() {
    let local = Local::new();
    let d = discovered(&local);
    let root = local.output();
    let c = local.fixture();
    let mut store = Store::create(&local.0.join("store")).unwrap();
    let cancel = NetworkCancel::default();
    let mut calls = 0;
    let out = run_entries(
        &request(),
        &d,
        &root,
        Counts::default(),
        Instant::now(),
        &cancel,
        &mut |_| Ok(()),
        &mut |job, child, callback| {
            calls += 1;
            count_receipt(child, job, Counts::default());
            if calls == 1 {
                return Err(Error::new(
                    "unsupported-native-family",
                    "Synthetic unsupported work",
                ));
            }
            import_job::resume(&Resume::new(c.clone()), &mut store, &cancel, &mut |p| {
                callback(p)
            })
        },
    )
    .unwrap();
    assert_eq!(calls, 2);
    assert!(out.accounting_complete);
    assert!(out.stop.is_none());
    assert!(!out.complete);
    assert!(matches!(out.entries[0].result, EntryResult::Failed { .. }));
    assert!(matches!(
        out.entries[1].result,
        EntryResult::Completed { .. }
    ));
}
#[test]
fn child_transfer_receipts_debit_the_next_jobs_shared_budget() {
    let local = Local::new();
    let mut req = request();
    req.index.fetch.maximum_requests = 1;
    let out = run(
        &local,
        &req,
        &NetworkCancel::default(),
        |call, _, child, job| {
            assert_eq!(job.preparation.source.fetch.offline, call == 2);
            count_receipt(
                child,
                job,
                if call == 1 {
                    Counts {
                        requests: 1,
                        wire_bytes: 2,
                        decoded_bytes: 3,
                    }
                } else {
                    Counts::default()
                },
            );
        },
    );
    assert_eq!(out.transfers.requests, 1);
    assert_eq!(out.transfers.wire_bytes, 2);
    assert_eq!(out.transfers.decoded_bytes, 3);
    assert!(out.accounting_complete);
}
#[test]
fn navigation_required_entries_are_reported_without_executing_or_fetching() {
    let local = Local::new();
    let mut d = discovered(&local);
    d.inventory.entries[0].kind = EntryKind::NavigationRequired;
    d.inventory.entries[0].reason = Some("Requires genuine navigation".into());
    let mut req = request();
    req.selected = Some(vec![0]);
    let root = local.output();
    let out = run_entries(
        &req,
        &d,
        &root,
        Counts::default(),
        Instant::now(),
        &NetworkCancel::default(),
        &mut |_| Ok(()),
        &mut |_, _, _| panic!("must not execute browser-required destinations"),
    )
    .unwrap();
    assert!(!out.complete);
    assert!(matches!(out.entries[0].result, EntryResult::Skipped { .. }));
    assert_eq!(out.transfers.requests, 0);
}

#[test]
fn a_host_progress_error_stops_the_collection_without_a_cancel_token() {
    let local = Local::new();
    let d = discovered(&local);
    let root = local.output();
    let c = local.fixture();
    let mut store = Store::create(&local.0.join("store")).unwrap();
    let cancel = NetworkCancel::default();
    let mut calls = 0;
    let out = run_entries(
        &request(),
        &d,
        &root,
        Counts::default(),
        Instant::now(),
        &cancel,
        &mut |p| {
            if matches!(p, Progress::Work { .. }) {
                Err(Error::new("host-stop", "Synthetic host callback stop"))
            } else {
                Ok(())
            }
        },
        &mut |job, child, callback| {
            calls += 1;
            count_receipt(child, job, Counts::default());
            import_job::resume(&Resume::new(c.clone()), &mut store, &cancel, &mut |p| {
                callback(p)
            })
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(out.stop.unwrap().code, "host-stop");
    assert!(out.accounting_complete);
    assert!(!cancel.is_cancelled());
    assert!(store.catalog().unwrap().songs.is_empty());
    assert!(matches!(out.entries[1].result, EntryResult::NotAttempted));
}
