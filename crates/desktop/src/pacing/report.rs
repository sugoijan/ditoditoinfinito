//! `--pacing-report`: what a play's frames did under each pacing and
//! number of frames in flight, the columns alternating in blocks of a few
//! seconds in one play so they meet the same song, the same display and
//! the same machine; printed when the play ends.

use super::{Mode, Pacer, Record, Source};

/// Records this long (s) after a switch are left out: the other
/// column's frames are still leaving the queue.
const SETTLE: f64 = 0.5;

/// A way of running a play's frames: the pacing and the frames in
/// flight (the swapchain's drawables less one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    pub mode: Mode,
    pub in_flight: u32,
}

impl Column {
    pub const fn new(mode: Mode, in_flight: u32) -> Column {
        Column { mode, in_flight }
    }

    /// `block·1`, `late·2`.
    pub fn name(self) -> String {
        format!("{}·{}", self.mode.name(), self.in_flight)
    }

    fn of(r: &Record) -> Column {
        Column::new(r.mode, r.in_flight)
    }
}

/// What `--pacing-report` compares: blocking frames as play runs them
/// (one in flight) and with two, late pacing with one and with two.
pub const COLUMNS: [Column; 4] = [
    Column::new(Mode::Block, 1),
    Column::new(Mode::Block, 2),
    Column::new(Mode::Late, 1),
    Column::new(Mode::Late, 2),
];

/// The frames of one play, by column.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Switch every this many seconds (else the pacer's own column).
    pub alternate: Option<f64>,
    /// The columns alternated, in turn.
    pub columns: Vec<Column>,
    start: Option<f64>,
    switched: f64,
    /// Each record, and whether the record before it is its frame's
    /// predecessor (same column, none left out between).
    records: Vec<(Record, bool)>,
    last_kept: Option<Column>,
    /// The OS's refresh rate (Hz).
    pub hint: Option<f64>,
    /// The display link the shell runs, if any (`CADisplayLink`, …).
    pub link: Option<&'static str>,
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

/// One column's figures.
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
    /// The clock and input read to the inferred refresh target
    /// (ms): mean, p50, p95. Not observed presentation latency.
    pub latency: [f64; 3],
    /// The frame's start (the keys and pads polled just before it) to
    /// the inferred refresh target (ms): mean, p50, p95.
    pub polled: [f64; 3],
    /// Refresh slots skipped between inferred targets, not observed repeats.
    pub target_gaps: u64,
    /// Consecutive frames assigned to the same or an earlier target.
    /// Boundary jitter can pair these with gaps at a steady present cadence.
    pub target_collisions: usize,
    pub overruns: usize,
    pub early: usize,
    pub latch_misses: usize,
    /// Frames late pacing ran as blocking frames, having fallen back
    /// (share), and when it first did (s into the column's frames).
    pub fallback: f64,
    pub fallback_after: Option<f64>,
    /// The missed frames' time to spare at their present and how late
    /// they woke (ms, medians).
    pub miss_spare: f64,
    pub miss_late: f64,
    /// Frames on a locked grid (share).
    pub locked: f64,
    /// Frames whose refresh times came from a display link, from the
    /// acquires' returns (shares).
    pub from_link: f64,
    pub from_returns: f64,
    /// The display link's refresh period over the column (ms): mean, sd.
    pub link_period: [f64; 2],
    /// At the end: the period (ms), whether it was measured, and the
    /// margin (ms).
    pub period: f64,
    pub measured: bool,
    pub margin: f64,
}

impl Report {
    /// A report that runs `columns` in turn, `every` seconds each.
    pub fn alternating(every: f64, columns: &[Column]) -> Report {
        Report {
            alternate: Some(every),
            columns: columns.to_vec(),
            ..Report::default()
        }
    }

    /// A report like this one (alternating as it does, the same rate and
    /// link) with nothing recorded: the next play's.
    pub fn fresh(&self) -> Report {
        Report {
            alternate: self.alternate,
            columns: self.columns.clone(),
            hint: self.hint,
            link: self.link,
            ..Report::default()
        }
    }

    /// The column a frame starting at `now` runs in (`current` when not
    /// alternating).
    /// The turns count from the first frame presented (a window that
    /// is not visible yet presents none), the first column until then.
    pub fn column_at(&mut self, now: f64, current: Column) -> Column {
        match self.alternate {
            Some(e) if e > 0.0 && !self.columns.is_empty() => {
                let Some(start) = self.start else {
                    return self.columns[0];
                };
                let k = ((now - start).max(0.0) / e).floor() as usize;
                self.columns[k % self.columns.len()]
            }
            _ => current,
        }
    }

    /// The column changed at `now`.
    pub fn switched(&mut self, now: f64) {
        self.switched = now;
        self.last_kept = None;
    }

    /// Before a frame is asked for at `now`: `pacer` set to the column
    /// this time runs in. Whether it changed (the shell then configures
    /// the surface for its frames in flight).
    pub fn choose(&mut self, now: f64, pacer: &mut Pacer) -> bool {
        let current = Column::new(pacer.mode(), pacer.in_flight());
        let column = self.column_at(now, current);
        if column == current {
            return false;
        }
        pacer.set_mode(column.mode);
        pacer.set_in_flight(column.in_flight);
        self.switched(now);
        true
    }

    /// A frame done.
    pub fn push(&mut self, r: Record) {
        // The first frame presented starts the turns, and settles like a
        // switch (a window just shown, its first frames' pipelines).
        if self.start.is_none() {
            self.start = Some(r.begin);
            self.switched = r.begin;
            self.last_kept = None;
        }
        if self.alternate.is_some() && r.begin - self.switched < SETTLE {
            self.last_kept = None;
            return;
        }
        let c = Column::of(&r);
        let follows = self.last_kept == Some(c);
        self.records.push((r, follows));
        self.last_kept = Some(c);
    }

    /// Frames kept.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// When frames said a refresh was missed (host s).
    pub fn misses(&self) -> Vec<f64> {
        self.records
            .iter()
            .filter(|(r, _)| r.latch_miss)
            .map(|(r, _)| r.begin)
            .collect()
    }

    /// The columns shown: the report's, then any other with frames.
    pub fn shown(&self) -> Vec<Column> {
        let mut cs: Vec<Column> = self.columns.clone();
        for (r, _) in &self.records {
            let c = Column::of(r);
            if !cs.contains(&c) {
                cs.push(c);
            }
        }
        cs
    }

    /// One column's figures.
    pub fn figures(&self, column: Column) -> Figures {
        let rs: Vec<(Record, bool)> = self
            .records
            .iter()
            .filter(|(r, _)| Column::of(r) == column)
            .copied()
            .collect();
        let mut f = Figures {
            frames: rs.len(),
            ..Figures::default()
        };
        if rs.is_empty() {
            return f;
        }
        let mut intervals = Vec::new();
        let mut target_gaps = 0;
        let mut target_collisions = 0;
        for w in rs.windows(2) {
            let ((a, _), (b, follows)) = (w[0], w[1]);
            if !follows {
                continue;
            }
            intervals.push(b.end - a.end);
            let gap = ((b.target - a.target) / b.period).round();
            target_gaps += (gap - 1.0).max(0.0) as u64;
            target_collisions += usize::from(gap <= 0.0);
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
        f.target_gaps = target_gaps;
        f.target_collisions = target_collisions;
        f.probes = rs.iter().filter(|(r, _)| r.probe).count();
        f.overruns = rs.iter().filter(|(r, _)| r.overrun).count();
        f.early = rs.iter().filter(|(r, _)| r.early).count();
        f.latch_misses = rs.iter().filter(|(r, _)| r.latch_miss).count();
        f.fallback = rs.iter().filter(|(r, _)| r.fallback).count() as f64 / rs.len() as f64;
        // Counted in the column's own time (its turns end to end).
        let mut at = 0.0;
        if rs[0].0.fallback {
            f.fallback_after = Some(0.0);
        }
        for w in rs.windows(2) {
            let ((a, _), (b, follows)) = (w[0], w[1]);
            if f.fallback_after.is_some() {
                break;
            }
            if follows {
                at += b.end - a.end;
            }
            if b.fallback {
                f.fallback_after = Some(at);
            }
        }
        f.miss_spare = summary(rs.iter().filter_map(|(r, _)| r.miss_spare).collect()).p50;
        f.miss_late = summary(rs.iter().filter_map(|(r, _)| r.miss_late).collect()).p50;
        let n = rs.len() as f64;
        let share = |s: Source| rs.iter().filter(|(r, _)| r.source == s).count() as f64 / n;
        f.locked = rs.iter().filter(|(r, _)| r.locked).count() as f64 / n;
        f.from_link = share(Source::Link);
        f.from_returns = share(Source::Returns);
        let s = summary(
            rs.iter()
                .filter(|(r, _)| r.source == Source::Link)
                .map(|(r, _)| r.period)
                .collect(),
        );
        f.link_period = [s.mean, s.sd];
        let last = rs[rs.len() - 1].0;
        f.period = last.period * 1000.0;
        f.measured = last.measured;
        f.margin = last.margin * 1000.0;
        f
    }

    /// The report as text: a line a figure, a column a column.
    pub fn text(&self, title: &str) -> String {
        let columns = self.shown();
        let figures: Vec<Figures> = columns.iter().map(|c| self.figures(*c)).collect();
        let mut out = format!(
            "pacing report: {title}{} · display link: {}\n",
            self.hint
                .map_or(String::new(), |h| format!(" · the OS says {h:.2} Hz")),
            self.link.unwrap_or("no"),
        );
        let head = |out: &mut String, name: &str, cells: Vec<String>| {
            out.push_str(&format!("  {name:<46}"));
            for c in cells {
                out.push_str(&format!(" {c:>22}"));
            }
            out.push('\n');
        };
        // A column with no frames shows dashes.
        let row = |out: &mut String, name: &str, cell: &dyn Fn(&Figures) -> String| {
            let cells = figures
                .iter()
                .map(|f| if f.frames == 0 { "-".into() } else { cell(f) })
                .collect();
            head(out, name, cells);
        };
        let ms3 = |v: [f64; 3]| format!("{:.2} / {:.2} / {:.2}", v[0], v[1], v[2]);
        head(&mut out, "", columns.iter().map(|c| c.name()).collect());
        head(
            &mut out,
            "frames (seconds)",
            figures
                .iter()
                .map(|f| format!("{} ({:.1})", f.frames, f.seconds))
                .collect(),
        );
        let hint = self.hint.is_some();
        row(&mut out, "refresh (Hz, where from)", &|f| {
            let from = if f.from_link > 0.5 {
                "link"
            } else if f.measured {
                "measured"
            } else if hint {
                "OS"
            } else {
                "assumed"
            };
            format!("{:.2} {from}", 1000.0 / f.period)
        });
        row(&mut out, "refresh times: link · returns", &|f| {
            format!(
                "{:.0} % · {:.0} %",
                f.from_link * 100.0,
                f.from_returns * 100.0
            )
        });
        row(&mut out, "display link period ms (mean / sd)", &|f| {
            if f.from_link > 0.0 {
                format!("{:.3} / {:.3}", f.link_period[0], f.link_period[1])
            } else {
                "-".into()
            }
        });
        row(&mut out, "on a locked grid", &|f| {
            format!("{:.0} %", f.locked * 100.0)
        });
        row(&mut out, "present to present ms (mean/sd/max)", &|f| {
            ms3(f.interval)
        });
        row(&mut out, "blocked in acquire ms (mean/p95/max)", &|f| {
            ms3(f.blocked)
        });
        row(&mut out, "time blocked", &|f| {
            format!("{:.0} %", f.blocked_share * 100.0)
        });
        row(&mut out, "present call ms (mean / max)", &|f| {
            format!("{:.2} / {:.2}", f.present[0], f.present[1])
        });
        row(&mut out, "build ms (mean / p95 / max)", &|f| ms3(f.build));
        row(&mut out, "woke late ms (p50 / p95 / max)", &|f| {
            if f.late[0].is_nan() {
                "-".into()
            } else {
                ms3(f.late)
            }
        });
        row(
            &mut out,
            "input read → estimated shown ms (mean/p50/p95)",
            &|f| ms3(f.latency),
        );
        row(
            &mut out,
            "frame start → estimated shown ms (mean/p50/p95)",
            &|f| ms3(f.polled),
        );
        row(&mut out, "inferred target gaps · collisions", &|f| {
            format!("{} · {}", f.target_gaps, f.target_collisions)
        });
        head(
            &mut out,
            "target later · earlier · acquire misses",
            columns
                .iter()
                .zip(&figures)
                .map(|(c, f)| {
                    if f.frames == 0 {
                        return "-".into();
                    }
                    let misses = if c.mode == Mode::Late {
                        f.latch_misses.to_string()
                    } else {
                        "-".into()
                    };
                    format!("{} · {} · {misses}", f.overruns, f.early)
                })
                .collect(),
        );
        head(
            &mut out,
            "late fell back to block (frames · after s)",
            columns
                .iter()
                .zip(&figures)
                .map(|(c, f)| {
                    if f.frames == 0 || c.mode != Mode::Late {
                        "-".into()
                    } else if let Some(at) = f.fallback_after {
                        format!("{:.0} % · {at:.1}", f.fallback * 100.0)
                    } else {
                        "no".into()
                    }
                })
                .collect(),
        );
        row(&mut out, "missed: spare · woke late ms (p50)", &|f| {
            if f.latch_misses == 0 || f.miss_spare.is_nan() {
                "-".into()
            } else {
                format!("{:.2} · {:.2}", f.miss_spare, f.miss_late)
            }
        });
        row(&mut out, "probes · margin ms", &|f| {
            format!("{} · {:.2}", f.probes, f.margin)
        });
        out.push_str(
            "  notes: \"shown\" is the refresh the grid gives the frame (no present timestamps):\n\
             \x20        bounded by the configured queue depth; block assumes a full queue.\n\
             \x20        Display-link ticks locate refreshes, not individual frame presentations.\n\
             \x20        Target gaps are not observed repeats: boundary jitter can pair gaps\n\
             \x20        with collisions (two frames assigned the same or an earlier refresh).\n\
             \x20        Later/earlier compare targets before/after building; acquire misses\n\
             \x20        are inferred from late pacing's acquire waits, unavailable for block.\n\
             \x20        A late column that fell back blocked from then on, as many in flight\n\
             \x20        (its figures mix both): misses no margin could answer kept coming.\n\
             \x20        Keys and pads are polled just before a frame starts, and a blocking\n\
             \x20        frame then waits in the acquire: \"frame start → shown\" is closer to\n\
             \x20        what a press waits for than \"input read → shown\", which flatters block.\n\
             \x20        Probes and bursts (late, no display link) start right after the frame before.\n",
        );
        out
    }
}
