//! The field preview's skin, as both shells keep it: the skin wanted and
//! the layout it is readied for, the one shown, and the load running for
//! the one wanted (`docs/plans/shell-sharing.md`, item 1). A shell starts
//! its loads its own way ([`SkinLoad`]: desktop on a thread, the web in a
//! task) and puts what comes on its `FieldPreview`; nothing is drawn
//! until the skin wanted is in, readied for the layout wanted
//! ([`SkinSlot::ready`]), never the built-in arrows on the way.

use ddi_chart::Layout;
use ddi_noteskin::lua::Env;
use ddi_render::field_preview::{FieldPreviewOptions, ReceptorPreview, SkinWant};
use ddi_ui::screens::prompt_controls::{Controls, Picture};
use ddi_ui::state::settings::Settings;

use crate::ReceptorSkin;

/// A skin load a shell runs its own way. Dropped, it gives up.
pub trait SkinLoad {
    type Skin;
    /// The skin once in (`Some(None)`: none to draw with, the built-in
    /// arrows).
    fn poll(&mut self) -> Option<Option<Self::Skin>>;
}

/// The preview's skin: wanted, shown and loading.
pub struct SkinSlot<L> {
    wanted: Option<(SkinWant, Layout)>,
    shown: Option<(SkinWant, Layout)>,
    loading: Option<(SkinWant, Layout, L)>,
    /// The diagram's skin for the controls it last showed.
    receptor_skin: ReceptorSkin,
}

impl<L> Default for SkinSlot<L> {
    fn default() -> SkinSlot<L> {
        SkinSlot {
            wanted: None,
            shown: None,
            loading: None,
            receptor_skin: ReceptorSkin::default(),
        }
    }
}

impl<L: SkinLoad> SkinSlot<L> {
    /// Wants `want`, readied for `layout`.
    pub fn want(&mut self, want: SkinWant, layout: Layout) {
        self.wanted = Some((want, layout));
    }

    /// Options' demo under `settings`: its options, and the skin they
    /// draw with wanted.
    pub fn set_settings(&mut self, settings: &Settings) -> FieldPreviewOptions {
        let options = crate::field_preview_options(settings);
        let want = crate::field_preview_skin(settings, &options);
        let layout = want.layout();
        self.want(want, layout);
        options
    }

    /// The prompt's receptor diagram: its options and pose, its skin
    /// wanted.
    pub fn set_receptors(&mut self, p: &Picture) -> (FieldPreviewOptions, ReceptorPreview) {
        let (options, want, pose) = self.receptor_skin.options(p);
        self.want(want, pose.layout.clone());
        (options, pose)
    }

    /// The diagram's skin for `controls` (a chart song select shows)
    /// wanted ahead, so the prompt's diagram is in from its first frame.
    pub fn warm(&mut self, controls: &Controls) -> FieldPreviewOptions {
        let (options, want, layout) = crate::receptor_skin(controls);
        self.want(want, layout);
        options
    }

    /// For a new device: the want kept (or the one shown, to show again),
    /// nothing shown or loading.
    pub fn for_new_device(&self) -> SkinSlot<L> {
        SkinSlot {
            wanted: self.wanted.clone().or_else(|| self.shown.clone()),
            ..SkinSlot::default()
        }
    }

    /// Starts the load wanted unless it runs (an older one dropped, so it
    /// gives up), and polls it: `Some(skin)` is to go on the preview now
    /// (`None` inside: the built-in arrows, at once for a want without a
    /// key), and is from then shown.
    pub fn step(&mut self, start: impl FnOnce(&str, &Layout, Env) -> L) -> Option<Option<L::Skin>> {
        let Some((want, layout)) = &self.wanted else {
            return None;
        };
        if self.shown == self.wanted {
            self.loading = None;
            return None;
        }
        if self
            .loading
            .as_ref()
            .is_some_and(|(w, l, _)| (w, l) != (want, layout))
        {
            self.loading = None;
        }
        if self.loading.is_none() {
            let Some(key) = &want.key else {
                self.shown = self.wanted.clone();
                return Some(None);
            };
            let load = start(key, layout, want.env(layout));
            self.loading = Some((want.clone(), layout.clone(), load));
        }
        let (_, _, load) = self.loading.as_mut()?;
        let skin = load.poll()?;
        let (want, layout, _) = self.loading.take()?;
        self.shown = Some((want, layout));
        Some(skin)
    }

    /// Nothing is wanted, or the skin wanted is shown.
    pub fn ready(&self) -> bool {
        self.wanted.is_none() || self.shown == self.wanted
    }

    /// A skin is wanted and shown.
    pub fn skin_in(&self) -> bool {
        self.wanted.is_some() && self.shown == self.wanted
    }

    /// A load runs.
    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }

    /// The load running for the skin wanted.
    pub fn load(&self) -> Option<&L> {
        self.loading
            .as_ref()
            .filter(|(w, l, _)| {
                self.wanted
                    .as_ref()
                    .is_some_and(|(ww, wl)| (w, l) == (ww, wl))
            })
            .map(|(_, _, load)| load)
    }

    /// The skin shown and the layout it was readied for.
    pub fn shown(&self) -> Option<(&SkinWant, &Layout)> {
        self.shown.as_ref().map(|(w, l)| (w, l))
    }

    /// The layout the skin shown was readied for.
    pub fn shown_layout(&self) -> Option<&Layout> {
        self.shown.as_ref().map(|(_, l)| l)
    }

    /// What is wanted against what is shown, for the verify skill: each
    /// as `key layout-id lanes reverse params`, and whether a load runs.
    pub fn debug_wants(&self) -> String {
        let one = |v: &Option<(SkinWant, Layout)>| match v {
            Some((w, l)) => format!(
                "{} {} {} lanes rev={} {:?}",
                w.key.as_deref().unwrap_or("builtin"),
                l.id,
                l.lanes.len(),
                w.reverse,
                w.params
            ),
            None => "none".into(),
        };
        format!(
            "wanted: {} | shown: {} | loading: {} | same: {}",
            one(&self.wanted),
            one(&self.shown),
            self.loading.is_some(),
            self.wanted == self.shown
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;

    /// A load whose result the test gives, counting its starts and drops.
    struct Fake {
        result: Rc<RefCell<Option<Option<&'static str>>>>,
        dropped: Rc<Cell<u32>>,
    }

    impl SkinLoad for Fake {
        type Skin = &'static str;
        fn poll(&mut self) -> Option<Option<&'static str>> {
            self.result.borrow_mut().take()
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.dropped.set(self.dropped.get() + 1);
        }
    }

    #[derive(Default)]
    struct Loads {
        started: Vec<String>,
        result: Rc<RefCell<Option<Option<&'static str>>>>,
        dropped: Rc<Cell<u32>>,
    }

    impl Loads {
        fn step(&mut self, slot: &mut SkinSlot<Fake>) -> Option<Option<&'static str>> {
            slot.step(|key, _, _| {
                self.started.push(key.to_string());
                Fake {
                    result: self.result.clone(),
                    dropped: self.dropped.clone(),
                }
            })
        }
        fn finish(&self, skin: Option<&'static str>) {
            *self.result.borrow_mut() = Some(skin);
        }
    }

    fn want(key: Option<&str>) -> (SkinWant, Layout) {
        let options = FieldPreviewOptions {
            layout: "dance-single".into(),
            ..FieldPreviewOptions::default()
        };
        let want = SkinWant::new(key.map(str::to_string), &options);
        let layout = want.layout();
        (want, layout)
    }

    #[test]
    fn nothing_shows_until_the_skin_wanted_is_in() {
        let mut slot = SkinSlot::default();
        let mut loads = Loads::default();
        assert!(slot.ready() && !slot.skin_in());
        assert_eq!(loads.step(&mut slot), None);
        let (w, l) = want(Some("a"));
        slot.want(w, l);
        assert!(!slot.ready());
        assert_eq!(loads.step(&mut slot), None);
        assert!(slot.loading() && slot.load().is_some() && !slot.ready());
        // Polled again, not started again.
        assert_eq!(loads.step(&mut slot), None);
        assert_eq!(loads.started, ["a"]);
        loads.finish(Some("skin a"));
        assert_eq!(loads.step(&mut slot), Some(Some("skin a")));
        assert!(slot.ready() && slot.skin_in() && !slot.loading());
        assert_eq!(slot.shown().map(|(w, _)| w.key.as_deref()), Some(Some("a")));
        assert_eq!(loads.step(&mut slot), None);
        assert_eq!(loads.started, ["a"]);
    }

    #[test]
    fn a_newer_want_drops_the_older_load() {
        let mut slot = SkinSlot::default();
        let mut loads = Loads::default();
        let (w, l) = want(Some("a"));
        slot.want(w, l);
        loads.step(&mut slot);
        let (w, l) = want(Some("b"));
        slot.want(w, l);
        // The older load runs still, but is not the one wanted.
        assert!(slot.loading() && slot.load().is_none());
        loads.step(&mut slot);
        assert_eq!(loads.started, ["a", "b"]);
        assert_eq!(loads.dropped.get(), 1);
        // Back to one shown: the load running goes.
        loads.finish(Some("skin b"));
        assert_eq!(loads.step(&mut slot), Some(Some("skin b")));
        let (w, l) = want(Some("c"));
        slot.want(w, l);
        loads.step(&mut slot);
        let (w, l) = want(Some("b"));
        slot.want(w, l);
        assert_eq!(loads.step(&mut slot), None);
        assert!(!slot.loading() && slot.ready());
        assert_eq!(loads.dropped.get(), 3);
    }

    #[test]
    fn a_want_without_a_key_shows_at_once() {
        let mut slot = SkinSlot::default();
        let mut loads = Loads::default();
        let (w, l) = want(None);
        slot.want(w, l);
        assert_eq!(loads.step(&mut slot), Some(None));
        assert!(slot.ready() && slot.skin_in());
        assert!(loads.started.is_empty());
    }

    #[test]
    fn a_failed_load_shows_the_built_in_arrows() {
        let mut slot = SkinSlot::default();
        let mut loads = Loads::default();
        let (w, l) = want(Some("gone"));
        slot.want(w, l);
        loads.step(&mut slot);
        loads.finish(None);
        assert_eq!(loads.step(&mut slot), Some(None));
        assert!(slot.ready());
    }

    #[test]
    fn a_warm_then_the_same_controls_picture_loads_once() {
        let settings = Settings::default();
        let controls = Rc::new(Controls::new(&Layout::dance_single(), &settings));
        let mut slot = SkinSlot::default();
        let mut loads = Loads::default();
        let warmed = slot.warm(&controls);
        loads.step(&mut slot);
        loads.finish(Some("diagram"));
        assert_eq!(loads.step(&mut slot), Some(Some("diagram")));
        let picture = Picture {
            controls: controls.clone(),
            size: [400.0, 100.0],
            ready: Rc::default(),
            time: 0.0,
            active: None,
        };
        let (options, pose) = slot.set_receptors(&picture);
        assert_eq!(options, warmed);
        assert_eq!(Some(&pose.layout), slot.shown_layout());
        assert_eq!(loads.step(&mut slot), None);
        assert!(slot.ready());
        assert_eq!(loads.started.len(), 1);
    }

    #[test]
    fn a_new_device_keeps_the_want() {
        let mut slot = SkinSlot::default();
        let mut loads = Loads::default();
        let (w, l) = want(Some("a"));
        slot.want(w, l);
        loads.step(&mut slot);
        loads.finish(Some("skin a"));
        loads.step(&mut slot);
        // Shown only: wanted again on the new device.
        let mut fresh: SkinSlot<Fake> = slot.for_new_device();
        assert!(!fresh.ready() && !fresh.loading());
        loads.step(&mut fresh);
        assert_eq!(loads.started, ["a", "a"]);
        // Wanted and loading: the want kept, the load not.
        let (w, l) = want(Some("b"));
        fresh.want(w, l);
        loads.step(&mut fresh);
        let newer: SkinSlot<Fake> = fresh.for_new_device();
        assert!(!newer.loading());
        assert!(newer.debug_wants().starts_with("wanted: b "));
    }
}
