//! Round SD: a room's stage follows the room's chat while it plays
//! (`crate::chat::Following`, read from the room's client each frame,
//! `LiveRoom::room_chat`), and the play judges, scores and logs exactly as
//! without a chat, whatever the host changes.

use super::*;
use ddi_room::Finisher;
use ddi_ui::state::room::{LivePrep, LiveRoom};
use ddi_ui::state::twitch::RoomChat;

/// A room's client as a stage reads it for its chat alone: the room's
/// chat as the test sets it (a practice stage, which never plays live:
/// nothing else of it is read).
struct ChatRoom(Rc<RefCell<Option<RoomChat>>>);

impl LiveRoom for ChatRoom {
    fn room_chat(&self) -> Option<Option<RoomChat>> {
        Some(self.0.borrow().clone())
    }
    fn live_rules(&self) -> ddi_room::live::LiveRules {
        unreachable!("a stage not played live")
    }
    fn view_delay(&self, _: &ddi_room::live::LiveRules) -> ddi_room::live::ViewDelay {
        unreachable!()
    }
    fn room_ms_at(&self, _: u64) -> Option<ddi_room::Millis> {
        unreachable!()
    }
    fn clock_uncertainty_us(&self) -> i64 {
        unreachable!()
    }
    fn song_zero_at(&self, _: ddi_room::StageId) -> Option<ddi_room::Millis> {
        unreachable!()
    }
    fn now_ms(&self) -> u64 {
        unreachable!()
    }
    fn tick(&mut self, _: u64) {
        unreachable!()
    }
    fn take_live_news(&mut self) -> Vec<ddi_room::live::LiveNews> {
        unreachable!()
    }
    fn stop_live_news(&mut self) {
        unreachable!()
    }
    fn live_stream(&mut self, _: ddi_room::StageId) -> Option<&mut ddi_room::live::Outbox> {
        unreachable!()
    }
    fn forget_live(&mut self, _: ddi_room::StageId) {
        unreachable!()
    }
    fn decision(&self, _: ddi_room::StageId) -> Option<(Vec<Vec<Finisher>>, bool)> {
        unreachable!()
    }
    fn live_prep(&self, _: ddi_room::StageId) -> Option<LivePrep> {
        unreachable!()
    }
}

fn host(ceiling: ddi_chat::Level, id: &str) -> RoomChat {
    RoomChat {
        ceiling,
        channel_id: id.into(),
        login: format!("host{id}"),
        room: Some("Room000001".into()),
        named: false,
    }
}

/// What the chat was at each change, and the play's results and log.
type Played = (Vec<Option<(ddi_chat::Level, String)>>, String, String);

/// The stage autoplayed at `fps` with the device's chat at `level`, the
/// room's chat changed at the songs' seconds of `changes`.
fn played(fps: f64, level: ddi_chat::Level, changes: &[(f64, Option<RoomChat>)]) -> Played {
    let song = bundled("beatucada");
    let settings = Settings {
        twitch_chat: level,
        ..Settings::default()
    };
    crate::chat::set_scripted(None);
    ddi_ui::state::twitch::set_dev_live(None);
    let state = crate::chat::tests::fake_live();
    let mut room = room(1, 1, 5);
    let mut now = 0;
    let mut order = begin(&song, &mut room, &mut now);
    let shown = Rc::new(RefCell::new(Some(host(ddi_chat::Level::Messages, "77"))));
    order.setup.chat = shown.borrow().clone();
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.opponents = Some(order.setup.clone());
    config.live = Some(crate::battle::live::LiveHandle {
        room: Rc::new(RefCell::new(ChatRoom(shown.clone()))),
        wall_ms: Rc::new(|| 0),
        host_s: Rc::new(|| 0.0),
    });
    let mut s = PlaySession::start(
        &song,
        order.chart.index,
        &settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    let what = |s: &PlaySession<FakeAudio, FakeInputs>| {
        s.chat().map(|c| {
            let channel = c.live_report().map_or(String::new(), |r| r["channel"].to_string());
            (c.config().level, channel)
        })
    };
    let mut seen = vec![what(&s)];
    let mut pending = changes.to_vec();
    let event = loop {
        *clock.borrow_mut() += 1.0 / fps;
        let t = *clock.borrow();
        if let Some(at) = pending.first().map(|c| c.0)
            && t >= at
        {
            *shown.borrow_mut() = pending.remove(0).1;
            let (_, e) = s.tick(HostTime(t));
            seen.push(what(&s));
            // Never two connections at once, none once it is off.
            let st = state.borrow();
            let open = st.opened.len() - st.closed.len();
            assert_eq!(open, usize::from(s.chat().is_some()), "at {at} s");
            drop(st);
            if let Some(e) = e {
                break e;
            }
            continue;
        }
        let (_, e) = s.tick(HostTime(t));
        if let Some(e) = e {
            break e;
        }
        assert!(t < 300.0, "the stage ends");
    };
    let SessionEvent::Finished { results, facts, .. } = event else {
        panic!("an autoplayed stage plays to its end");
    };
    drop(s);
    crate::chat::set_live(None);
    (seen, format!("{results:?}"), format!("{:?}", facts.log))
}

#[test]
fn a_stage_follows_the_hosts_chat_and_judges_the_same() {
    use ddi_chat::Level::{Emotes, Messages};
    let changes = [
        (20.0, Some(host(Emotes, "77"))),
        (30.0, Some(host(Messages, "77"))),
        (40.0, None),
        (50.0, Some(host(Messages, "77"))),
        (60.0, Some(host(Messages, "88"))),
    ];
    for fps in [60.0, 37.0] {
        let (seen, results, log) = played(fps, Messages, &changes);
        let ch = |id: &str| format!("\"host{id}\"");
        assert_eq!(
            seen,
            [
                Some((Messages, ch("77"))),
                Some((Emotes, ch("77"))),
                Some((Messages, ch("77"))),
                None,
                Some((Messages, ch("77"))),
                Some((Messages, ch("88"))),
            ],
            "{fps} fps"
        );
        // The device's own choice stays its own: never above it.
        let (emotes, ..) = played(fps, Emotes, &changes[..2]);
        assert_eq!(emotes, [Some((Emotes, ch("77"))), Some((Emotes, ch("77"))), Some((Emotes, ch("77")))]);
        // Judged as without a chat.
        let (none, without, without_log) = played(fps, ddi_chat::Level::Off, &changes);
        assert!(none.iter().all(Option::is_none), "the device's option off: none ever");
        assert_eq!(results, without);
        assert_eq!(log, without_log);
    }
}
