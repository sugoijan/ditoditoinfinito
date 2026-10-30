//! What the first screens need, loaded under the boot shell so they
//! come whole and the first transitions are instant: the theme's
//! essential images (`ddi_ui::screens::essential_images`: the title,
//! the rain, the wordmark, the hint atlases) into the painter's texture
//! cache at the sides the screens draw them, the song manifest and the
//! bundled skins' index into their caches, and the skin the settings
//! name readied for their style on the app's one field preview
//! (`field_preview`), so the first prompt and the first play have it.
//! The shell's bar runs its last stretch on them (`boot::settling`:
//! each texture, the three others as units, each weighted by the
//! milliseconds it should take, so the bar keeps one pace: a round trip
//! for the manifest and the index; for the skin its bytes on the link
//! as the shell measured it downloading the game, and its drawings at
//! what the last load of it took, remembered in `localStorage`) and
//! the shell fades once every one is in. A skin that should take
//! longer than the shell waits for one is not waited for at all: it
//! loads on behind the menu. At the fade the app phase's bytes and its
//! own time are left for the shell (`ddi.boot.v1`), which sizes the
//! download's share of the bar from them next time. What is per song or
//! per choice (banners, other skins, movies) loads when it is drawn, as
//! before.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_actor::screen::VirtualScreen;
use ddi_chart::Layout;
use ddi_render::texture_cache::TextureKey;
use ddi_ui::screens::prompt_controls::Controls;

use crate::settings::Settings;
use crate::web::gfx::Gfx;
use ddi_ui::screens::Env as ThemeEnv;

thread_local! {
    static STARTED: Cell<bool> = const { Cell::new(false) };
    /// The skin's load measured, from the preload's start to the fade.
    static LOAD: RefCell<Option<Load>> = const { RefCell::new(None) };
}

struct Load {
    /// Which skin for which style the memory is of.
    name: String,
    /// When the preload began, ms.
    began: f64,
    /// Bytes a millisecond, as the shell measured the link, less
    /// [`PESSIMISM`].
    bandwidth: f64,
    /// The fetch and SVG draw counters where the load began, and the
    /// textures' bytes.
    bytes_at: u64,
    draws_at: u32,
    texture_bytes_at: u64,
    /// What the load should take: drawings (the last load's, or a
    /// guess) and bytes (the files asked for at once, once the index
    /// names them; the last load's or a guess before).
    draws: u32,
    bytes: u64,
    /// The skin's unit, ms, as added to the shell's bar: `None` where
    /// the skin is not waited for.
    unit: Option<f64>,
    /// The skin came in.
    is_in: bool,
    /// The unit was reported done (the skin in, or the deadline).
    reported: bool,
    /// The skin's load, put off to the fade where the skin is not
    /// waited for, so its files do not take the link from what is.
    deferred: Option<Deferred>,
    /// How long the shell waits for the skin at most, ms.
    wait_ms: f64,
}

/// A skin's load put off to the fade: the device and the controls the
/// preview readies it for, and the bundled skin to ask the files of.
type Deferred = (Rc<Gfx>, Controls, Option<(String, Layout)>);

impl Load {
    fn bytes_ms(&self) -> f64 {
        self.bytes as f64 / self.bandwidth
    }

    fn draws_ms(&self) -> f64 {
        f64::from(self.draws) * DRAW_MS
    }
}

/// `localStorage`'s key for this skin's last load: `name;draws;bytes`.
const MEMORY_KEY: &str = "ddi.preload.v1";
/// `localStorage`'s key for the shell's plan of the app's phase:
/// `bytes;ms` (its own time, the bytes' apart).
const PLAN_KEY: &str = "ddi.boot.v1";
/// What a skin's first load is measured against before its counts are
/// known: about what the bundled skins take for one layout, rounded
/// up, so the bar runs short of the mark rather than past it.
const GUESS_DRAWS: u32 = 600;
const GUESS_BYTES: u64 = 8_000_000;
/// What a drawing of the SVG workers costs, ms of wall time, several
/// workers drawing at once.
const DRAW_MS: f64 = 1.2;
/// How much of the link as the shell measured it is counted on: its
/// measure runs high (a burst, no round trips in it), and a bar short
/// of the mark looks better than one past it.
const PESSIMISM: f64 = 0.7;
/// A round trip (the manifest's, the index's), ms.
const RTT_MS: f64 = 150.0;
/// The link where the shell measured none, bytes a millisecond
/// (20 Mbit/s).
const DEFAULT_BANDWIDTH: f64 = 2500.0;
/// How long the shell waits for the skin at most, ms, after the preload
/// began, and how long the skin may be expected to take for the shell
/// to wait for it at all: on a slow link a skin's files take longer
/// than the menu should wait (it goes on loading behind the menu, on
/// the preview). Under the menu, the skin is needed seconds later.
const SKIN_WAIT_MS: f64 = 4000.0;
/// The same for the play route's start prompt, which shows the skin
/// from its first frame (its diagram), so a longer wait is worth it.
const PROMPT_SKIN_WAIT_MS: f64 = 10_000.0;

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn remembered(name: &str) -> Option<(u32, u64)> {
    let value = local_storage()?.get_item(MEMORY_KEY).ok()??;
    let (rest, bytes) = value.rsplit_once(';')?;
    let (kept, draws) = rest.rsplit_once(';')?;
    (kept == name).then(|| Some((draws.parse().ok()?, bytes.parse().ok()?)))?
}

fn remember(name: &str, draws: u32, bytes: u64) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(MEMORY_KEY, &format!("{name};{draws};{bytes}"));
    }
}

/// Starts the loads, once: from the first UI host's first frame on a
/// device (`vp` its virtual screen, for the title's side). `warm`: the
/// skin is readied on the preview here (a page's host; the play route's
/// prompt readies the chart's own on it, and a second load of the same
/// skin would double the drawing).
pub(crate) fn start(env: &ThemeEnv, vp: VirtualScreen, gfx: &Rc<Gfx>, warm: bool) {
    if STARTED.with(|s| s.replace(true)) {
        return;
    }
    // The images, asked of the painter as a draw would: the next
    // frame's `prepare_textures` fetches and decodes them.
    let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio()) as f32;
    let images = ddi_ui::screens::essential_images(env, vp);
    crate::web::screen::with_painter(gfx, |p| {
        for (source, side) in &images {
            p.load_texture(TextureKey::drawn(source.as_str(), side * dpr));
        }
    });
    crate::boot::add_unit(RTT_MS);
    crate::boot::add_unit(RTT_MS);
    wasm_bindgen_futures::spawn_local(async {
        let _ = crate::songs::load_manifest().await;
        crate::boot::unit_done(RTT_MS);
    });
    // The skin the settings name, for their style (`poll` reports it),
    // its files all asked for as soon as the index names them.
    let settings = Settings::load();
    let want =
        ddi_shell::field_preview_skin(&settings, &ddi_shell::field_preview_options(&settings));
    let name = format!(
        "{}:{}",
        settings.style,
        settings.noteskin.as_deref().unwrap_or_default()
    );
    let (draws, bytes) = remembered(&name).unwrap_or((GUESS_DRAWS, GUESS_BYTES));
    let mut load = Load {
        name,
        began: now_ms(),
        bandwidth: crate::boot::bandwidth().map_or(DEFAULT_BANDWIDTH, |b| b * PESSIMISM / 1000.0),
        bytes_at: crate::noteskins::fetches().0,
        draws_at: crate::web::svg::counts().0,
        texture_bytes_at: crate::web::textures::bytes(),
        draws,
        bytes,
        unit: None,
        is_in: false,
        reported: false,
        deferred: None,
        wait_ms: if crate::boot::expected() {
            PROMPT_SKIN_WAIT_MS
        } else {
            SKIN_WAIT_MS
        },
    };
    let expected = load.bytes_ms() + load.draws_ms();
    let wait = expected <= load.wait_ms;
    let layout = Layout::builtin(&settings.style).unwrap_or_else(Layout::dance_single);
    let controls = Controls::new(&layout, &settings);
    let skin = want.key.clone().map(|key| (key, want.layout()));
    if wait {
        crate::boot::add_unit(expected);
        load.unit = Some(expected);
        if warm {
            warm_preview(gfx, &controls);
        }
    } else {
        load.reported = true;
        load.deferred = warm.then(|| (gfx.clone(), controls, skin.clone()));
    }
    LOAD.with_borrow_mut(|l| *l = Some(load));
    wasm_bindgen_futures::spawn_local(async move {
        let _ = crate::noteskins::bundled().await;
        crate::boot::unit_done(RTT_MS);
        // The skins one can pick, for the prompt's options' first frame.
        wasm_bindgen_futures::spawn_local(async {
            let _ = crate::noteskins::choices().await;
        });
        if wait && let Some((key, layout)) = &skin {
            let bytes = crate::noteskins::prefetch(key, layout);
            LOAD.with_borrow_mut(|l| {
                if let Some(l) = l
                    && bytes > 0
                {
                    l.bytes = bytes;
                    if let Some(unit) = l.unit {
                        let now = l.bytes_ms() + l.draws_ms();
                        crate::boot::resize_unit(unit, now);
                        l.unit = Some(now);
                    }
                }
            });
        }
    });
}

/// Readies the skin for `controls` on the app's field preview.
fn warm_preview(gfx: &Rc<Gfx>, controls: &Controls) {
    let slot = crate::field_preview::slot();
    let mut s = slot.borrow_mut();
    if s.as_ref().is_none_or(|f| !f.on(gfx)) {
        *s = Some(crate::field_preview::FieldPreviewHost::new(gfx));
    }
    if let Some(f) = s.as_mut() {
        f.warm(gfx, &Rc::downgrade(&slot), controls);
    }
}

fn now_ms() -> f64 {
    crate::web::clock::PerformanceClock::now_ms()
}

/// Each frame while the shell waits: the skin's unit, once the skin is
/// in, or once [`SKIN_WAIT_MS`] passed (the skin loads on); how far it
/// is meanwhile, its bytes and its drawings against what it should
/// take.
pub(crate) fn poll() {
    let (begun, fetched) = crate::noteskins::fetches();
    let (_, drawn) = crate::web::svg::counts();
    let is_in = crate::field_preview::slot()
        .borrow()
        .as_ref()
        .is_some_and(|f| f.skin_in());
    LOAD.with_borrow_mut(|l| {
        let Some(l) = l else {
            return;
        };
        if is_in && !l.is_in {
            l.is_in = true;
            remember(&l.name, drawn - l.draws_at, fetched - l.bytes_at);
        }
        if l.reported {
            return;
        }
        let Some(unit) = l.unit else {
            return;
        };
        if is_in || now_ms() - l.began >= l.wait_ms {
            l.reported = true;
            crate::boot::unit_done(unit);
            return;
        }
        let bytes = (fetched - l.bytes_at) as f64 / (begun - l.bytes_at).max(1) as f64;
        let draws = f64::from(drawn - l.draws_at) / f64::from(l.draws.max(1));
        let partial = l.bytes_ms() * bytes.min(1.0) + l.draws_ms() * draws.min(1.0);
        crate::boot::unit_partial(partial.min(unit * 0.98));
    });
}

/// The shell fades: what the app's phase took is left for the shell's
/// next plan, when the skin came in or was not waited for (a deadline's
/// cut would make the next plan short).
pub(crate) fn finished() {
    let deferred = LOAD.with_borrow_mut(|l| {
        let l = l.as_mut()?;
        if l.is_in || l.unit.is_none() {
            let (_, fetched) = crate::noteskins::fetches();
            let bytes = fetched - l.bytes_at + crate::web::textures::bytes() - l.texture_bytes_at;
            let own = (now_ms() - l.began - bytes as f64 / l.bandwidth).max(300.0);
            if let Some(storage) = local_storage() {
                let _ = storage.set_item(PLAN_KEY, &format!("{bytes};{own:.0}"));
            }
        }
        l.deferred.take()
    });
    // The skin put off: its files asked for together now, and readied
    // on the preview once the screen's entrance is done (a load parses
    // the skin's components on the main thread).
    if let Some((gfx, controls, skin)) = deferred {
        if let Some((key, layout)) = skin {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = crate::noteskins::bundled().await;
                crate::noteskins::prefetch(&key, &layout);
            });
        }
        gloo::timers::callback::Timeout::new(DEFERRED_WARM_MS, move || {
            warm_preview(&gfx, &controls)
        })
        .forget();
    }
}

/// How long after the shell's fade a skin put off is readied on the
/// preview: after the first screen's entrance.
const DEFERRED_WARM_MS: u32 = 1000;
