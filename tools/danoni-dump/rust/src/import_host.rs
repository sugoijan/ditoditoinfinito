//! Host-owned atomic library transactions over opaque validated imports.
use crate::{
    Error, Result,
    assets::{sha256, valid_hash},
    prepared_import::{PreparedImport, PreparedSong},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-library-handover", message)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledSong {
    pub id: String,
    pub source_identity_sha256: String,
    pub identity_sha256: String,
    pub revision_sha256: String,
    pub selectors: Vec<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub revision: String,
    pub songs: Vec<InstalledSong>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DuplicatePolicy {
    #[default]
    Reject,
    KeepExisting,
    Replace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decision {
    Add,
    Unchanged,
    KeepExisting,
    Replace,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub id: String,
    pub decision: Decision,
    pub previous_revision_sha256: Option<String>,
    pub revision_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub schema: &'static str,
    pub import_digest: String,
    pub expected_catalog_revision: String,
    pub duplicate_policy: DuplicatePolicy,
    pub operations: Vec<Operation>,
}
/// A serialized plan is informational; only `plan` constructs an installable one.
pub struct Plan {
    summary: PlanSummary,
}
impl Plan {
    pub fn summary(&self) -> &PlanSummary {
        &self.summary
    }
}
pub fn plan(import: &PreparedImport, snapshot: &Snapshot, policy: DuplicatePolicy) -> Result<Plan> {
    if snapshot.revision.is_empty()
        || snapshot.revision.len() > 256
        || snapshot.revision.chars().any(char::is_control)
        || snapshot.songs.len() > 100000
    {
        return Err(invalid("Need bounded catalog revision and entries"));
    }
    let mut existing = BTreeMap::new();
    for s in &snapshot.songs {
        if !valid_hash(&s.identity_sha256)
            || !valid_hash(&s.source_identity_sha256)
            || !valid_hash(&s.revision_sha256)
            || s.selectors.is_empty()
            || s.selectors.len() > 16
            || s.selectors.iter().any(|n| *n >= 16)
            || s.selectors.windows(2).any(|w| w[0] >= w[1])
            || s.id != format!("u-{}", s.identity_sha256)
            || existing.insert(&s.id, s).is_some()
        {
            return Err(invalid("Invalid/repeated managed catalog identity"));
        }
    }
    let mut operations = Vec::new();
    for s in &import.summary().songs {
        let old = existing.get(&s.entry.id);
        if old.is_some_and(|o| {
            o.identity_sha256 != s.identity_sha256
                || o.source_identity_sha256 != s.source_identity_sha256
        }) {
            return Err(invalid("Catalog identity collision"));
        }
        let decision = match old {
            None => Decision::Add,
            Some(o) if o.revision_sha256 == s.revision_sha256 => Decision::Unchanged,
            Some(_) => match policy {
                DuplicatePolicy::Reject => {
                    return Err(Error::new(
                        "library-import-conflict",
                        format!("Song already has a different revision: {}", s.entry.id),
                    ));
                }
                DuplicatePolicy::KeepExisting => Decision::KeepExisting,
                DuplicatePolicy::Replace => Decision::Replace,
            },
        };
        if decision == Decision::Replace
            && old.is_some_and(|o| o.selectors.iter().any(|n| !s.selectors.contains(n)))
        {
            return Err(Error::new(
                "library-chart-removal",
                "Replacing a song with a selector subset would remove installed charts",
            ));
        }
        operations.push(Operation {
            id: s.entry.id.clone(),
            decision,
            previous_revision_sha256: old.map(|o| o.revision_sha256.clone()),
            revision_sha256: s.revision_sha256.clone(),
        });
    }
    Ok(Plan {
        summary: PlanSummary {
            schema: "ddi-danoni-install-plan/1",
            import_digest: import.summary().digest.clone(),
            expected_catalog_revision: snapshot.revision.clone(),
            duplicate_policy: policy,
            operations,
        },
    })
}
/// Host commit must distinguish a known rejection from an uncertain durable
/// outcome. An uncertain commit must never trigger automatic rollback or retry.
#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum CommitOutcome {
    Committed {
        catalog_revision: String,
    },
    Rejected {
        error: Error,
    },
    NeedsRecovery {
        transaction_id: String,
        reason: String,
    },
}
pub trait Transaction {
    /// Staged data must be invisible to readers until commit. Hosts should keep
    /// provenance beside their managed payloads, deduplicating hashes if desired.
    fn stage_evidence(&mut self, import: &PreparedImport) -> Result<()>;
    fn stage_song(&mut self, song: &PreparedSong, operation: &Operation) -> Result<()>;
    /// Recheck the expected catalog revision atomically (or hold its writer lock
    /// throughout). Publish *all* payloads, evidence and catalog rows together.
    fn commit(&mut self) -> CommitOutcome;
    /// Only called for definitely uncommitted transactions. Must be idempotent.
    fn rollback(&mut self) -> Result<()>;
}
pub trait LibraryStore {
    type Transaction<'a>: Transaction
    where
        Self: 'a;
    /// Compare without mutation for an all-skipped plan. Must fail if stale.
    fn check_revision(&mut self, expected: &str) -> Result<()>;
    /// Must bind expected_catalog_revision and all operation preconditions.
    /// Drop/crash cleanup and recovery are host responsibilities, not inferred
    /// from the presence of a song folder or a temporary manifest.
    fn begin<'a>(&'a mut self, plan: &PlanSummary) -> Result<Self::Transaction<'a>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Begin,
    Evidence,
    Songs,
    ReadyToCommit,
}
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub stage: Stage,
    pub completed: usize,
    pub total: usize,
}
#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Outcome {
    Unchanged {
        catalog_revision: String,
    },
    Installed {
        catalog_revision: String,
        added: usize,
        replaced: usize,
        skipped: usize,
    },
    NeedsRecovery {
        transaction_id: String,
        reason: String,
        import_digest: String,
    },
}
fn rollback(tx: &mut impl Transaction, error: Error) -> Error {
    match tx.rollback() {
        Ok(()) => error,
        Err(e) => Error::new(
            "library-rollback-failed",
            format!("Original error: {error}; rollback error: {e}; host recovery required"),
        ),
    }
}
/// No fallible notification is issued after successful commit: late cancellation
/// cannot relabel installed songs as a failed/cancelled job. The host's atomic
/// commit is the single visibility/linearization point.
pub fn install(
    import: &PreparedImport,
    plan: &Plan,
    store: &mut impl LibraryStore,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<Outcome> {
    if plan.summary.import_digest != import.summary().digest {
        return Err(invalid("Plan belongs to another prepared import"));
    }
    let changing = plan
        .summary
        .operations
        .iter()
        .filter(|o| matches!(o.decision, Decision::Add | Decision::Replace))
        .collect::<Vec<_>>();
    control(Progress {
        stage: Stage::Begin,
        completed: 0,
        total: changing.len(),
    })?;
    if changing.is_empty() {
        store.check_revision(&plan.summary.expected_catalog_revision)?;
        return Ok(Outcome::Unchanged {
            catalog_revision: plan.summary.expected_catalog_revision.clone(),
        });
    }
    let mut tx = store.begin(&plan.summary)?;
    let staged = (|| {
        control(Progress {
            stage: Stage::Evidence,
            completed: 0,
            total: 1,
        })?;
        tx.stage_evidence(import)?;
        for (i, op) in changing.iter().enumerate() {
            control(Progress {
                stage: Stage::Songs,
                completed: i,
                total: changing.len(),
            })?;
            let song = import
                .songs()
                .iter()
                .find(|s| s.summary().entry.id == op.id)
                .ok_or_else(|| invalid("Plan song missing"))?;
            tx.stage_song(song, op)?;
        }
        control(Progress {
            stage: Stage::ReadyToCommit,
            completed: changing.len(),
            total: changing.len(),
        })
    })();
    if let Err(e) = staged {
        return Err(rollback(&mut tx, e));
    }
    match tx.commit() {
        CommitOutcome::Rejected { error } => Err(rollback(&mut tx, error)),
        CommitOutcome::NeedsRecovery {
            transaction_id,
            reason,
        } => Ok(Outcome::NeedsRecovery {
            transaction_id,
            reason,
            import_digest: import.summary().digest.clone(),
        }),
        CommitOutcome::Committed { catalog_revision } => Ok(Outcome::Installed {
            catalog_revision,
            added: changing
                .iter()
                .filter(|o| o.decision == Decision::Add)
                .count(),
            replaced: changing
                .iter()
                .filter(|o| o.decision == Decision::Replace)
                .count(),
            skipped: plan.summary.operations.len() - changing.len(),
        }),
    }
}
/// Hosts can bind a durable plan/journal name to the exact serialized contract.
pub fn plan_digest(plan: &Plan) -> Result<String> {
    let bytes = serde_json::to_vec(&plan.summary).map_err(|e| invalid(e.to_string()))?;
    Ok(sha256(&bytes))
}
