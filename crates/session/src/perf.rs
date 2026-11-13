//! Parts of a frame timed for a shell's frame recorder (the web's
//! `perf=1`): a shell that records sets a clock ([`set_clock`]) and takes
//! each part's milliseconds after its frame ([`take`]). Without a clock
//! (the default) a timed part costs one thread-local read.

use std::cell::Cell;

/// A part of the session's work a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// A stage's battle judged to the frame: its CPUs, the fields' feeds,
    /// the attack ledger, a live stage's streams (`Battle::advance`).
    Battle,
}

const PARTS: usize = 1;

thread_local! {
    static CLOCK: Cell<Option<fn() -> f64>> = const { Cell::new(None) };
    static SPENT: Cell<[f64; PARTS]> = const { Cell::new([0.0; PARTS]) };
}

/// The clock (milliseconds) parts are timed by; `None` stops timing.
pub fn set_clock(clock: Option<fn() -> f64>) {
    CLOCK.set(clock);
    SPENT.set([0.0; PARTS]);
}

/// The milliseconds spent in `part` since the last take.
pub fn take(part: Part) -> f64 {
    let mut spent = SPENT.get();
    let v = std::mem::take(&mut spent[part as usize]);
    SPENT.set(spent);
    v
}

/// Runs `f`, its time counted to `part` where a clock is set.
pub(crate) fn timed<R>(part: Part, f: impl FnOnce() -> R) -> R {
    let Some(clock) = CLOCK.get() else {
        return f();
    };
    let start = clock();
    let r = f();
    let mut spent = SPENT.get();
    spent[part as usize] += clock() - start;
    SPENT.set(spent);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static NOW: Cell<f64> = const { Cell::new(0.0) };
    }

    fn now() -> f64 {
        NOW.get()
    }

    #[test]
    fn a_part_is_timed_only_with_a_clock() {
        assert_eq!(timed(Part::Battle, || 7), 7);
        assert_eq!(take(Part::Battle), 0.0);
        set_clock(Some(now));
        timed(Part::Battle, || NOW.set(2.5));
        timed(Part::Battle, || NOW.set(3.0));
        assert_eq!(take(Part::Battle), 3.0);
        assert_eq!(take(Part::Battle), 0.0);
        set_clock(None);
    }
}
