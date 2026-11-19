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
        RoomCall::Redirect(_) | RoomCall::Preload(_) => {}
    })
}

/// The "You" badge after the name `id` (inside the actor `owner`, where
/// given) sits on the name's line (feedback round H, item 1): its plate's
/// middle on the name's letters' optical middle, its word on the plate's,
/// as [`super::room::badge_offsets`] has them for a name of `size`, read
/// from the frame's rects (so no screen places the two apart again).
pub(crate) fn assert_badge_on_line(h: &Harness, owner: Option<&str>, id: &str, size: f32) {
    let rect = |part: &str| -> [f32; 4] {
        match owner {
            Some(o) => h.ui.bounds_of(o, part),
            None => h.ui.bounds(part),
        }
        .unwrap_or_else(|| panic!("no {part} in {owner:?}"))
    };
    let middle = |r: [f32; 4]| (r[1] + r[3]) / 2.0;
    let name = rect(id);
    let plate = rect(&format!("{id}.Badge"));
    let word = rect(&format!("{id}.BadgeText"));
    let (plate_dy, word_dy) = super::room::badge_offsets(size);
    let line = (name[3] - name[1]) / ddi_actor::text::LINE_HEIGHT;
    assert!((line - size).abs() < 0.01, "{id}: the name is {line} px, not {size}");
    let at = middle(plate) - middle(name);
    assert!((at - plate_dy).abs() < 0.01, "{id}: the plate {at} below the name's middle, not {plate_dy}");
    let inner = middle(word) - middle(plate);
    assert!((inner - word_dy).abs() < 0.01, "{id}: the word {inner} from the plate's middle, not {word_dy}");
    // Its word centered across the plate too.
    let across = (word[0] + word[2]) / 2.0 - (plate[0] + plate[2]) / 2.0;
    assert!(across.abs() < 0.01, "{id}: the word {across} off the plate's middle");
}
