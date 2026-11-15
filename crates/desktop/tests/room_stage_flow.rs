//! A whole CPU-practice match as the desktop window runs it
//! (`docs/plans/multiplayer-prototype-progress.md`, M3 and M7b's flow), on
//! this machine's GPU offscreen and a fake clock with no sound device: the
//! room's song select (its pick), the stage's song prepared (the chart
//! read, the CPUs' plan checked), its prompt opened by the room over it
//! (Ready, then the room starting the play against the CPUs drawn with
//! their column, its outcome reported, results going back to the room),
//! the standings, the match's results, the next match's pick; then a
//! stage's prompt left before its start withdraws (no forfeit). Nothing
//! is kept in the history.
//! Without a GPU adapter (CI) the stages run undrawn
//! (`common::rig::Draws`): everything of the room, the prompts, the
//! plays' outcomes, the results and their standings, the attacks'
//! ledger, the policy and the log is checked; only what the view says
//! (where the policy's label and the attack lines were drawn) and the
//! pictures are not. `DDI_OFFSCREEN_PPM=<dir>` keeps pictures of the
//! play, the intermission and the results (PPM).

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::play::host::Host;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_room::Phase;
use ddi_ui::screens::data::Difficulty;
use ddi_ui::state::history::Plays;
use common::ui::key;
use common::ui::top;

const SIZE: [u32; 2] = [1280, 720];
const CHART: usize = 1;

#[test]
fn a_practice_match_plays_its_stages_and_comes_back_to_the_room() {
    common::indexes();
    let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), SIZE, true);
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut host = Host::new(Rc::default(), Some(plays.clone()));
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    let mut pictures = common::gpu::Pictures::new();

    // The room over the start menu, its stages played through the host.
    let room = common::practice::open(&mut ui, &env, &host);
    let entry = common::practice::song_entry("beatucada", "Beatucada", CHART, Difficulty::Hard);
    {
        let mut r = room.borrow_mut();
        let rules = ddi_room::MatchRules {
            stages: 2,
            ..r.rules().clone()
        };
        r.save_rules(rules).unwrap();
    }

    let mut play_stage = |ui: &mut ddi_ui::Ui, shot: Option<&str>| {
        // The host's pick: the room's update prepares it (the chart read,
        // the plan checked) and hands its prompt to the host.
        room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
        ui.update(0.1);
        let stage = room.borrow().state().upcoming_stage().unwrap();
        assert!(!room.borrow().loading(stage), "prepared");
        assert_eq!(room.borrow().load_problem(stage), None);
        let mut request = host
            .requested
            .borrow_mut()
            .take()
            .expect("the stage asked for");
        assert!(request.stage.is_some());
        request.auto = true;
        *host.requested.borrow_mut() = Some(request);
        draws.wait_prompt(&mut host, ui, &env);
        ui.update(0.5);
        assert_eq!(top(ui), "play-prompt");
        // Ready: the room counts down (alone with CPUs, all ready), then
        // starts the stage, the prompt pressing Start for the host.
        key(ui, "Enter", "Enter");
        ui.update(0.05);
        assert!(room.borrow().ready());
        {
            let mut r = room.borrow_mut();
            let later = r.now_ms() + 3_000;
            r.tick(later);
        }
        ui.update(0.05);
        draws.poll(&mut host, ui, &env);
        assert!(host.playing(), "{:?}", ui.layers());
        let mut frames = 0;
        let mut columns = 0;
        while host.playing() {
            let now = clock.fetch_add(1_000_000_000 / 4, Ordering::SeqCst) + 1_000_000_000 / 4;
            draws.frame(&mut host, ui, &env, now);
            if let Some(p) = host.running() {
                let opponents = p.session.opponents();
                if !opponents.is_empty() {
                    columns += 1;
                    assert_eq!(opponents.len(), 3);
                }
            }
            // The column's fonts are read on a thread: real time for it
            // before the picture.
            if shot.is_some() && draws.drawn() && (60..120).contains(&frames) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            if frames == 120
                && let Some(name) = shot
            {
                draws.shot(&format!("{name}.ppm"));
            }
            if frames % 16 == 0 {
                draws.wait_gpu();
            }
            frames += 1;
            assert!(frames < 4 * 400, "the play never ended");
        }
        assert!(columns > 0, "the opponents were shown");
        draws.wait_gpu();
        ui.update(0.05);
        assert_eq!(top(ui), "results");
        // The room's mark after the autoplay's note; no Retry, no song
        // offset.
        let shown = common::ui::results_shown(ui);
        assert!(
            shown.notes.starts_with("Autoplay: this play does not count toward your bests. · Stage "),
            "{shown:?}"
        );
        assert!(!shown.retry && !shown.offset, "{shown:?}");
        // Back to room, never Retry: a stage is played once.
        assert!(ui.bounds("SongList").is_some());
        assert!(ui.bounds("Retry").is_none());
        ui.update(3.0);
        // Where the graphs are: the Standings tab first (feedback round
        // B), the stage confirmed, the player and the three CPUs each
        // with how they performed, as their engine-judged plays say.
        let view = env.state.borrow().stage.clone().expect("the standings");
        assert_eq!(view.tab, ddi_ui::screens::results::Tab::Standings);
        assert!(view.confirmed, "{view:?}");
        assert_eq!(view.rows.len(), 4);
        assert_eq!(view.rows.iter().filter(|r| r.me).count(), 1);
        for row in &view.rows {
            assert!(row.place.is_some() && row.award.is_some(), "{row:?}");
            // DDI's score as shown everywhere: the sum (round 3b), a
            // CPU's its own.
            assert!(row.score.parse::<i64>().is_ok(), "{row:?}");
            assert!(!row.grade.is_empty() && !row.life.is_empty(), "{row:?}");
            assert!(row.tiers.iter().sum::<u32>() > 0, "{row:?}");
            assert_eq!(row.level.is_some(), !row.me, "a CPU's stars");
        }
        // The sum, never the proportional part alone (at most a million).
        assert!(
            view.rows
                .iter()
                .any(|r| r.score.parse::<i64>().is_ok_and(|s| s > 1_000_000)),
            "{view:?}"
        );
        // The match's result on the last stage's, not before.
        assert_eq!(view.number == view.of, !view.winners.is_empty(), "{view:?}");
        assert!(ui.bounds("Panel.Standings").is_some() && ui.bounds("Panel.Timing").is_none());
        // Right: the graphs; Left: the standings again.
        key(ui, "ArrowRight", "ArrowRight");
        ui.update(0.3);
        assert!(ui.bounds("Panel.Timing").is_some() && ui.bounds("Panel.Standings").is_none());
        key(ui, "ArrowLeft", "ArrowLeft");
        ui.update(0.3);
        assert!(ui.bounds("Panel.Standings").is_some());
        // Back to the room (Enter: its button has focus).
        key(ui, "Enter", "Enter");
        ui.update(0.5);
        draws.poll(&mut host, ui, &env);
        ui.update(1.0);
        assert!(!host.prompting() && !host.playing());
        // The room's song select: the standings were seen on the
        // results, so they do not pop up again.
        assert_eq!(top(ui), "select", "{:?}", ui.layers());
        assert_eq!(room.borrow().unseen_results(), None);
        assert!(env.state.borrow().stage.is_none(), "gone with the results");
    };

    play_stage(&mut ui, Some("play"));
    assert!(matches!(
        room.borrow().state().phase,
        Phase::Intermission { .. }
    ));
    let view = env.state.borrow().room.clone().unwrap();
    assert!(
        view.mark
            .as_deref()
            .is_some_and(|m| m.starts_with("Stage 1:")),
        "{view:?}"
    );
    assert!(!view.ready, "Ready again for stage 2");
    ui.update(0.5);
    pictures.shot(&mut ui, SIZE, "intermission.ppm");
    play_stage(&mut ui, None);
    assert_eq!(room.borrow().state().phase, Phase::MatchResults);
    ui.update(0.5);
    pictures.shot(&mut ui, SIZE, "results.ppm");
    let standings = room.borrow().state().standings().unwrap();
    assert_eq!(standings.len(), 4);
    assert!(store.is_empty(), "a stage is not kept in the history");
    eprintln!("room flow: {:?}", host.log);

    // The next match's pick: fresh readiness; its prompt left before the
    // start withdraws (the pick reopened, no forfeit).
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    assert_eq!(room.borrow().state().history.len(), 1);
    assert!(!room.borrow().ready());
    ui.update(0.1);
    let mut waited = 0;
    while !host.prompt_ready() {
        draws.poll(&mut host, &mut ui, &env);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    ui.update(0.5);
    key(&mut ui, "Escape", "Escape");
    ui.update(0.5);
    draws.poll(&mut host, &mut ui, &env);
    ui.update(1.0);
    assert_eq!(top(&ui), "select", "{:?}", ui.layers());
    assert!(matches!(
        room.borrow().state().phase,
        Phase::Preparing { window: None, .. }
    ));
    assert!(room.borrow().last_mark().is_none(), "no stage played");
    assert_eq!(store.len(), 0);
}

/// M8b: a practice stage under the attacks' battle (the development
/// switch's) through the window: full bars launch attacks at the
/// checkpoints, each commitment logged, the shocks delivered into the
/// recipients' plays (the player's own among them, avoided by autoplay),
/// and the play ends on its results as any stage's.
#[test]
fn a_practice_stage_with_attacks_launches_them_through_the_window() {
    common::indexes();
    let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), SIZE, true);
    let store = Rc::new(MemoryPlayStore::default());
    let mut host = Host::new(Rc::default(), Some(Plays::new(store.clone())));
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    let room = common::practice::open(&mut ui, &env, &host);
    // Beatucada's Medium chart: long enough for full bars.
    let chart = 4;
    let entry = common::practice::song_entry("beatucada", "Beatucada", chart, Difficulty::Medium);
    {
        let mut r = room.borrow_mut();
        let rules = ddi_room::MatchRules {
            stages: 1,
            battle: ddi_room::BattlePreset::GAUGE_ATTACKS_V0.into(),
            ..r.rules().clone()
        };
        r.save_rules(rules).unwrap();
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    ui.update(0.1);
    let mut request = host.requested.borrow_mut().take().expect("asked for");
    assert_eq!(
        request.stage.as_ref().map(|s| s.battle),
        Some(ddi_room::BattlePreset::GaugeAttacksV0)
    );
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    let mut waited = 0;
    while !host.prompt_ready() {
        draws.poll(&mut host, &mut ui, &env);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    ui.update(0.5);
    // M8 round 3: the prompt's attack policy row (the match has attacks):
    // Up from Ready reaches it, Right makes it Retaliate; the play starts
    // with the policy the room has then.
    use ddi_room::attacks::Targeting;
    for _ in 0..4 {
        if ui.focused() == Some("Opt.room.targeting") {
            break;
        }
        key(&mut ui, "ArrowUp", "ArrowUp");
        ui.update(0.05);
    }
    assert_eq!(ui.focused(), Some("Opt.room.targeting"), "{:?}", ui.layers());
    key(&mut ui, "ArrowRight", "ArrowRight");
    ui.update(0.05);
    assert_eq!(room.borrow().my_targeting(), Targeting::Retaliate);
    key(&mut ui, "ArrowDown", "ArrowDown");
    ui.update(0.05);
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    {
        let mut r = room.borrow_mut();
        let later = r.now_ms() + 3_000;
        r.tick(later);
    }
    ui.update(0.05);
    draws.poll(&mut host, &mut ui, &env);
    assert!(host.playing(), "{:?}", ui.layers());
    let mut frames = 0;
    let mut attacks = 0;
    let mut against_me = 0;
    let mut label = None;
    let (mut links, mut lines, mut outlines) = ((0, 0), 0, 0);
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 4, Ordering::SeqCst) + 1_000_000_000 / 4;
        // In play: the label by the power bar says the policy, and a
        // press on it (as Tab) makes it the next one.
        if frames == 40 {
            let p = host.running().expect("playing");
            let b = p.session.battle().expect("a stage");
            assert_eq!(b.policy(&p.session.player), Some(Targeting::Retaliate));
            // (Where its label was drawn, where the play is drawn.)
            label = host.feed.borrow().policy_at;
            assert_eq!(label.is_some(), draws.drawn(), "its label drawn");
            for pressed in [true, false] {
                host.feed.borrow_mut().edges.push(ddi_platform::RawInput {
                    device: ddi_platform::DeviceId::Touch,
                    control: ddi_session::play::TOUCH_POLICY_CONTROL.into(),
                    pressed,
                    host_time: HostTime(now as f64 / 1e9 - 0.2),
                    slot: 0,
                });
            }
        }
        if frames == 44 {
            let p = host.running().expect("playing");
            let b = p.session.battle().expect("a stage");
            assert_eq!(b.policy(&p.session.player), Some(Targeting::Leader));
            assert_eq!(p.session.player.log().policies.len(), 1);
        }
        draws.frame(&mut host, &mut ui, &env, now);
        if let Some(p) = host.running()
            && let Some(b) = p.session.battle()
            && let Some(a) = b.attacks()
        {
            attacks = a.ledger().attacks().len();
            against_me = p.session.player.log().hazards.len() / 8;
        }
        // M8 round 3 (S5): who attacks whom, as drawn for the player's
        // own attacks: a line (while it shows) between the field and the
        // opponent's miniature, which is outlined while the attack lasts.
        let fields = host.opponent_fields();
        for l in host.attack_links() {
            let f = fields.as_ref().expect("the miniatures");
            assert_eq!(f.slots[l.slot].seat, l.seat);
            assert!(l.outline > 0.0 || l.line > 0.0, "{l:?}");
            if l.line > 0.0 {
                assert!(l.path.len() >= 2, "{l:?}");
                lines += 1;
            } else {
                assert!(l.path.is_empty());
                outlines += 1;
            }
            if l.outgoing {
                links.0 += 1;
            } else {
                links.1 += 1;
            }
        }
        if frames % 16 == 0 {
            draws.wait_gpu();
        }
        frames += 1;
        assert!(frames < 4 * 400, "the play never ended");
    }
    draws.wait_gpu();
    ui.update(0.05);
    assert_eq!(top(&ui), "results");
    let shown = common::ui::results_shown(&ui);
    assert_eq!(
        shown.notes,
        "Autoplay: this play does not count toward your bests. · Stage 1: place 1 of 4 · +12 pts"
    );
    assert!(!shown.retry && !shown.offset, "{shown:?}");
    assert!(attacks > 0, "full bars launched");
    // The player launched (autoplayed, its bar fills) and its links
    // showed: lines for their time, outlines after.
    if draws.drawn() {
        assert!(links.0 > 0, "outgoing links drawn: {links:?}");
        assert!(lines > 0 && outlines > lines, "{lines} lines, {outlines} outlines");
    }
    eprintln!("links drawn: {} outgoing, {} incoming frames", links.0, links.1);
    // The standings count them: every line says its attacks and shocks,
    // and those launched add up to the ledger's.
    ui.update(1.0);
    let view = env.state.borrow().stage.clone().expect("the standings");
    assert!(view.rows.iter().all(|r| r.attacks.is_some()), "{view:?}");
    let launched: u32 = view
        .rows
        .iter()
        .filter_map(|r| r.attacks)
        .map(|a| a.0)
        .sum();
    assert_eq!(launched as usize, attacks, "{view:?}");
    let logged = host
        .log
        .iter()
        .filter(|l| l.starts_with("attack: committed"))
        .count();
    assert!(logged >= attacks, "{:?}", host.log);
    assert!(
        host.log.iter().filter(|l| l.contains("→you")).count() >= against_me,
        "{:?}",
        host.log
    );
    assert!(store.is_empty(), "a stage is not kept in the history");
    // The label under the gauge's left end, inside the field, and the
    // policy the play left is the room's for the next stage.
    if let Some([l, t, r, b]) = label {
        assert!(l > 400.0 && r < 640.0 && t < 20.0 && b > 30.0 && b < 80.0, "{l} {t} {r} {b}");
    }
    assert_eq!(room.borrow().my_targeting(), Targeting::Leader);
    eprintln!("attacks: {attacks}, {against_me} against the player");
}
