//! The usage report (the stats proposal's step 2, room model revision 13,
//! `ddi_room::client_stats`): sent once a socket where Options' Usage
//! statistics is on and the shell said what kind of client it is; with the
//! row Off, nothing leaves.

use std::rc::Rc;

use ddi_platform::storage::{self, MemoryStore};
use ddi_room::ManualClock;
use ddi_room::client_stats::ClientKind;

use super::tests::room::{client, person, room_of, tick};
use crate::state::settings::Settings;

fn kind() -> ClientKind {
    ClientKind {
        app: "web",
        system: "ios",
        form: "tablet",
        gpu: "webgpu",
        touch: "yes",
        build: "0.1.0",
    }
}

/// Bob's reports as the fake server read them, after his socket was cut
/// once and came back.
fn reports_with(usage_stats: bool, said: bool) -> usize {
    storage::install(Rc::new(MemoryStore::default()));
    let mut s = Settings::default();
    assert!(s.usage_stats, "on by default");
    s.usage_stats = usage_stats;
    s.save();
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let bob_online = person(&server, "bob");
    if said {
        bob_online.borrow_mut().set_client_kind(Rc::new(kind));
    }
    bob_online.borrow_mut().set_hidden(false, 1_000);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, bob_online, "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 2_000);
    server.borrow_mut().drop_socket(ddi_room::ParticipantId(2));
    tick(&clock, &mut [&mut alice, &mut bob], 4_000);
    tick(&clock, &mut [&mut alice, &mut bob], 2_000);
    assert_eq!(bob.link_state(), ddi_room::LinkState::Connected);
    let s = server.borrow();
    let reports = s.usage_reports();
    assert!(
        reports
            .iter()
            .all(|(p, _)| *p == ddi_room::ParticipantId(2)),
        "only Bob's shell said what it is"
    );
    if let Some((_, Some(last))) = reports.last() {
        assert_eq!(last.nth, "2");
        let prev = last.prev.as_ref().expect("the socket before");
        assert_eq!(
            (prev.fate.as_str(), prev.code.as_str(), prev.at_end.as_str()),
            ("dropped", "1006", "visible")
        );
    }
    assert!(reports.iter().all(|(_, r)| r.is_some()), "every one taken");
    reports.len()
}

#[test]
fn a_report_goes_once_a_socket_where_the_row_is_on() {
    assert_eq!(reports_with(true, true), 2);
}

#[test]
fn with_the_row_off_nothing_is_sent() {
    assert_eq!(reports_with(false, true), 0);
}

#[test]
fn a_shell_that_says_nothing_of_itself_sends_nothing() {
    assert_eq!(reports_with(true, false), 0);
}
