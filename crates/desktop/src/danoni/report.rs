//! What a DanOni import tells the screens, as values made where it is
//! found (the importer, its client, the app) and said in the player's
//! language only where the import view is made, on the UI's thread
//! (`docs/plans/localization.md`, step 4). What the importer and the
//! system say themselves stays theirs: data, or the kind of failure it
//! is with their words after it (`ddi_ui::screens::errors`).

use ddi_importer_protocol::{Counts, Failure, Outcome, Stage};
use ddi_l10n::UiText;
use ddi_l10n::msg::danoni as m;
use ddi_ui::screens::data::ImportReportView;
use ddi_ui::screens::errors::{self, FailureKind};
use std::io;

/// What an import is doing, as the status line says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// Begun, before the importer said anything.
    Preparing,
    /// The importer reading its catalog of earlier imports.
    OpeningCatalog,
    /// The importer's own step.
    Importer(Stage),
    /// Ended; the importer closing.
    Finishing,
    /// Cancelled; the importer stopping.
    Cancelling,
}

impl Step {
    pub(super) fn text(self) -> UiText {
        match self {
            Step::Preparing => m::stage_preparing(),
            Step::OpeningCatalog => m::stage_opening_catalog(),
            Step::Importer(stage) => stage_text(stage),
            Step::Finishing => m::stage_finishing(),
            Step::Cancelling => m::stage_cancelling(),
        }
    }
}

fn stage_text(stage: Stage) -> UiText {
    match stage {
        Stage::OpeningWork => m::stage_opening_work(),
        Stage::Downloading => m::stage_downloading(),
        Stage::ReadingCharts => m::stage_reading_charts(),
        Stage::CheckingCharts => m::stage_checking_charts(),
        Stage::ConvertingMusic => m::stage_converting_music(),
        Stage::MappingTiming => m::stage_mapping_timing(),
        Stage::MakingArtwork => m::stage_making_artwork(),
        Stage::PreparingSongs => m::stage_preparing_songs(),
        Stage::FinishingPreparation => m::stage_finishing_preparation(),
        Stage::SavingPrepared => m::stage_saving_prepared(),
        Stage::CheckingImport => m::stage_checking_import(),
        Stage::CheckingFiles => m::stage_checking_files(),
        Stage::VerifyingCharts => m::stage_verifying_charts(),
        Stage::AddingDetails => m::stage_adding_details(),
        Stage::FinishingChecks => m::stage_finishing_checks(),
        Stage::CheckingPrepared => m::stage_checking_prepared(),
        Stage::CheckingExisting => m::stage_checking_existing(),
        Stage::PreparingUpdate => m::stage_preparing_update(),
        Stage::SavingDetails => m::stage_saving_details(),
        Stage::AddingSongs => m::stage_adding_songs(),
        Stage::FinishingImport => m::stage_finishing_import(),
    }
}

/// A system's error as it was found (a file's, a process's): its kind
/// and its words, said in the player's language when shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SystemError {
    kind: io::ErrorKind,
    words: String,
}

impl SystemError {
    /// Words of the importer's own, of no kind the system gives.
    pub(super) fn other(words: impl Into<String>) -> SystemError {
        SystemError {
            kind: io::ErrorKind::Other,
            words: words.into(),
        }
    }

    fn text(&self) -> UiText {
        errors::io_error_text(&io::Error::new(self.kind, self.words.clone()))
    }
}

impl From<io::Error> for SystemError {
    fn from(e: io::Error) -> SystemError {
        SystemError {
            kind: e.kind(),
            words: e.to_string(),
        }
    }
}

/// What went wrong: a line of a report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Problem {
    /// The address typed is not a whole http(s) URL without credentials.
    InvalidUrl,
    /// What the importer said failed: its code and its (or its SDK's) words.
    Importer(Failure),
    /// After the import, the importer could not list the library's songs.
    Refresh(Failure),
    /// The game could not take the importer's new list of songs: why
    /// (the client's technical words).
    CatalogRejected(String),
    /// The importer's new list of songs ended early; the last whole one
    /// is kept.
    CatalogIncomplete,
    /// The packaged importer is missing or damaged: what (technical).
    Bundle(String),
    /// The system does not say where the data folder is.
    NoDataFolder,
    /// The system does not say where the cache folder is.
    NoCacheFolder,
    /// The importer's storage could not be made.
    Storage(SystemError),
    /// A folder of the importer's storage is a file or a symbolic link.
    StorageNotFolder,
    /// The importer, or a thread talking with it, could not be started.
    Start(SystemError),
    /// The importer could not open its catalog, with what it said.
    CatalogUnopened(Option<Failure>),
    /// The pipes to the importer closed or failed.
    Connection(SystemError),
    /// The packaged importer speaks another protocol.
    Incompatible,
    /// The importer answered against its protocol: what (technical).
    Protocol(String),
    /// The importer's process ended with a failure: its status.
    Exited(String),
    /// The importer's process kept running after it finished.
    NoExit,
    /// The importer worked past its time limit.
    TimedOut,
    /// The importer did not stop in time when asked.
    NotStopped,
    /// The client's thread stopped by itself.
    ClientStopped,
    /// The import ended without a report.
    LostResult,
    /// The end of what the importer wrote to its diagnostics (data).
    Diagnostics(String),
}

impl Problem {
    pub(super) fn text(&self) -> UiText {
        match self {
            Problem::InvalidUrl => m::url_invalid(),
            Problem::Importer(f) => failure_text(f),
            Problem::Refresh(f) => m::refresh_failed(failure_text(f)),
            Problem::CatalogRejected(why) => m::catalog_rejected(errors::error_text(why)),
            Problem::CatalogIncomplete => m::catalog_incomplete(),
            Problem::Bundle(why) => m::bundle_damaged(errors::error_text(why)),
            Problem::NoDataFolder => m::no_data_folder(),
            Problem::NoCacheFolder => m::no_cache_folder(),
            Problem::Storage(e) => m::storage_failed(e.text()),
            Problem::StorageNotFolder => m::storage_not_folder(),
            Problem::Start(e) => m::start_failed(e.text()),
            Problem::CatalogUnopened(None) => m::catalog_unopened(),
            Problem::CatalogUnopened(Some(f)) => m::catalog_unopened_because(failure_text(f)),
            Problem::Connection(e) => m::connection_lost(e.text()),
            Problem::Incompatible => m::incompatible(),
            Problem::Protocol(what) => m::protocol(UiText::data(what)),
            Problem::Exited(status) => m::exited(UiText::data(status)),
            Problem::NoExit => m::no_exit(),
            Problem::TimedOut => m::timed_out(),
            Problem::NotStopped => m::not_stopped(),
            Problem::ClientStopped => m::client_stopped(),
            Problem::LostResult => m::lost_result(),
            Problem::Diagnostics(words) => UiText::data(words),
        }
    }
}

/// A failure the importer reported: ours where its code is the
/// importer's own report, else the kind of failure its words or its code
/// say with them after it, else its words.
fn failure_text(f: &Failure) -> UiText {
    match f.code.as_str() {
        "importer-busy" => m::busy(),
        "importer-unavailable" => m::unavailable(),
        "importer-panic" => m::importer_stopped(),
        "invalid-url" => m::url_invalid(),
        code => {
            let detail = format!("{code}: {}", f.message);
            match errors::kind_of(&detail).or_else(|| code_kind(code)) {
                Some(kind) => errors::failure(kind, &detail),
                None => UiText::data(detail),
            }
        }
    }
}

/// The kind of failure the importer's SDK says by its code alone.
fn code_kind(code: &str) -> Option<FailureKind> {
    match code {
        "request-timeout" | "origin-timeout" | "collection-timeout" => Some(FailureKind::Timeout),
        "http-error" | "dns-error" | "dns-empty" | "dns-identity" | "partial-response"
        | "redirect-loop" | "redirect-limit" | "tls-root-bundle" => Some(FailureKind::Network),
        "site-terms-denied" => Some(FailureKind::Denied),
        _ => None,
    }
}

/// How an import ended: the status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Summary {
    /// Its songs were imported.
    Installed(Counts),
    /// Its songs were all imported before, unchanged.
    Unchanged,
    /// The library needs recovery.
    NeedsRecovery,
    /// Cancelled while the importer worked: nothing published.
    Cancelled,
    /// Cancelled before the import began.
    CancelledEarly,
    /// Nothing imported.
    Failed,
    /// Files from an address allowed for this import, the choice for
    /// later ones not saved: why (the storage's words).
    NotRemembered(String),
}

/// An import's report, as values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Report {
    pub summary: Summary,
    /// What went wrong ("Details").
    pub details: Vec<Problem>,
    /// What went wrong after its songs were imported ("After import").
    pub after: Vec<Problem>,
}

impl Report {
    pub(super) fn new(summary: Summary) -> Report {
        Report {
            summary,
            details: Vec::new(),
            after: Vec::new(),
        }
    }

    /// Nothing imported, `problem` why.
    pub(super) fn failed(problem: Problem) -> Report {
        Report {
            details: vec![problem],
            ..Report::new(Summary::Failed)
        }
    }

    /// The report of an import's end as the importer told it.
    pub(super) fn ended(
        status: Outcome,
        songs: Option<Counts>,
        failure: Option<Failure>,
        refresh: Option<Failure>,
    ) -> Report {
        let mut report = Report::new(match status {
            Outcome::Installed => Summary::Installed(songs.unwrap_or_default()),
            Outcome::Unchanged => Summary::Unchanged,
            Outcome::Cancelled => Summary::Cancelled,
            Outcome::Failed => Summary::Failed,
            Outcome::NeedsRecovery => Summary::NeedsRecovery,
        });
        report.details.extend(failure.map(Problem::Importer));
        report.details.extend(refresh.map(Problem::Refresh));
        report
    }

    /// Nothing was imported.
    pub(super) fn error(&self) -> bool {
        matches!(self.summary, Summary::Failed | Summary::NeedsRecovery)
    }

    /// The summary in the player's language.
    pub(super) fn summary_text(&self) -> UiText {
        match &self.summary {
            Summary::Installed(c) => m::done(c.added, c.replaced, c.skipped),
            Summary::Unchanged => m::unchanged(),
            Summary::NeedsRecovery => m::needs_recovery(),
            Summary::Cancelled => m::cancelled(),
            Summary::CancelledEarly => m::cancelled_early(),
            Summary::Failed => m::failed(),
            Summary::NotRemembered(why) => m::not_remembered(errors::error_text(why)),
        }
    }

    /// Every line of what went wrong, in the player's language.
    pub(super) fn lines(&self) -> impl Iterator<Item = UiText> + '_ {
        self.details.iter().chain(&self.after).map(Problem::text)
    }

    /// As the import dialog shows it, in the player's language.
    pub(super) fn view(&self) -> ImportReportView {
        let sections = [
            (m::details(), &self.details),
            (m::after_import(), &self.after),
        ]
        .into_iter()
        .filter(|(_, problems)| !problems.is_empty())
        .map(|(title, problems)| {
            (
                title.into(),
                problems.iter().map(|p| p.text().into()).collect(),
            )
        })
        .collect();
        ImportReportView::Worded {
            summary: self.summary_text().into(),
            error: self.error(),
            sections,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_step() -> Vec<Step> {
        use Stage::*;
        let mut steps = vec![
            Step::Preparing,
            Step::OpeningCatalog,
            Step::Finishing,
            Step::Cancelling,
        ];
        steps.extend(
            [
                OpeningWork,
                Downloading,
                ReadingCharts,
                CheckingCharts,
                ConvertingMusic,
                MappingTiming,
                MakingArtwork,
                PreparingSongs,
                FinishingPreparation,
                SavingPrepared,
                CheckingImport,
                CheckingFiles,
                VerifyingCharts,
                AddingDetails,
                FinishingChecks,
                CheckingPrepared,
                CheckingExisting,
                PreparingUpdate,
                SavingDetails,
                AddingSongs,
                FinishingImport,
            ]
            .map(Step::Importer),
        );
        steps
    }

    fn failure(code: &str, message: &str) -> Failure {
        Failure {
            code: code.into(),
            message: message.into(),
        }
    }

    fn every_problem() -> Vec<Problem> {
        let io = || SystemError::from(io::Error::from(io::ErrorKind::PermissionDenied));
        vec![
            Problem::InvalidUrl,
            Problem::Importer(failure("importer-busy", "Another importer job is running")),
            Problem::Importer(failure("importer-unavailable", "…")),
            Problem::Importer(failure("importer-panic", "…")),
            Problem::Importer(failure("http-error", "error sending request")),
            Problem::Importer(failure("unknown-thing", "odd")),
            Problem::Refresh(failure("filesystem", "No such file (os error 2)")),
            Problem::CatalogRejected("Invalid catalog pack name".into()),
            Problem::CatalogIncomplete,
            Problem::Bundle("Importer artifact hash differs".into()),
            Problem::NoDataFolder,
            Problem::NoCacheFolder,
            Problem::Storage(io()),
            Problem::StorageNotFolder,
            Problem::Start(io()),
            Problem::CatalogUnopened(None),
            Problem::CatalogUnopened(Some(failure("filesystem", "…"))),
            Problem::Connection(SystemError::other("Importer output closed")),
            Problem::Incompatible,
            Problem::Protocol("Incomplete importer snapshot".into()),
            Problem::Exited("exit status: 1".into()),
            Problem::NoExit,
            Problem::TimedOut,
            Problem::NotStopped,
            Problem::ClientStopped,
            Problem::LostResult,
        ]
    }

    fn every_summary() -> Vec<Summary> {
        vec![
            Summary::Installed(Counts {
                added: 1,
                replaced: 0,
                skipped: 2,
            }),
            Summary::Unchanged,
            Summary::NeedsRecovery,
            Summary::Cancelled,
            Summary::CancelledEarly,
            Summary::Failed,
            Summary::NotRemembered("disk is full".into()),
        ]
    }

    #[test]
    fn every_step_problem_and_summary_is_a_message() {
        ddi_l10n::set_language("qps-ploc");
        let mut texts: Vec<UiText> = every_step().into_iter().map(Step::text).collect();
        texts.extend(every_problem().iter().map(Problem::text));
        texts.extend(
            every_summary()
                .into_iter()
                .map(|s| Report::new(s).summary_text()),
        );
        let mut both = Report::failed(Problem::NoExit);
        both.after.push(Problem::NotStopped);
        let view = both.view();
        assert_eq!(view.sections().len(), 2);
        for (title, _) in &view.sections() {
            assert!(title.starts_with('«'), "{title}");
        }
        for text in texts.iter().filter(|t| !t.starts_with('‹')) {
            assert!(
                text.starts_with('«') && text.ends_with('»'),
                "{text} is not a message"
            );
        }
        // The importer's and the system's own words are data.
        assert!(
            Problem::Diagnostics("thread panicked".into())
                .text()
                .starts_with('‹')
        );
        assert!(
            Problem::Importer(failure("unknown-thing", "odd"))
                .text()
                .contains("‹unknown-thing: odd›")
        );
        ddi_l10n::set_language("en");
    }

    #[test]
    fn english_reads_as_the_importer_wrote_it_before() {
        ddi_l10n::set_language("en");
        assert_eq!(
            &*Step::Importer(Stage::ConvertingMusic).text(),
            "Converting music…"
        );
        assert_eq!(&*Step::Cancelling.text(), "Cancelling DanOni import…");
        let done = Report::ended(
            Outcome::Installed,
            Some(Counts {
                added: 1,
                replaced: 2,
                skipped: 3,
            }),
            None,
            None,
        );
        assert!(!done.error());
        assert_eq!(
            done.view().summary(),
            "DanOni import complete: 1 added, 2 replaced, 3 skipped."
        );
        assert!(done.view().sections().is_empty());
        let failed = Report::ended(
            Outcome::Failed,
            None,
            Some(failure("http-error", "dns error: no record")),
            Some(failure("filesystem", "Permission denied (os error 13)")),
        );
        let view = failed.view();
        assert!(view.error());
        assert_eq!(view.summary(), "DanOni import could not finish.");
        assert_eq!(view.sections().len(), 1);
        assert_eq!(view.sections()[0].0, "Details");
        assert_eq!(
            view.sections()[0].1,
            [
                "the network failed (http-error: dns error: no record)",
                "Library refresh failed: access was refused (filesystem: Permission denied (os error 13))",
            ]
        );
        let mut after = Report::ended(Outcome::Unchanged, None, None, None);
        after.after.push(Problem::NoExit);
        let view = after.view();
        assert!(!view.error());
        assert_eq!(view.sections()[0].0, "After import");
    }

    #[test]
    fn a_report_is_said_in_the_language_it_is_shown_in() {
        let report = Report::failed(Problem::TimedOut);
        ddi_l10n::set_language("ja");
        let ja = report.view();
        assert_eq!(ja.summary(), "DanOniのインポートを完了できませんでした。");
        ddi_l10n::set_language("en");
        assert_eq!(report.view().summary(), "DanOni import could not finish.");
        assert_ne!(ja.sections(), report.view().sections());
    }
}
