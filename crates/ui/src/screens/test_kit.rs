//! What the screens' tests share: the plain environment, the harness at
//! the design's size, a few readings of it, and the room's test shell.

use std::cell::RefCell;
use std::rc::Rc;

use super::data::SongEntry;
use super::room::{RoomCall, RoomShell};
use super::{DESIGN, Env};
use crate::look::Plain;
use crate::state::room::{SharedRoom, StageOrder, UploadJob};
use crate::testing::Harness;
use crate::TextMode;

/// The screens' environment in the plain look, searching as the game
/// folds text.
pub(crate) fn env() -> Env {
    Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    )
}

/// A harness at the design's 1280×720, text measured by the UI itself.
pub(crate) fn harness() -> Harness {
    Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own)
}

/// The top layer's name (none: empty).
pub(crate) fn top(h: &Harness) -> String {
    h.ui.layers().last().map(|l| l.name.clone()).unwrap_or_default()
}

/// Whether an actor of `name` is drawn.
pub(crate) fn there(h: &Harness, name: &str) -> bool {
    h.ui.actor(name).is_some()
}

/// What the room's test shell was asked: the stages' prompts it opened.
pub(crate) type Orders = Rc<RefCell<Vec<StageOrder>>>;
/// The uploads it was handed.
pub(crate) type Uploads = Rc<RefCell<Vec<UploadJob>>>;

/// The room's shell for tests: songs prepared at once, prompts recorded.
pub(crate) fn shell(room: &SharedRoom, orders: &Orders) -> RoomShell {
    shell_with(room, orders, &Uploads::default())
}

/// [`shell`], its uploads kept in `uploads`.
pub(crate) fn shell_with(room: &SharedRoom, orders: &Orders, uploads: &Uploads) -> RoomShell {
    let shared = room.clone();
    let orders = orders.clone();
    let uploads = uploads.clone();
    Rc::new(move |call, cx| match call {
        RoomCall::Leave | RoomCall::Discard => cx.close(true),
        // A shared song read as the online tests' song, its id the
        // share's; a library song prepared at once.
        RoomCall::Prepare(job) if job.share.is_some() => {
            let s = crate::state::online::tests::room::song();
            let picked = crate::state::room::Picked {
                song: SongEntry {
                    id: job.song.clone(),
                    ..s.clone()
                },
                chart: s.charts[0].clone(),
            };
            shared.borrow_mut().finish_shared_load(&job, Ok(picked));
        }
        RoomCall::Prepare(job) => shared.borrow_mut().finish_load(&job, Ok(())),
        RoomCall::PlayStage(order) => orders.borrow_mut().push(*order),
        RoomCall::Upload(job) => uploads.borrow_mut().push(*job),
        RoomCall::Redirect(_) => {}
    })
}
