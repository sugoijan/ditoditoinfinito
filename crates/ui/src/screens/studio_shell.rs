//! The announcer studio's page on any platform
//! (`docs/plans/announcer-studio.md`): the draft and what the page shows
//! of it ([`StudioState`]), and what each press does to them
//! ([`call`], [`act`]), written once for both shells. What only a
//! platform can do (keeping the draft and the recordings, its pickers,
//! playing a sound, making the pack's files, hearing a line in its scene)
//! goes to its [`StudioHost`], as the options' shell goes to its
//! `OptionsPlatform` (`super::options_shell`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ddi_l10n::UiText;
use ddi_l10n::msg::studio as m;
use ddi_library::announcer::draft::{Adopted, DRAFT_MAX_BYTES, Draft, PackPlan, TAKE_MAX_BYTES};
use ddi_library::announcer::examples::Set;
use ddi_library::announcer::{Bundled, Line, MAX_FILES};

use super::Env;
use super::data::Platform;
use super::page::{PageCall, PageDoc};
use super::studio::{self, Act, Applied, Example, View};
use crate::screen::Cx;

/// The draft and what the page shows beside it, shared by the shell's
/// calls and its tasks.
pub struct StudioState {
    pub draft: RefCell<Draft>,
    pub view: RefCell<View>,
    /// Each line's examples in the set chosen ([`StudioState::refresh_examples`]).
    pub examples: RefCell<BTreeMap<Line, Vec<Example>>>,
}

impl StudioState {
    /// The page on `draft`, its examples the set remembered.
    pub fn new(draft: Draft, status: String) -> StudioState {
        StudioState {
            draft: RefCell::new(draft),
            view: RefCell::new(View {
                examples: studio::remembered_set(),
                status,
                ..View::default()
            }),
            examples: RefCell::default(),
        }
    }

    /// The page as it is now, in `platform`'s words.
    pub fn doc(&self, platform: Platform) -> PageDoc {
        studio::doc(
            &self.draft.borrow(),
            &self.view.borrow(),
            &self.examples.borrow(),
            platform,
        )
    }

    /// The examples of the set chosen, with `ours` (the built-in
    /// announcer's voice in its language) for their sounds.
    pub fn refresh_examples(&self, ours: Option<&Bundled>) {
        let set = self.view.borrow().examples;
        *self.examples.borrow_mut() = studio::examples(set, ours);
    }

    /// The example set chosen.
    pub fn set(&self) -> Set {
        self.view.borrow().examples
    }

    /// What a pack made before says of itself taken in ([`Draft::adopt`]),
    /// what it says that the draft cannot carry over shown; its pool,
    /// where it is a declared pack, for its sounds ([`StudioState::take_in`]).
    pub fn adopt(&self, pack: Adopted) -> Option<PackPlan> {
        let plan = pack.plan.clone();
        self.view.borrow_mut().lost = plan
            .iter()
            .flat_map(|p| &p.lost)
            .map(|l| studio::lost_text(l).into())
            .collect();
        self.draft.borrow_mut().adopt(pack);
        plan
    }

    /// A recording picked or dropped, taken in: `path` its name (its path
    /// in a folder or zip added), `size` its bytes and `head` its first
    /// ones; on `line` where given (else where a declared pack's pool
    /// puts it, `plan`, else by its name), the first in text line
    /// `script`'s place where given (taken). Its id, or `None` where the
    /// draft has no room for it ([`Draft::has_room`]).
    pub fn take_in(
        &self,
        path: &str,
        size: u64,
        head: &[u8],
        line: Option<Line>,
        script: &mut Option<u64>,
        plan: Option<&PackPlan>,
    ) -> Option<u64> {
        let mut d = self.draft.borrow_mut();
        if !d.has_room(size) {
            return None;
        }
        let id = match script.take() {
            Some(s) => d.add_to(s, path, size, head),
            None => d.add(path, size, head),
        };
        if let Some(p) = plan.and_then(|p| p.takes.get(path)) {
            d.place(id, p);
        }
        if let Some(l) = line
            && let Some(t) = d.take_mut(id)
        {
            t.line = Some(l);
        }
        Some(id)
    }

    /// The line text line `id` is written for.
    pub fn script_line(&self, id: u64) -> Option<Line> {
        Some(self.draft.borrow().scripts.iter().find(|s| s.id == id)?.line)
    }
}

/// What the page says once picked files were taken in: `added`
/// recordings, `passed` files that are none, `over` left out for want of
/// room, and why reading them failed where it did.
pub fn added_status(added: usize, passed: usize, over: usize, failed: Option<&str>) -> UiText {
    match failed {
        Some(e) if added == 0 && over == 0 => m::status_files_unreadable(super::errors::error_text(e)),
        _ if over > 0 => m::status_added_some(
            added,
            over,
            MAX_FILES,
            TAKE_MAX_BYTES >> 20,
            DRAFT_MAX_BYTES >> 20,
        ),
        _ => m::status_added(added, passed),
    }
}

/// What a platform does for the studio's page.
pub trait StudioHost: 'static {
    /// The page's draft and view.
    fn state(&self) -> &StudioState;
    /// The theme's screens' environment (for the page's dialogs).
    fn env(&self) -> Option<Env>;
    /// The built-in announcer's voice in `set`'s language, as a pack (the
    /// examples' sounds).
    fn example_pack(&self, set: Set) -> Option<Bundled>;
    /// Keeps the draft (as it is now).
    fn save(&self);
    /// Shows the page again, soon.
    fn show(&self);
    /// What the page holds changed, for the debug state.
    fn publish(&self) {}
    /// An export made before no longer says what the draft does.
    fn forget_export(&self) {}
    /// Stops what plays.
    fn stop(&self);
    /// The page is left.
    fn leave(&self, cx: &mut Cx);
    /// Picks recordings for `line` (the first for text line `script`).
    fn pick_recordings(self: &Rc<Self>, line: Line, script: Option<u64>);
    /// Picks a pack made before: a zip (or loose recordings), or a folder.
    fn pick_pack(self: &Rc<Self>, folder: bool);
    /// Plays take `id` as it goes in the pack (again: stops it); whether
    /// the page is to show at once.
    fn play_take(self: &Rc<Self>, id: u64) -> bool;
    /// Plays our example `i` of `line` (again: stops it); as
    /// [`StudioHost::play_take`].
    fn play_example(self: &Rc<Self>, line: Line, i: usize) -> bool;
    /// Take `id`, off the draft, has its file and its sound let go.
    fn removed(&self, id: u64);
    /// Makes the pack to export; as [`StudioHost::play_take`].
    fn export(self: &Rc<Self>) -> bool;
    /// Saves the export made, in the press; as [`StudioHost::play_take`].
    fn save_zip(self: &Rc<Self>) -> bool;
    /// Makes the pack an announcer of the game; as
    /// [`StudioHost::play_take`].
    fn use_in_game(self: &Rc<Self>) -> bool;
    /// Hears a line in the game (`Act::Hear`, `HearTake`, `HearExample`).
    fn hear(self: &Rc<Self>, act: Act);
}

/// The page shown at once, as it is now.
fn doc_of<H: StudioHost>(host: &H) -> PageDoc {
    let platform = host.env().map_or_else(Default::default, |e| e.platform);
    host.state().doc(platform)
}

/// The draft changed by the page: kept, and what was made of it before
/// let go.
fn draft_changed<H: StudioHost>(host: &H) {
    host.forget_export();
    host.save();
}

/// A call of the page, in the input's handler: the page again where it
/// shows at once.
pub fn call<H: StudioHost>(host: &Rc<H>, call: PageCall, cx: &mut Cx) -> Option<PageDoc> {
    let id = match call {
        PageCall::Leave => {
            host.leave(cx);
            return None;
        }
        PageCall::Field { id, text } => {
            if studio::set_field(&mut host.state().draft.borrow_mut(), &id, &text) {
                draft_changed(&**host);
                host.publish();
            }
            return None;
        }
        PageCall::Button(id) => id,
    };
    match studio::act(&id)? {
        Act::Move(id) => {
            // Heard as its line is chosen.
            let playing = host.state().view.borrow().playing.clone();
            if playing.as_deref() != Some(&format!("take:{id}")) {
                host.play_take(id);
            }
            let weak = Rc::downgrade(host);
            let picker = studio::line_picker(
                host.env()?,
                &host.state().draft.borrow(),
                id,
                Rc::new(move |id, line, _cx| {
                    if let Some(h) = weak.upgrade() {
                        set_line(&*h, id, line);
                    }
                }),
            )?;
            cx.overlay(picker, true);
            None
        }
        Act::AddCombo => {
            let weak = Rc::downgrade(host);
            let dialog = studio::combo_dialog(
                host.env()?,
                Rc::new(move |n, _cx| {
                    let Some(h) = weak.upgrade() else {
                        return true;
                    };
                    if !h.state().draft.borrow_mut().add_combo(n) {
                        return false;
                    }
                    h.save();
                    h.show();
                    true
                }),
            );
            cx.overlay(dialog, true);
            None
        }
        Act::AddLanguage => {
            let weak = Rc::downgrade(host);
            let dialog = studio::language_dialog(
                host.env()?,
                &host.state().draft.borrow(),
                Rc::new(move |tag, _cx| {
                    let Some(h) = weak.upgrade() else {
                        return false;
                    };
                    if !h.state().draft.borrow_mut().add_language(tag) {
                        return false;
                    }
                    h.stop();
                    draft_changed(&*h);
                    h.show();
                    true
                }),
            );
            cx.overlay(dialog, true);
            None
        }
        Act::Also(id) => {
            let weak = Rc::downgrade(host);
            let picker = studio::also_picker(
                host.env()?,
                &host.state().draft.borrow(),
                id,
                Rc::new(move |id, line, _cx| {
                    let Some(h) = weak.upgrade() else {
                        return;
                    };
                    let Some(line) = line else {
                        return;
                    };
                    if h.state().draft.borrow_mut().share(id, line) {
                        draft_changed(&*h);
                        h.show();
                    }
                }),
            )?;
            cx.overlay(picker, true);
            None
        }
        Act::Tags(id) => {
            let weak = Rc::downgrade(host);
            let dialog = studio::tags_dialog(
                host.env()?,
                &host.state().draft.borrow(),
                id,
                Rc::new(move |tag, cx| {
                    let Some(h) = weak.upgrade() else {
                        return;
                    };
                    match tag {
                        Some(tag) => {
                            toggle_tag(&*h, id, tag);
                        }
                        None => {
                            let Some(env) = h.env() else {
                                return;
                            };
                            let weak = Rc::downgrade(&h);
                            let typed = studio::new_tag_dialog(
                                env,
                                Rc::new(move |tag, _cx| {
                                    weak.upgrade().is_some_and(|h| toggle_tag(&*h, id, tag))
                                }),
                            );
                            cx.overlay(typed, true);
                        }
                    }
                }),
            )?;
            cx.overlay(dialog, true);
            None
        }
        a => act(host, a),
    }
}

/// Tag `tag` put on take `id`, or taken off it; whether it did.
fn toggle_tag<H: StudioHost>(host: &H, id: u64, tag: &str) -> bool {
    if host.state().draft.borrow_mut().toggle_tag(id, tag).is_none() {
        return false;
    }
    draft_changed(host);
    host.show();
    true
}

/// Take `id` put on `line` (`None`: unsorted).
pub fn set_line<H: StudioHost>(host: &H, id: u64, line: Option<Line>) {
    host.state().draft.borrow_mut().set_line(id, line);
    host.stop();
    draft_changed(host);
    host.show();
}

/// What a press asks, but those opening a dialog ([`call`]'s): the page
/// again where it shows at once.
pub fn act<H: StudioHost>(host: &Rc<H>, act: Act) -> Option<PageDoc> {
    let state = host.state();
    let applied = studio::apply(act, &mut state.draft.borrow_mut(), &mut state.view.borrow_mut());
    if let Some(applied) = applied {
        match applied {
            Applied::Draft => {
                host.stop();
                draft_changed(&**host);
                host.publish();
            }
            Applied::Examples => {
                state.refresh_examples(host.example_pack(state.set()).as_ref());
                host.publish();
            }
            Applied::View => {}
            Applied::Kept => {
                host.save();
                host.publish();
            }
        }
        return Some(doc_of(&**host));
    }
    let now = match act {
        // ([`studio::apply`]'s.)
        Act::Tidy
        | Act::Stadium
        | Act::StepMania
        | Act::Examples
        | Act::Details
        | Act::RemoveCombo(_)
        | Act::Write(_)
        | Act::Unwrite(_)
        | Act::Language(_)
        | Act::RemoveLanguage(_)
        | Act::Mixed
        | Act::Switchable(_)
        | Act::TagDefault(_)
        | Act::Unshare(..) => false,
        // (The dialogs': [`call`]'s.)
        Act::Move(_) | Act::AddCombo | Act::AddLanguage | Act::Tags(_) | Act::Also(_) => false,
        Act::Add(line) => {
            host.pick_recordings(line, None);
            false
        }
        Act::AddTo(id) => {
            let line = state.script_line(id)?;
            host.pick_recordings(line, Some(id));
            false
        }
        Act::ImportFiles => {
            host.pick_pack(false);
            false
        }
        Act::ImportFolder => {
            host.pick_pack(true);
            false
        }
        Act::Play(id) => host.play_take(id),
        Act::Example(line, i) => host.play_example(line, i),
        Act::Remove(id) => {
            state.draft.borrow_mut().remove(id);
            draft_changed(&**host);
            host.removed(id);
            host.publish();
            true
        }
        Act::Export => host.export(),
        Act::Save => host.save_zip(),
        Act::UseInGame => host.use_in_game(),
        Act::Hear(_) | Act::HearTake(_) | Act::HearExample(..) | Act::HearShared(..) => {
            host.hear(act);
            false
        }
    };
    now.then(|| doc_of(&**host))
}

#[cfg(test)]
mod tests;
