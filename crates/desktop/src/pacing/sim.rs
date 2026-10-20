//! A display and its swapchain on a fake clock: the pacing's harness
//! (built for tests and the `sim` feature only).
//! Fifo with a few drawables: a frame presented in time for a refresh
//! (its `latch` before it) is shown at it, the drawable of the frame it
//! replaces released `release` after; an acquire with no drawable free
//! waits for one. The display refreshes on a fixed grid, or (a variable
//! refresh rate) when a frame comes, no sooner than its shortest period
//! after the last refresh and no later than its longest.
//!
//! [`run`] drives a [`Pacer`] the way the shell's loop does (ask when
//! the next frame starts and sleep until then, acquire, read the clock,
//! build, present) with given costs and timer lateness, and says when
//! each frame was really shown.

use std::collections::VecDeque;

use super::{Next, Pacer, Record, Report};

/// How the display refreshes.
#[derive(Clone, Copy, Debug)]
pub enum Display {
    /// Every `period` seconds from `phase`.
    Fixed { period: f64, phase: f64 },
    /// When a frame comes, between `min` and `max` seconds apart.
    Vrr { min: f64, max: f64 },
}

/// A display with a Fifo swapchain.
#[derive(Clone, Debug)]
pub struct Surface {
    pub display: Display,
    /// Drawables (one frame in flight: 2).
    pub images: usize,
    /// A present must come this long before a refresh to be shown at it
    /// (a compositor's deadline, the GPU's work).
    pub latch: f64,
    /// A replaced frame's drawable is released this long after the
    /// refresh, give or take `release_sd` (a normal spread).
    pub release: f64,
    pub release_sd: f64,
    /// The share of refreshes whose deadline is `hiccup_latch` instead
    /// (a compositor's stall).
    pub hiccups: f64,
    pub hiccup_latch: f64,
    noise: Noise,
    queue: VecDeque<(usize, f64)>,
    shown: Option<usize>,
    held: bool,
    releases: Vec<f64>,
    last_refresh: f64,
    count: u64,
    /// When each frame was shown.
    pub glass: Vec<Option<f64>>,
    /// Each refresh, and the frame it newly showed.
    pub refreshes: Vec<(f64, Option<usize>)>,
}

impl Surface {
    pub fn new(display: Display) -> Surface {
        let last_refresh = match display {
            Display::Fixed { phase, period } => phase - period,
            Display::Vrr { .. } => 0.0,
        };
        Surface {
            display,
            images: 2,
            latch: 0.0,
            release: 0.000_1,
            release_sd: 0.0,
            hiccups: 0.0,
            hiccup_latch: 0.0,
            noise: Noise::new(0x5eed),
            queue: VecDeque::new(),
            shown: None,
            held: false,
            releases: Vec::new(),
            last_refresh,
            count: 0,
            glass: Vec::new(),
            refreshes: Vec::new(),
        }
    }

    fn next_refresh(&self) -> f64 {
        match self.display {
            Display::Fixed { period, phase } => phase + self.count as f64 * period,
            Display::Vrr { min, max } => match self.queue.front() {
                Some(&(_, t)) => (t + self.latch)
                    .max(self.last_refresh + min)
                    .min(self.last_refresh + max),
                None => self.last_refresh + max,
            },
        }
    }

    fn refresh(&mut self, g: f64) {
        let mut newly = None;
        let latch = if self.hiccups > 0.0 && self.noise.uniform() < self.hiccups {
            self.hiccup_latch
        } else {
            self.latch
        };
        if let Some(&(id, t)) = self.queue.front()
            && t <= g - latch + 1e-12
        {
            self.queue.pop_front();
            if self.shown.replace(id).is_some() {
                let spread = if self.release_sd > 0.0 {
                    self.release_sd * self.noise.normal()
                } else {
                    0.0
                };
                self.releases.push(g + (self.release + spread).max(0.0));
            }
            if self.glass.len() <= id {
                self.glass.resize(id + 1, None);
            }
            self.glass[id] = Some(g);
            newly = Some(id);
        }
        self.refreshes.push((g, newly));
        self.last_refresh = g;
        self.count += 1;
    }

    /// Refreshes up to `t`.
    pub fn advance(&mut self, t: f64) {
        loop {
            let g = self.next_refresh();
            if g > t {
                break;
            }
            self.refresh(g);
        }
        self.releases.retain(|r| *r > t);
    }

    fn free(&self) -> usize {
        self.images.saturating_sub(
            usize::from(self.shown.is_some())
                + self.queue.len()
                + self.releases.len()
                + usize::from(self.held),
        )
    }

    /// An acquire at `t`: when it returns.
    pub fn acquire(&mut self, t: f64) -> f64 {
        let mut now = t;
        loop {
            self.advance(now);
            if self.free() > 0 {
                self.held = true;
                return now;
            }
            let g = self.next_refresh();
            now = match self.releases.iter().copied().reduce(f64::min) {
                Some(r) if r <= g => r,
                _ => g,
            };
        }
    }

    /// Frame `id` presented at `t`.
    pub fn present(&mut self, t: f64, id: usize) {
        self.advance(t);
        self.held = false;
        self.queue.push_back((id, t));
    }

    /// A fixed display changes its period (another rate, the same
    /// display): the next refresh comes `period` after the last.
    pub fn set_period(&mut self, period: f64) {
        self.display = Display::Fixed {
            period,
            phase: self.last_refresh + period - self.count as f64 * period,
        };
    }

    /// The display link's last refresh at or before `t` and the next
    /// (a fixed display only).
    pub fn link_tick(&self, t: f64) -> Option<(f64, f64)> {
        let Display::Fixed { period, phase } = self.display else {
            return None;
        };
        let k = ((t - phase) / period).floor();
        let at = phase + k * period;
        Some((at, at + period))
    }

    /// Runs on until everything presented is shown.
    pub fn finish(&mut self) {
        while let Some(&(_, t)) = self.queue.back() {
            let g = self.next_refresh().max(t);
            self.advance(g + 1.0);
        }
    }
}

/// A frame of a simulated run.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub record: Record,
    /// When it was really shown.
    pub glass: Option<f64>,
}

/// What the loop around the pacer spends besides the frame's build:
/// the UI's update before the acquire, the present call, the loop's turn.
const BEFORE: f64 = 0.000_2;
const PRESENT: f64 = 0.000_1;
const TURN: f64 = 0.000_05;

/// Runs `frames` frames through `pacer` on `surface`: `cost(i)` is frame
/// `i`'s build, `late(i)` how late its timer wakes the loop; `report`
/// (alternating or not) gets every frame, as the shell's does.
pub fn run(
    pacer: &mut Pacer,
    surface: &mut Surface,
    frames: usize,
    mut cost: impl FnMut(usize) -> f64,
    mut late: impl FnMut(usize) -> f64,
    mut report: Option<&mut Report>,
) -> Vec<Frame> {
    let mut l = Loop::default();
    for _ in 0..frames {
        l.step(pacer, surface, &mut cost, &mut late, report.as_deref_mut());
    }
    l.finish(surface)
}

/// The shell's loop, a frame at a time (for runs that change the display
/// on the way).
#[derive(Clone, Debug, Default)]
pub struct Loop {
    /// The fake clock (s).
    pub t: f64,
    pub records: Vec<Record>,
    /// A display link gives the pacer the refreshes (a fixed display).
    pub link: bool,
}

impl Loop {
    /// One frame.
    pub fn step(
        &mut self,
        pacer: &mut Pacer,
        surface: &mut Surface,
        cost: &mut impl FnMut(usize) -> f64,
        late: &mut impl FnMut(usize) -> f64,
        report: Option<&mut Report>,
    ) {
        let i = self.records.len();
        let mut report = report;
        if let Some(r) = report.as_deref_mut()
            && r.choose(self.t, pacer)
        {
            // The surface configured for the column's frames in flight.
            surface.images = pacer.in_flight() as usize + 1;
        }
        self.tick(pacer, surface);
        if let Next::At(w) = pacer.next(self.t) {
            self.t = self.t.max(w + late(i));
        }
        self.tick(pacer, surface);
        pacer.begin(self.t);
        self.t += BEFORE;
        let a = self.t;
        self.t = surface.acquire(self.t);
        pacer.acquired(a, self.t);
        let sample = self.t;
        self.t += cost(i);
        let p = self.t;
        surface.present(self.t, i);
        self.t += PRESENT;
        let record = pacer.presented(sample, p, self.t);
        if let Some(r) = report {
            r.push(record);
        }
        self.records.push(record);
        self.t += TURN;
    }

    /// The display link's news, as the shell reads it before asking for
    /// a frame and before starting one.
    fn tick(&self, pacer: &mut Pacer, surface: &Surface) {
        if self.link
            && let Some((at, target)) = surface.link_tick(self.t)
        {
            pacer.refresh(at, target);
        }
    }

    /// Frames until the clock reaches `end`.
    pub fn until(
        &mut self,
        end: f64,
        pacer: &mut Pacer,
        surface: &mut Surface,
        cost: &mut impl FnMut(usize) -> f64,
        late: &mut impl FnMut(usize) -> f64,
    ) {
        while self.t < end {
            self.step(pacer, surface, cost, late, None);
        }
    }

    /// The run's frames, with when each was shown.
    pub fn finish(self, surface: &mut Surface) -> Vec<Frame> {
        surface.finish();
        self.records
            .into_iter()
            .enumerate()
            .map(|(i, record)| Frame {
                record,
                glass: surface.glass.get(i).copied().flatten(),
            })
            .collect()
    }
}

/// A small deterministic noise source (xorshift).
#[derive(Clone, Debug)]
pub struct Noise(u64);

impl Noise {
    pub fn new(seed: u64) -> Noise {
        Noise(seed.max(1))
    }

    /// Uniform in [0, 1).
    pub fn uniform(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Roughly normal, mean 0, sd 1 (a sum of uniforms).
    pub fn normal(&mut self) -> f64 {
        (0..12).map(|_| self.uniform()).sum::<f64>() - 6.0
    }
}
