//! `--pacing-report`: what a play's frames did under each pacing, the
//! two alternating in blocks of a few seconds in one play so they meet
//! the same song, the same display and the same machine; printed when
//! the play ends.

use super::{Mode, Pacer, Record};

/// Records this long (s) after a switch are left out: the other
/// mode's frames are still leaving the queue.
const SETTLE: f64 = 0.5;

/// The frames of one play, by pacing.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Switch every this many seconds (else the pacer's mode only).
    pub alternate: Option<f64>,
    start: Option<f64>,
    switched: f64,
    /// Each record, and whether the record before it is its frame's
    /// predecessor (same mode, none left out between).
    records: [Vec<(Record, bool)>; 2],
    last_kept: Option<Mode>,
    /// The OS's refresh rate (Hz).
    pub hint: Option<f64>,
}

fn slot(m: Mode) -> usize {
    match m {
        Mode::Block => 0,
        Mode::Late => 1,
    }
}

/// Mean, sd, percentiles and the largest of some values (ms).
struct Summary {
    mean: f64,
    sd: f64,
    p50: f64,
    p95: f64,
    max: f64,
}

fn summary(mut v: Vec<f64>) -> Summary {
    if v.is_empty() {
        return Summary {
            mean: f64::NAN,
            sd: f64::NAN,
            p50: f64::NAN,
            p95: f64::NAN,
            max: f64::NAN,
        };
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let mean = v.iter().sum::<f64>() / n as f64;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
    let at = |q: f64| v[((n - 1) as f64 * q).round() as usize];
    Summary {
        mean: mean * 1000.0,
        sd: var.sqrt() * 1000.0,
        p50: at(0.5) * 1000.0,
        p95: at(0.95) * 1000.0,
        max: v[n - 1] * 1000.0,
    }
}

/// One mode's figures.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Figures {
    pub frames: usize,
    pub seconds: f64,
    pub probes: usize,
    /// Present to present (ms): mean, sd, max.
    pub interval: [f64; 3],
    /// In the acquire (ms): mean, p95, max; and the share of the time.
    pub blocked: [f64; 3],
    pub blocked_share: f64,
    /// The present call (ms): mean, max.
    pub present: [f64; 2],
    /// Acquire's return to present (ms): mean, p95, max.
    pub build: [f64; 3],
    /// Late pacing's wake after its planned start (ms): p50, p95, max.
    pub late: [f64; 3],
    /// The clock and input read to the refresh the frame is shown at
    /// (ms, as the grid has it): mean, p50, p95.
    pub latency: [f64; 3],
    /// The frame's start (the keys and pads polled just before it) to
    /// the refresh it is shown at (ms): mean, p50, p95.
    pub polled: [f64; 3],
    /// Refreshes that showed the frame before again.
    pub repeats: u64,
    pub overruns: usize,
    pub early: usize,
    pub latch_misses: usize,
    /// Frames on a locked grid (share).
    pub locked: f64,
    /// At the end: the period (ms), whether it was measured, and the
    /// margin (ms).
    pub period: f64,
    pub measured: bool,
    pub margin: f64,
}

impl Report {
    /// A report that alternates the modes every `every` seconds.
    pub fn alternating(every: f64) -> Report {
        Report {
            alternate: Some(every),
            ..Report::default()
        }
    }

    /// A report like this one (alternating as it does, the same rate)
    /// with nothing recorded: the next play's.
    pub fn fresh(&self) -> Report {
        Report {
            alternate: self.alternate,
            hint: self.hint,
            ..Report::default()
        }
    }

    /// The mode a frame starting at `now` runs in (`current` when not
    /// alternating). Block first.
    pub fn mode_at(&mut self, now: f64, current: Mode) -> Mode {
        let start = *self.start.get_or_insert(now);
        match self.alternate {
            Some(e) if e > 0.0 => {
                if ((now - start) / e).floor() as i64 % 2 == 0 {
                    Mode::Block
                } else {
                    Mode::Late
                }
            }
            _ => current,
        }
    }

    /// The mode changed at `now`.
    pub fn switched(&mut self, now: f64) {
        self.switched = now;
        self.last_kept = None;
    }

    /// Before a frame is asked for at `now`: `pacer` set to the mode
    /// this time runs in. Whether it changed.
    pub fn choose(&mut self, now: f64, pacer: &mut Pacer) -> bool {
        let mode = self.mode_at(now, pacer.mode());
        if mode == pacer.mode() {
            return false;
        }
        pacer.set_mode(mode);
        self.switched(now);
        true
    }

    /// A frame done.
    pub fn push(&mut self, r: Record) {
        self.start.get_or_insert(r.begin);
        if self.alternate.is_some() && r.begin - self.switched < SETTLE {
            self.last_kept = None;
            return;
        }
        let follows = self.last_kept == Some(r.mode);
        self.records[slot(r.mode)].push((r, follows));
        self.last_kept = Some(r.mode);
    }

    /// Frames kept.
    pub fn len(&self) -> usize {
        self.records[0].len() + self.records[1].len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// One mode's figures.
    pub fn figures(&self, mode: Mode) -> Figures {
        let rs = &self.records[slot(mode)];
        let mut f = Figures {
            frames: rs.len(),
            ..Figures::default()
        };
        if rs.is_empty() {
            return f;
        }
        let mut intervals = Vec::new();
        let mut repeats = 0;
        for w in rs.windows(2) {
            let ((a, _), (b, follows)) = (w[0], w[1]);
            if !follows {
                continue;
            }
            intervals.push(b.end - a.end);
            let gap = ((b.target - a.target) / b.period).round();
            repeats += (gap - 1.0).max(0.0) as u64;
        }
        f.seconds = intervals.iter().sum();
        let s = summary(intervals);
        f.interval = [s.mean, s.sd, s.max];
        let blocked: Vec<f64> = rs.iter().map(|(r, _)| r.blocked).collect();
        let total: f64 = rs
            .iter()
            .filter(|(_, follows)| *follows)
            .map(|(r, _)| r.blocked)
            .sum();
        f.blocked_share = if f.seconds > 0.0 {
            total / f.seconds
        } else {
            0.0
        };
        let s = summary(blocked);
        f.blocked = [s.mean, s.p95, s.max];
        let s = summary(rs.iter().map(|(r, _)| r.present).collect());
        f.present = [s.mean, s.max];
        let s = summary(rs.iter().map(|(r, _)| r.build).collect());
        f.build = [s.mean, s.p95, s.max];
        let s = summary(rs.iter().filter_map(|(r, _)| r.late).collect());
        f.late = [s.p50, s.p95, s.max];
        let s = summary(rs.iter().map(|(r, _)| r.target - r.sample).collect());
        f.latency = [s.mean, s.p50, s.p95];
        let s = summary(rs.iter().map(|(r, _)| r.target - r.begin).collect());
        f.polled = [s.mean, s.p50, s.p95];
        f.repeats = repeats;
        f.probes = rs.iter().filter(|(r, _)| r.probe).count();
        f.overruns = rs.iter().filter(|(r, _)| r.overrun).count();
        f.early = rs.iter().filter(|(r, _)| r.early).count();
        f.latch_misses = rs.iter().filter(|(r, _)| r.latch_miss).count();
        f.locked = rs.iter().filter(|(r, _)| r.locked).count() as f64 / rs.len() as f64;
        let last = rs[rs.len() - 1].0;
        f.period = last.period * 1000.0;
        f.measured = last.measured;
        f.margin = last.margin * 1000.0;
        f
    }

    /// The report as text: a line a figure, a column a mode.
    pub fn text(&self, title: &str) -> String {
        let b = self.figures(Mode::Block);
        let l = self.figures(Mode::Late);
        let mut out = format!(
            "pacing report: {title}{}\n",
            self.hint
                .map_or(String::new(), |h| format!(" · the OS says {h:.2} Hz"))
        );
        out.push_str(&format!("  {:<38} {:>22} {:>22}\n", "", "block", "late"));
        // A mode with no frames shows dashes.
        let (none_b, none_l) = (b.frames == 0, l.frames == 0);
        let row = |out: &mut String, name: &str, a: String, c: String| {
            let a = if none_b { "-".into() } else { a };
            let c = if none_l { "-".into() } else { c };
            out.push_str(&format!("  {name:<38} {a:>22} {c:>22}\n"));
        };
        let ms3 = |v: [f64; 3]| format!("{:.2} / {:.2} / {:.2}", v[0], v[1], v[2]);
        out.push_str(&format!(
            "  {:<38} {:>22} {:>22}\n",
            "frames (seconds)",
            format!("{} ({:.1})", b.frames, b.seconds),
            format!("{} ({:.1})", l.frames, l.seconds),
        ));
        let source = |f: &Figures| {
            if f.measured {
                "measured"
            } else if self.hint.is_some() {
                "OS"
            } else {
                "assumed"
            }
        };
        row(
            &mut out,
            "refresh (Hz, where from)",
            format!("{:.2} {}", 1000.0 / b.period, source(&b)),
            format!("{:.2} {}", 1000.0 / l.period, source(&l)),
        );
        row(
            &mut out,
            "on a locked grid",
            format!("{:.0} %", b.locked * 100.0),
            format!("{:.0} %", l.locked * 100.0),
        );
        row(
            &mut out,
            "present to present ms (mean/sd/max)",
            ms3(b.interval),
            ms3(l.interval),
        );
        row(
            &mut out,
            "blocked in acquire ms (mean/p95/max)",
            ms3(b.blocked),
            ms3(l.blocked),
        );
        row(
            &mut out,
            "time blocked",
            format!("{:.0} %", b.blocked_share * 100.0),
            format!("{:.0} %", l.blocked_share * 100.0),
        );
        row(
            &mut out,
            "present call ms (mean / max)",
            format!("{:.2} / {:.2}", b.present[0], b.present[1]),
            format!("{:.2} / {:.2}", l.present[0], l.present[1]),
        );
        row(
            &mut out,
            "build ms (mean / p95 / max)",
            ms3(b.build),
            ms3(l.build),
        );
        row(
            &mut out,
            "woke late ms (p50 / p95 / max)",
            "-".into(),
            ms3(l.late),
        );
        row(
            &mut out,
            "input read → shown ms (mean/p50/p95)",
            ms3(b.latency),
            ms3(l.latency),
        );
        row(
            &mut out,
            "frame start → shown ms (mean/p50/p95)",
            ms3(b.polled),
            ms3(l.polled),
        );
        row(
            &mut out,
            "repeated refreshes",
            b.repeats.to_string(),
            l.repeats.to_string(),
        );
        row(
            &mut out,
            "overran · early · missed refreshes",
            format!("{} · {} · {}", b.overruns, b.early, b.latch_misses),
            format!("{} · {} · {}", l.overruns, l.early, l.latch_misses),
        );
        row(
            &mut out,
            "probes · margin ms",
            "-".into(),
            format!("{} · {:.2}", l.probes, l.margin),
        );
        out.push_str(
            "  notes: \"shown\" is the refresh the grid gives the frame (no present timestamps).\n\
             \x20        Keys and pads are polled just before a frame starts, and a blocking\n\
             \x20        frame then waits in the acquire: \"frame start → shown\" is closer to\n\
             \x20        what a press waits for than \"input read → shown\", which flatters block.\n\
             \x20        Probes and bursts (late) start right after the frame before.\n",
        );
        out
    }
}
