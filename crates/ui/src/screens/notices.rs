//! The app's notices over every screen (`docs/plans/ui-core.md`, step
//! 6.4): a banner across the top when the game meets a device it has no
//! calibration for ("First time here? …", "New audio path: …"), with its
//! actions (calibrate, use defaults, use another device's offset, later),
//! and a short line at the bottom when the devices change to known ones.
//! Drawn as the UI's chrome ([`crate::Ui::push_chrome`]): over the screens
//! and their dialogs, taking the pointers on its banner (the devices' line
//! lets them through), and the menu's focus only when sent there (F2, or
//! a fresh Up past a screen's top; Down, Back or F2 give it back).
//! The shell keeps what is shown ([`NoticesFeed`], [`NOTICES_CHANGED`])
//! and does what an action asks ([`NoticeActionFn`]).

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};

use super::select_dialogs::wrap_lines;
use super::{Env, frame, left};
use crate::look::{self, Spec, call, quad};
use crate::message::Message;
use crate::screen::{Cx, Handled, Input, Screen};

/// Broadcast when the feed changed.
pub const NOTICES_CHANGED: &str = "Notices.Changed";

/// A banner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppNotice {
    pub text: String,
    /// `(id, label, primary)`: the buttons, in order.
    pub actions: Vec<(String, String, bool)>,
}

/// What the notices show now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppNotices {
    pub banner: Option<AppNotice>,
    /// A short line at the bottom ("devices: …").
    pub toast: Option<String>,
}

pub type NoticesFeed = Rc<RefCell<AppNotices>>;

/// Does action `id` (inside the press: calibrating navigates).
pub type NoticeActionFn = Rc<dyn Fn(&str, &mut Cx)>;

pub struct NoticeBar {
    env: Env,
    feed: NoticesFeed,
    action: NoticeActionFn,
    shown: AppNotices,
    /// The layout's width in CSS pixels and its zoom onto the screen.
    css: [f32; 2],
    zoom: f32,
    /// The banner's height (the layout's pixels), 0 without one.
    height: std::cell::Cell<f32>,
    /// Keys or a pad in use: the "F2" hint shows (on touch it means
    /// nothing).
    by_keys: bool,
}

impl NoticeBar {
    pub fn new(env: Env, feed: NoticesFeed, action: NoticeActionFn) -> NoticeBar {
        NoticeBar {
            env,
            feed,
            action,
            shown: AppNotices::default(),
            css: [1280.0, 720.0],
            zoom: 1.0,
            height: std::cell::Cell::new(0.0),
            by_keys: false,
        }
    }

    fn place(&mut self, cx: &Cx) -> bool {
        let vp = cx.viewport();
        let ratio = vp.ratio.max(0.1);
        let css = [vp.physical[0] / ratio, vp.physical[1] / ratio];
        let zoom = if css[0] > 0.0 {
            vp.screen.width / css[0]
        } else {
            1.0
        };
        let changed = css != self.css || zoom != self.zoom;
        self.css = css;
        self.zoom = zoom;
        changed
    }

    fn tree(&self) -> Actor {
        let env = &self.env;
        let [w, h] = self.css;
        let mut children = Vec::new();
        self.height.set(0.0);
        if let Some(b) = &self.shown.banner {
            children.push(self.banner(b, w));
        }
        if let Some(t) = &self.shown.toast {
            let tw = (look::mono_width(t, 13.0) + 32.0).min(w - 24.0);
            let mut plate = quad("", [tw, 32.0], env.color("Plate2"));
            call(&mut plate, "cornerradius", &[16.0]);
            call(&mut plate, "xy", &[w / 2.0, h - 40.0]);
            let mut text = env.text("AppToast", t, 13.0, 400, "Muted");
            call(&mut text, "xy", &[w / 2.0, h - 40.0]);
            if let Kind::Text(r) = &mut text.kind {
                r.max_width = tw - 24.0;
                r.ellipsis = true;
            }
            let mut f = frame("AppToast.Frame", vec![plate, text]);
            look::command_of(&mut f, "On", "diffusealpha,0;linear,0.15;diffusealpha,1");
            children.push(f);
        }
        let mut root = Actor::new(Kind::Frame(children));
        call(&mut root, "zoom", &[self.zoom]);
        root
    }

    /// The banner across the top: its text, then its buttons (beside it
    /// where they fit, else under it).
    fn banner(&self, b: &AppNotice, w: f32) -> Actor {
        let env = &self.env;
        // Thinner on a phone: smaller text, less padding.
        let pad = if w < 900.0 { 10.0 } else { 16.0 };
        let size = if w < 900.0 { 13.0 } else { 15.0 };
        let buttons: Vec<(String, String, bool, f32)> = shown_actions(b, w)
            .into_iter()
            .map(|(id, label, primary)| {
                let bw = look::mono_width(label, if *primary { 18.0 } else { 14.0 })
                    + if *primary { 32.0 } else { 16.0 };
                (id.clone(), label.clone(), *primary, bw)
            })
            .collect();
        let buttons_w: f32 = buttons.iter().map(|b| b.3 + 8.0).sum();
        let beside = buttons_w < w * 0.45;
        // Room kept at the right for the "F2" hint (shown or not, so the
        // banner does not move when the player picks up a pad).
        let hint_w = 28.0;
        let text_w = if beside {
            w - 2.0 * pad - buttons_w - 16.0 - hint_w
        } else {
            w - 2.0 * pad
        };
        let lines = wrap_lines(&b.text, text_w, size);
        let line_h = size + 6.0;
        let text_h = lines.len() as f32 * line_h;
        // Buttons under the text in rows that fit.
        let mut rows: Vec<Vec<usize>> = Vec::new();
        if beside {
            rows.push((0..buttons.len()).collect());
        } else {
            let mut x = 0.0;
            for (i, bt) in buttons.iter().enumerate() {
                if rows.is_empty() || x + bt.3 > text_w - hint_w {
                    rows.push(Vec::new());
                    x = 0.0;
                }
                rows.last_mut().expect("a row").push(i);
                x += bt.3 + 8.0;
            }
        }
        let row_h = if w < 900.0 { 38.0 } else { 40.0 };
        let height = if beside {
            (text_h + 2.0 * pad).max(row_h + 16.0)
        } else {
            text_h + 2.0 * pad + rows.len() as f32 * row_h
        };
        self.height.set(height);
        let mut c = Vec::new();
        let mut plate = quad("AppNotice.Plate", [w, height], env.color("Plate2"));
        call(&mut plate, "xy", &[w / 2.0, height / 2.0]);
        c.push(plate);
        let mut edge = quad("", [w, 2.0], env.color("Accent"));
        call(&mut edge, "xy", &[w / 2.0, height - 1.0]);
        c.push(edge);
        let text_top = if beside { (height - text_h) / 2.0 } else { pad };
        for (i, l) in lines.iter().enumerate() {
            let t = env.text(&format!("AppNotice.Text{i}"), l, size, 500, "Text");
            c.push(left(t, pad, text_top + i as f32 * line_h + line_h / 2.0));
        }
        // Reached by keys and pads too: F2, or Up past the screen's top.
        // Its place: right of the text, or of the last row of buttons;
        // shown only while keys or a pad are in use.
        let mut hint = env.text("AppNotice.Hint", "F2", 11.0, 600, "Faint");
        let hint_y = if beside {
            height / 2.0
        } else {
            pad + text_h + (rows.len() as f32 - 0.5) * row_h
        };
        let hint_x = if beside {
            w - pad - buttons_w - hint_w / 2.0
        } else {
            w - pad - hint_w / 2.0
        };
        call(&mut hint, "xy", &[hint_x, hint_y]);
        hint.visible = self.by_keys;
        c.push(hint);
        for (r, row) in rows.iter().enumerate() {
            let (mut x, y) = if beside {
                (w - pad - buttons_w + 8.0, height / 2.0)
            } else {
                (pad, pad + text_h + r as f32 * row_h + row_h / 2.0)
            };
            for &i in row {
                let (id, label, primary, bw) = &buttons[i];
                let kind = if *primary { "PlayButton" } else { "Link" };
                let mut a =
                    look::component_or(&*env.look, kind, "Button", &Spec::new([*bw, 34.0], label));
                a.name = format!("AppNotice.{id}");
                call(&mut a, "xy", &[x + bw / 2.0, y]);
                c.push(a);
                x += bw + 8.0;
            }
        }
        let mut f = frame("AppNotice", c);
        look::command_of(&mut f, "On", "diffusealpha,0;linear,0.15;diffusealpha,1");
        f
    }

    fn refresh(&mut self, root: &mut Actor) {
        self.shown = self.feed.borrow().clone();
        *root = self.tree();
        let mut st = self.env.state.borrow_mut();
        st.app_notice = self.shown.banner.as_ref().map(|b| b.text.clone());
        st.app_actions = self
            .shown
            .banner
            .as_ref()
            .map(|b| {
                shown_actions(b, self.css[0])
                    .iter()
                    .map(|a| a.0.clone())
                    .collect()
            })
            .unwrap_or_default();
        st.app_toast = self.shown.toast.clone();
    }
}

impl Screen for NoticeBar {
    fn name(&self) -> &str {
        "notices"
    }

    fn reserve(&self) -> f32 {
        self.height.get() * self.zoom
    }

    /// The devices' line only informs: taps go through it.
    fn lets_through(&self, target: &crate::hit::Target) -> bool {
        target.within("AppToast.Frame")
    }

    fn focusables(&self) -> Vec<crate::nav::Focusable> {
        self.shown
            .banner
            .iter()
            .flat_map(|b| shown_actions(b, self.css[0]))
            .map(|(id, _, _)| crate::nav::Focusable::new(format!("AppNotice.{id}")))
            .collect()
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.place(cx);
        let mut root = Actor::new(Kind::Frame(Vec::new()));
        self.refresh(&mut root);
        root
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        if self.place(cx) {
            self.refresh(root);
            relight(root, cx);
        }
        if cx.by_keys() != self.by_keys {
            self.by_keys = cx.by_keys();
            if let Some(h) = crate::hit::find_mut(root, "AppNotice.Hint") {
                h.visible = self.by_keys;
            }
        }
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == NOTICES_CHANGED && *self.feed.borrow() != self.shown {
            self.refresh(root);
            relight(root, cx);
        }
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Pointer {
                target: Some(t),
                gesture: crate::Gesture::Tap,
                ..
            } => {
                if let Some(id) = t
                    .chain
                    .iter()
                    .find_map(|n| n.strip_prefix("AppNotice."))
                    .filter(|id| !id.starts_with("Text") && *id != "Plate" && *id != "Hint")
                {
                    let id = id.to_string();
                    (self.action)(&id, cx);
                }
                Handled::Yes
            }
            Input::Pointer { .. } => Handled::Yes,
            // Start on an action reached by keys or a pad.
            Input::Activate { name } => {
                if let Some(id) = name.strip_prefix("AppNotice.") {
                    let id = id.to_string();
                    (self.action)(&id, cx);
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// The banner built again while it has the menu's focus: the focused
/// action lit again (its `GainFocus`, at once).
fn relight(root: &mut Actor, cx: &Cx) {
    if let Some(a) = cx.focused().and_then(|f| crate::hit::find_mut(root, f)) {
        a.play("GainFocus", &mut ddi_actor::Problems::default());
        a.finish_tweening();
    }
}

/// The banner's actions shown `w` (CSS pixels) wide: on a phone one
/// offer of another device's offset (the first: the audio's), so its
/// buttons keep to two rows.
fn shown_actions(b: &AppNotice, w: f32) -> Vec<(&String, &String, &bool)> {
    let mut offers = 0;
    b.actions
        .iter()
        .filter(|(id, _, _)| {
            if !id.starts_with("copy-") {
                return true;
            }
            offers += 1;
            w >= 900.0 || offers == 1
        })
        .map(|(id, label, primary)| (id, label, primary))
        .collect()
}

/// The device banner for the settings and the devices last probed
/// (`None`: not probed yet, no audio context before a gesture): the first
/// run's while there are no profiles at all, else one naming the devices
/// without a calibration, with their actions: `calibrate`, `defaults`,
/// `copy-audio:<id>` / `copy-display:<id>` (a calibrated device's
/// offset), `later`. `None` when every device is known.
pub fn device_notice(
    settings: &crate::state::settings::Settings,
    probed: Option<&ddi_platform::DeviceProfile>,
) -> Option<AppNotice> {
    let action =
        |id: &str, label: &str, primary: bool| (id.to_string(), label.to_string(), primary);
    let Some(profile) = probed else {
        let first = settings.audio_profiles.is_empty() && settings.display_profiles.is_empty();
        return first.then(|| AppNotice {
            text: "First time here? Calibrate the timing for your audio and display before \
                   playing; it takes a minute."
                .into(),
            actions: vec![
                action("calibrate", "calibrate", true),
                action("later", "later", false),
            ],
        });
    };
    let new_audio = settings.audio_profile(&profile.audio.id).is_none();
    let new_display = settings.display_profile(&profile.display.id).is_none();
    if !new_audio && !new_display {
        return None;
    }
    let mut text = String::new();
    if new_audio {
        text.push_str(&format!("New audio path: {}. ", profile.audio.label));
    }
    if new_display {
        text.push_str(&format!("New display: {}. ", profile.display.label));
    }
    text.push_str("No calibration for it yet.");
    let mut actions = vec![
        action("calibrate", "calibrate", true),
        action("defaults", "use defaults", false),
    ];
    for (kind, new, list) in [
        ("audio", new_audio, &settings.audio_profiles),
        ("display", new_display, &settings.display_profiles),
    ] {
        // One offer a kind (the calibrated device seen last), so the
        // banner stays two lines on a phone however many there are.
        if new {
            let best = list
                .iter()
                .filter(|p| p.calibrated)
                .max_by(|a, b| a.last_seen.total_cmp(&b.last_seen));
            if let Some(p) = best {
                actions.push((
                    format!("copy-{kind}:{}", p.id),
                    format!("use {:+.0} ms from {}", p.offset * 1000.0, p.label),
                    false,
                ));
            }
        }
    }
    actions.push(action("later", "later", false));
    Some(AppNotice { text, actions })
}

/// The short line when the devices change to known ones: "devices:
/// <audio> (+12 ms) · <display> (+0 ms)", each with its offset.
pub fn devices_line(
    settings: &crate::state::settings::Settings,
    profile: &ddi_platform::DeviceProfile,
) -> String {
    let a = settings.audio_profile(&profile.audio.id);
    let d = settings.display_profile(&profile.display.id);
    format!(
        "devices: {} ({:+.0} ms) · {} ({:+.0} ms)",
        profile.audio.label,
        a.map(|p| p.offset).unwrap_or(0.0) * 1000.0,
        profile.display.label,
        d.map(|p| p.offset).unwrap_or(0.0) * 1000.0
    )
}

/// The devices probed, kept with a starting offset: the defaults, or a
/// calibrated device's (`copy-audio:<id>`, `copy-display:<id>`).
pub fn register_devices(
    settings: &mut crate::state::settings::Settings,
    profile: &ddi_platform::DeviceProfile,
    action: &str,
) {
    use crate::state::settings::Settings;
    // One offer or both (`copy-audio:<id>+copy-display:<id>`).
    let copy_audio = action
        .split('+')
        .find_map(|a| a.strip_prefix("copy-audio:"));
    let copy_display = action
        .split('+')
        .find_map(|a| a.strip_prefix("copy-display:"));
    let audio_initial = copy_audio
        .and_then(|id| settings.audio_profile(id).map(|p| p.offset))
        .unwrap_or(settings.audio_offset);
    let display_initial = copy_display
        .and_then(|id| settings.display_profile(id).map(|p| p.offset))
        .unwrap_or(settings.visual_offset);
    Settings::touch_profile(&mut settings.audio_profiles, &profile.audio, audio_initial);
    Settings::touch_profile(
        &mut settings.display_profiles,
        &profile.display,
        display_initial,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::settings::Settings;
    use ddi_platform::{DeviceFingerprint, DeviceProfile};

    fn profile() -> DeviceProfile {
        DeviceProfile {
            audio: DeviceFingerprint::new("audio:48000:5:20", "48 kHz · ~20 ms"),
            display: DeviceFingerprint::new("display:1280x720@1", "1280×720 (1×)"),
        }
    }

    #[test]
    fn the_banner_follows_the_profiles() {
        let mut s = Settings::default();
        let first = device_notice(&s, None).unwrap();
        assert!(first.text.starts_with("First time here?"));
        assert_eq!(first.actions.len(), 2);
        let p = profile();
        let n = device_notice(&s, Some(&p)).unwrap();
        assert!(n.text.starts_with("New audio path: 48 kHz"), "{}", n.text);
        assert!(n.text.contains("New display: 1280×720"));
        // A calibrated device elsewhere offers its offset.
        Settings::set_profile_offset(
            &mut s.audio_profiles,
            &DeviceFingerprint::new("audio:44100:5:40", "Headphones"),
            0.03,
        );
        let n = device_notice(&s, Some(&p)).unwrap();
        assert!(
            n.actions.iter().any(
                |a| a.0 == "copy-audio:audio:44100:5:40" && a.1 == "use +30 ms from Headphones"
            ),
            "{:?}",
            n.actions
        );
        register_devices(&mut s, &p, "copy-audio:audio:44100:5:40");
        assert_eq!(s.audio_profile("audio:48000:5:20").unwrap().offset, 0.03);
        assert!(device_notice(&s, Some(&p)).is_none());
        // Known devices and profiles: no first-run banner either.
        assert!(device_notice(&s, None).is_none());
    }
}
