//! Chart ratings (MSD) made in the background on desktop
//! (`ddi_ui::state::ratings`, `docs/plans/msd-ratings.md`): one song at a
//! time, read and rated on a thread of its own (its chart as a play reads
//! it, `crate::play::load::read_chart`), only while an option wants them,
//! and none started while a play runs. Driven from the event loop beside
//! the kept plays' reworker ([`Ratings::poll`]); the thread wakes the loop
//! when a song is done.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use ddi_ui::state::ratings::{self, Job, Rated};

/// A song's id and its charts' numbers, from the thread.
type Answer = (String, Vec<Rated>);

/// The thread and its answers, started with the first song to rate.
#[derive(Default)]
pub struct Ratings {
    worker: Option<(Sender<Job>, Receiver<Answer>)>,
}

impl Ratings {
    /// Takes the thread's answers, and hands it the next song where one
    /// waits (none while `playing`).
    pub fn poll(&mut self, playing: bool, wake: Option<&Arc<dyn Fn() + Send + Sync>>) {
        ratings::set_playing(playing);
        if let Some((_, rx)) = &self.worker {
            while let Ok((song, rated)) = rx.try_recv() {
                ratings::done(&song, rated);
            }
        }
        let Some(job) = ratings::next() else {
            return;
        };
        let (tx, _) = self.worker.get_or_insert_with(|| spawn(wake.cloned()));
        if let Err(e) = tx.send(job) {
            // The thread is gone: nothing more is rated this run.
            ratings::done(&e.0.song, Vec::new());
            self.worker = None;
        }
    }
}

fn spawn(
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) -> (Sender<Job>, Receiver<Answer>) {
    let (jobs, take) = channel::<Job>();
    let (give, answers) = channel();
    let spawned = std::thread::Builder::new()
        .name("ddi-ratings".into())
        .spawn(move || {
            for job in take {
                // A chart that cannot be read or panics is no rating.
                let rated = std::panic::catch_unwind(|| {
                    crate::play::load::read_chart(&job.song)
                        .map(|song| ratings::rate_song(&song, &job.charts))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
                if give.send((job.song, rated)).is_err() {
                    return;
                }
                if let Some(w) = &wake {
                    w();
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("ratings: no thread ({e}); charts keep their songs' numbers");
    }
    (jobs, answers)
}
