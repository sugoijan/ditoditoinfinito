use super::*;

/// What a rig of tests plays on: a quit button at (40, 40), the policy
/// label under it, the volume's button beside it, and four lanes along
/// the bottom, each 100 wide from x 100 (a slide changes lane only 8
/// inside the next, as the touch controls' dead band).
struct Rig {
    pointers: PlayPointers<i32>,
    inputs: Vec<RawInput>,
    volume: Vec<VolumeEdge>,
    open: bool,
    watch: Vec<(Option<usize>, [f32; 4])>,
    skip: Option<[f32; 4]>,
    at: f64,
}

const QUIT: [f64; 2] = [40.0, 40.0];
const POLICY: [f64; 2] = [200.0, 40.0];
const VOLUME: [f64; 2] = [40.0, 100.0];

fn lane_at([x, y]: [f64; 2], held: Option<usize>) -> Option<usize> {
    if y < 500.0 {
        return None;
    }
    let lane = ((x - 100.0) / 100.0).floor();
    if !(0.0..4.0).contains(&lane) {
        return None;
    }
    let lane = lane as usize;
    match held {
        Some(old) if lane != old => {
            let inner = x - 100.0 - lane as f64 * 100.0;
            (8.0..=92.0).contains(&inner).then_some(lane)
        }
        _ => Some(lane),
    }
}

fn lane(n: usize) -> [f64; 2] {
    [150.0 + n as f64 * 100.0, 600.0]
}

impl Rig {
    fn new() -> Rig {
        Rig {
            pointers: PlayPointers::default(),
            inputs: Vec::new(),
            volume: Vec::new(),
            open: false,
            watch: Vec::new(),
            skip: None,
            at: 1.0,
        }
    }

    fn press(&mut self, id: i32, pos: [f64; 2], primary: bool, finger: bool) -> Landed {
        self.at += 0.01;
        let places = Places {
            volume: Some(VolumeAt {
                button: [40.0, 100.0, 20.0],
                plate: self.open.then_some([70.0, 60.0, 110.0, 260.0]),
            }),
            quit: Some([40.0, 40.0, 20.0]),
            policy: Some([160.0, 30.0, 240.0, 50.0]),
            watch: &self.watch,
            skip: self.skip,
        };
        let mut sink = Sink {
            inputs: &mut self.inputs,
            volume: &mut self.volume,
            at: HostTime(self.at),
        };
        let press = Press {
            pos,
            primary,
            finger,
        };
        self.pointers
            .down(id, press, &places, || lane_at(pos, None), &mut sink)
    }

    fn down(&mut self, id: i32, pos: [f64; 2]) -> Landed {
        self.press(id, pos, true, true)
    }

    fn moved(&mut self, id: i32, pos: [f64; 2]) -> bool {
        self.at += 0.01;
        let mut sink = Sink {
            inputs: &mut self.inputs,
            volume: &mut self.volume,
            at: HostTime(self.at),
        };
        self.pointers
            .moved(id, pos, |old| lane_at(pos, Some(old)), &mut sink)
    }

    fn up(&mut self, id: i32, pos: [f64; 2]) -> bool {
        self.lift(id, pos, false)
    }

    fn lift(&mut self, id: i32, pos: [f64; 2], cancel: bool) -> bool {
        self.at += 0.01;
        let mut sink = Sink {
            inputs: &mut self.inputs,
            volume: &mut self.volume,
            at: HostTime(self.at),
        };
        self.pointers.up(id, pos, cancel, &mut sink)
    }

    fn release_all(&mut self) {
        self.at += 0.01;
        let mut sink = Sink {
            inputs: &mut self.inputs,
            volume: &mut self.volume,
            at: HostTime(self.at),
        };
        self.pointers.release_all(&mut sink);
    }

    /// The play's edges since the last look: `(control, pressed)`.
    fn edges(&mut self) -> Vec<(String, bool)> {
        self.inputs
            .drain(..)
            .map(|r| {
                assert_eq!(r.device, DeviceId::Touch);
                assert_eq!(r.slot, 0);
                (r.control, r.pressed)
            })
            .collect()
    }

    fn volume_edges(&mut self) -> Vec<(VolumePhase, [f32; 2])> {
        self.volume.drain(..).map(|e| (e.phase, e.pos)).collect()
    }
}

fn e(control: &str, pressed: bool) -> (String, bool) {
    (control.to_string(), pressed)
}

/// Review 2026-11-22, chat-touch 2: a finger holding the policy label
/// keeps it alone; another finger's lift on a lane is that lane's.
#[test]
fn a_second_fingers_lift_is_its_lanes_while_the_label_is_held() {
    let mut r = Rig::new();
    assert!(r.down(1, POLICY).holds);
    assert!(r.down(2, lane(1)).holds);
    assert_eq!(r.edges(), [e("policy", true), e("lane:1", true)]);
    assert!(r.up(2, lane(1)));
    assert_eq!(r.edges(), [e("lane:1", false)]);
    // Released off the label: still the label's.
    assert!(r.up(1, lane(3)));
    assert_eq!(r.edges(), [e("policy", false)]);
    assert!(r.pointers.is_empty());
    // A lift with nothing held is nobody's.
    assert!(!r.up(1, POLICY));
    assert_eq!(r.edges(), []);
}

/// Every pointer on a target is counted: two fingers on the quit button
/// are one press, released by the last (desktop kept one holder: a second
/// finger pressed it again, which a play reads as a second tap).
#[test]
fn fingers_on_one_target_press_it_once_and_the_last_lifts_it() {
    let mut r = Rig::new();
    r.down(1, QUIT);
    r.down(2, QUIT);
    assert_eq!(r.edges(), [e("cancel", true)]);
    assert_eq!(r.pointers.held(Hold::Quit), 2);
    r.up(1, QUIT);
    assert_eq!(r.edges(), []);
    r.up(2, QUIT);
    assert_eq!(r.edges(), [e("cancel", false)]);
    // Lifts out of order on a lane.
    r.down(1, lane(2));
    r.down(2, lane(2));
    r.down(3, lane(2));
    assert_eq!(r.edges(), [e("lane:2", true)]);
    r.up(2, lane(2));
    r.up(1, lane(2));
    assert_eq!(r.edges(), []);
    r.up(3, lane(2));
    assert_eq!(r.edges(), [e("lane:2", false)]);
    // The policy label, the same way.
    r.down(5, POLICY);
    r.down(6, POLICY);
    r.up(5, POLICY);
    assert_eq!(r.edges(), [e("policy", true)]);
    r.up(6, POLICY);
    assert_eq!(r.edges(), [e("policy", false)]);
}

/// A slide keeps its lane until well inside another; the other's press
/// comes after the old one's release, and a lane another finger holds
/// stays held.
#[test]
fn a_finger_slides_between_lanes_past_the_dead_band() {
    let mut r = Rig::new();
    r.down(1, lane(1));
    r.down(2, lane(1));
    assert_eq!(r.edges(), [e("lane:1", true)]);
    // Just inside lane 2: kept.
    assert!(r.moved(1, [304.0, 600.0]));
    assert_eq!(r.edges(), []);
    // Off the controls: kept.
    assert!(r.moved(1, [640.0, 100.0]));
    assert_eq!(r.edges(), []);
    // Well inside lane 2: lane 1 still held by the second finger.
    assert!(r.moved(1, [320.0, 600.0]));
    assert_eq!(r.edges(), [e("lane:2", true)]);
    assert!(r.moved(2, lane(0)));
    assert_eq!(r.edges(), [e("lane:1", false), e("lane:0", true)]);
    // Onto the lane the other holds: one press stays.
    assert!(r.moved(2, lane(2)));
    assert_eq!(r.edges(), [e("lane:0", false)]);
    r.up(1, lane(2));
    assert_eq!(r.edges(), []);
    r.up(2, lane(2));
    assert_eq!(r.edges(), [e("lane:2", false)]);
    // A pointer that holds nothing does not slide onto a lane.
    assert!(!r.moved(9, lane(3)));
    assert_eq!(r.edges(), []);
}

/// A pointer id that goes down again before its lift: taken, nothing more
/// (on any target), and its one lift releases.
#[test]
fn a_reused_id_is_taken_and_presses_nothing() {
    let mut r = Rig::new();
    r.down(1, QUIT);
    let again = r.down(1, QUIT);
    assert_eq!(
        again,
        Landed {
            taken: true,
            holds: false,
            touch: false
        }
    );
    let again = r.down(1, lane(0));
    assert!(again.taken && !again.holds && again.touch);
    let again = r.down(1, VOLUME);
    assert!(again.taken && !again.holds);
    assert_eq!(r.edges(), [e("cancel", true)]);
    assert_eq!(r.volume_edges(), []);
    assert_eq!(r.pointers.held(Hold::Quit), 1);
    assert_eq!(r.pointers.holding(1), Some(Hold::Quit));
    r.up(1, lane(0));
    assert_eq!(r.pointers.holding(1), None);
    assert_eq!(r.edges(), [e("cancel", false)]);
    assert!(r.pointers.is_empty());
}

/// The focus lost: each target released once (stamped then), the
/// volume's drag ended where it last was, nothing kept.
#[test]
fn release_all_lets_go_of_each_target_once() {
    let mut r = Rig::new();
    r.down(1, lane(0));
    r.down(2, lane(0));
    r.down(3, lane(3));
    r.down(4, QUIT);
    r.down(5, POLICY);
    r.down(6, VOLUME);
    r.moved(6, [44.0, 140.0]);
    r.edges();
    r.volume_edges();
    r.release_all();
    let at = r.at;
    assert!(r.inputs.iter().all(|i| i.host_time == HostTime(at)));
    let mut edges = r.edges();
    edges.sort();
    assert_eq!(
        edges,
        [
            e("cancel", false),
            e("lane:0", false),
            e("lane:3", false),
            e("policy", false)
        ]
    );
    assert_eq!(r.volume_edges(), [(VolumePhase::Cancel, [44.0, 140.0])]);
    assert!(r.pointers.is_empty());
    // Their lifts come later: nobody's.
    assert!(!r.up(1, lane(0)));
    assert_eq!(r.edges(), []);
    // A finger down again presses.
    r.down(2, lane(0));
    assert_eq!(r.edges(), [e("lane:0", true)]);
    // A drag that never moved ends where it went down.
    r.down(7, VOLUME);
    r.volume_edges();
    r.release_all();
    assert_eq!(r.volume_edges(), [(VolumePhase::Cancel, [40.0, 100.0])]);
}

/// The volume: its pointers' edges to the play's view, never the play's
/// inputs; a second finger's too; a press elsewhere while the slider is
/// open closes it and goes on.
#[test]
fn the_volume_takes_its_pointers_edges() {
    let mut r = Rig::new();
    let landed = r.down(1, VOLUME);
    assert!(landed.taken && landed.holds && !landed.touch);
    r.moved(1, [41.0, 120.0]);
    r.down(2, VOLUME);
    r.lift(2, [42.0, 100.0], true);
    r.up(1, [45.0, 130.0]);
    assert_eq!(r.edges(), []);
    assert_eq!(
        r.volume_edges(),
        [
            (VolumePhase::Down, [40.0, 100.0]),
            (VolumePhase::Move, [41.0, 120.0]),
            (VolumePhase::Down, [40.0, 100.0]),
            (VolumePhase::Cancel, [42.0, 100.0]),
            (VolumePhase::Up, [45.0, 130.0]),
        ]
    );
    // The slider open: a press on a lane closes it and presses the lane.
    r.open = true;
    let landed = r.down(3, lane(1));
    assert!(landed.holds);
    assert_eq!(r.volume_edges(), [(VolumePhase::Down, [250.0, 600.0])]);
    assert_eq!(r.edges(), [e("lane:1", true)]);
    // On its plate: the volume's.
    assert!(r.down(4, [90.0, 200.0]).holds);
    assert_eq!(r.volume_edges(), [(VolumePhase::Down, [90.0, 200.0])]);
}

/// The mouse presses the buttons, the volume and the fields, never a
/// lane; a press other than the primary reaches no button (a pen's other
/// button still presses a lane, as on the web).
#[test]
fn the_mouse_presses_no_lane_and_only_the_primary_presses_buttons() {
    let mut r = Rig::new();
    assert_eq!(r.press(0, lane(1), true, false), Landed::default());
    assert!(r.press(0, QUIT, true, false).holds);
    r.up(0, QUIT);
    assert_eq!(r.edges(), [e("cancel", true), e("cancel", false)]);
    assert_eq!(r.press(1, QUIT, false, true), Landed::default());
    assert!(!r.press(1, VOLUME, false, true).holds);
    assert_eq!(r.volume_edges(), []);
    assert!(r.press(1, lane(2), false, true).holds);
    assert_eq!(r.edges(), [e("lane:2", true)]);
    // A finger off every target: touch is the source, nothing held.
    let off = r.down(2, [640.0, 100.0]);
    assert_eq!(
        off,
        Landed {
            taken: false,
            holds: false,
            touch: true
        }
    );
    assert!(!r.moved(2, lane(3)));
    assert_eq!(r.edges(), []);
}

/// One order where places overlap: the volume, the quit button, the
/// policy label, the skip's plate, the fields, the lanes.
#[test]
fn overlapping_places_go_in_one_order() {
    let mut r = Rig::new();
    // A field under everything.
    r.watch = vec![(Some(2), [0.0, 0.0, 1000.0, 1000.0])];
    r.down(1, VOLUME);
    assert_eq!(r.pointers.held(Hold::Volume), 1);
    r.down(2, QUIT);
    r.down(3, POLICY);
    assert_eq!(r.edges(), [e("cancel", true), e("policy", true)]);
    let landed = r.down(4, lane(0));
    assert!(landed.taken && !landed.holds && !landed.touch);
    assert_eq!(r.edges(), [e("watch:2", true)]);
    // Its lift is nobody's.
    assert!(!r.up(4, lane(0)));
    r.skip = Some([100.0, 550.0, 300.0, 650.0]);
    r.down(5, lane(0));
    assert_eq!(r.edges(), [e("skip", true)]);
    r.watch = vec![(None, [0.0, 0.0, 1000.0, 1000.0])];
    r.skip = None;
    r.down(6, lane(0));
    assert_eq!(r.edges(), [e("watch:all", true)]);
    // Only the primary press follows a field.
    let landed = r.press(7, lane(0), false, true);
    assert!(landed.holds);
    assert_eq!(r.edges(), [e("lane:0", true)]);
}

/// The time stamps are the shell's: each edge carries the sink's.
#[test]
fn edges_carry_the_shells_time_stamp() {
    let mut r = Rig::new();
    r.down(1, lane(0));
    let pressed = r.at;
    r.up(1, lane(0));
    let lifted = r.at;
    let times: Vec<f64> = r.inputs.iter().map(|i| i.host_time.0).collect();
    assert_eq!(times, [pressed, lifted]);
}
