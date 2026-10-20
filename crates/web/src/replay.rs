//! Replays on the web (`docs/plans/play-history.md`, Replays): the play
//! route's `replay=<play id>`. [`ReplayLoader`] reads the kept play,
//! finds its chart (where the record says, else any song with a chart of
//! its content), checks the skin it was drawn with is still installed,
//! and puts the play screen up for it ([`GameCanvas`] with a
//! [`ReplayProp`]): the start prompt, then the play as it was, its log
//! for the lanes; or, refused, the prompt saying why. `view=results`
//! shows the play's results instead (judged again from its log).
//! `window.__DDI_DEBUG.replay` says what it is at.

use std::rc::Rc;

use ddi_platform::history::PlayEntry;
use ddi_ui::state::history::{Replay, ReplayCheck};
use js_sys::{Array, Object, Reflect};
use wasm_bindgen::JsValue;

use crate::component::{Callback, Component, Context, Mounted, mount};
use crate::components::game_canvas::{self, GameCanvas, SongSource};
use crate::settings::Settings;
use crate::songs::LoadError;
use crate::web::gfx::BackendPreference;

/// A kept play for the play screen: what it replays, or why it cannot.
#[derive(Debug, PartialEq)]
pub(crate) struct ReplayProp {
    pub(crate) replay: Option<Replay>,
    /// Why it cannot be watched: the prompt says so, nothing loads.
    pub(crate) refused: Option<String>,
    /// Its results, not its replay.
    pub(crate) view_results: bool,
    /// The skin it was drawn with was looked for (not when its folder
    /// waited for access: the play screen looks once the song is read).
    pub(crate) skin_checked: bool,
}

impl ReplayProp {
    /// The settings it is drawn with on this device
    /// (`Replay::settings`; the stored ones when refused).
    pub(crate) fn settings(&self) -> Settings {
        let here = Settings::load();
        match &self.replay {
            Some(r) => r.settings(&here),
            None => here,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct Props {
    pub(crate) id: String,
    pub(crate) view_results: bool,
    pub(crate) backend: BackendPreference,
    pub(crate) slow: bool,
    pub(crate) restart: Callback<()>,
}

pub(crate) enum Msg {
    Ready {
        prop: Rc<ReplayProp>,
        song: String,
        chart: usize,
    },
}

/// Reads the play, then shows the play screen for it.
pub(crate) struct ReplayLoader {
    canvas: Option<Mounted<GameCanvas>>,
}

impl Component for ReplayLoader {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link().clone();
        let view_results = ctx.props().view_results;
        publish(&ctx.props().id, "loading", None, None);
        crate::history::entry(&ctx.props().id, move |entry| {
            wasm_bindgen_futures::spawn_local(async move {
                let (prop, song, chart) = resolve(entry, view_results).await;
                link.send_message(Msg::Ready {
                    prop: Rc::new(prop),
                    song,
                    chart,
                });
            });
        });
        ReplayLoader { canvas: None }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Msg) -> bool {
        let Msg::Ready { prop, song, chart } = msg;
        publish(
            &ctx.props().id,
            if prop.refused.is_some() {
                "refused"
            } else {
                "ready"
            },
            prop.refused.as_deref(),
            None,
        );
        let p = ctx.props();
        // Beside this component's (empty) element, as the app mounts it.
        let parent = ctx
            .root()
            .parent_element()
            .unwrap_or_else(|| ctx.root().clone().into());
        self.canvas = Some(mount::<GameCanvas>(
            &parent,
            "div",
            game_canvas::Props {
                restart: p.restart.clone(),
                source: SongSource::Song { id: song, chart },
                auto: false,
                auto_pad: false,
                auto_bias: 0.0,
                slow: p.slow,
                backend: p.backend,
                replay: Some(prop),
            },
        ));
        false
    }

    fn view(&self, ctx: &Context<Self>) {
        ctx.root().set_hidden(true);
    }
}

/// The play behind `entry` made ready: its record and log read, its
/// chart found (its song and index), the skin it was drawn with looked
/// for; or why it cannot be replayed.
async fn resolve(
    entry: Result<PlayEntry, String>,
    view_results: bool,
) -> (ReplayProp, String, usize) {
    let refused = |why: String| {
        (
            ReplayProp {
                replay: None,
                refused: Some(why),
                view_results,
                skin_checked: true,
            },
            String::new(),
            0,
        )
    };
    let replay = match entry.and_then(|e| Replay::from_entry(&e)) {
        Ok(r) => r,
        Err(e) => return refused(e),
    };
    let key = replay.record.chart.clone();
    let (mut song_id, mut chart) = (key.song.clone(), key.index as usize);
    let mut layout = None;
    let mut refusal = None;
    // Where the record says; a chart of its content elsewhere (a pack
    // imported again from another folder has new song ids).
    let elsewhere = || async {
        let other = crate::songs::find_by_digest(&[key.digest]).await?;
        let song = crate::songs::load_chart(&other).await.ok()?;
        let i = replay.chart(&song).ok()?;
        Some((other, i, song.charts[i].layout.clone()))
    };
    match crate::songs::load_chart(&song_id).await {
        Ok(song) => match replay.chart(&song) {
            Ok(i) => {
                chart = i;
                layout = Some(song.charts[i].layout.clone());
            }
            Err(e) => match elsewhere().await {
                Some((id, i, l)) => (song_id, chart, layout) = (id, i, Some(l)),
                None => refusal = Some(e),
            },
        },
        // A linked folder to allow again: the play screen asks, then
        // checks the chart once it is read.
        Err(LoadError::NeedsAccess { .. }) => {}
        Err(LoadError::Failed(_)) => match elsewhere().await {
            Some((id, i, l)) => (song_id, chart, layout) = (id, i, Some(l)),
            None => refusal = Some(Replay::missing_song()),
        },
    }
    let skin_checked = layout.is_some();
    if refusal.is_none()
        && let Some(layout) = layout
    {
        refusal = missing_skin(&replay, &layout).await;
    }
    (
        ReplayProp {
            replay: Some(replay),
            refused: refusal,
            view_results,
            skin_checked,
        },
        song_id,
        chart,
    )
}

/// Why `replay` cannot be drawn on a chart of layout `layout`: the skin
/// it was drawn with is not installed (bundled ones ship with the game).
pub(crate) async fn missing_skin(replay: &Replay, layout: &str) -> Option<String> {
    let settings = replay.settings(&Settings::load());
    let skin = replay.skin(&settings, layout)?;
    let db = crate::web::idb::Db::open().await.ok();
    let skins = crate::noteskins::catalogue(db.as_ref()).await;
    (!skins.iter().any(|s| s.key() == skin)).then(|| Replay::missing_skin(&skin))
}

/// `window.__DDI_DEBUG.replay`: `id`, `state` (`loading`, `ready`,
/// `refused`, `playing`, `ended`), `refused` (why), and once a replay
/// ended `check` (`matches`, `withRecord`, `problems`).
pub(crate) fn publish(id: &str, state: &str, refused: Option<&str>, check: Option<&ReplayCheck>) {
    let Some(debug) = crate::web::debug_object() else {
        return;
    };
    let set = |o: &Object, k: &str, v: JsValue| {
        let _ = Reflect::set(o, &k.into(), &v);
    };
    let o = Object::new();
    set(&o, "id", id.into());
    set(&o, "state", state.into());
    set(&o, "refused", refused.map_or(JsValue::NULL, JsValue::from));
    if let Some(c) = check {
        let k = Object::new();
        set(&k, "matches", c.matches.into());
        set(&k, "withRecord", c.with_record.into());
        let p = Array::new();
        for x in &c.problems {
            p.push(&x.as_str().into());
        }
        set(&k, "problems", p.into());
        set(&o, "check", k.into());
    }
    set(&debug, "replay", o.into());
}

/// `__DDI_DEBUG.replay`'s edges fed so far (`fed`, `edges`), during a
/// replay.
pub(crate) fn progress(fed: usize, edges: usize) {
    let Some(o) = web_sys::window()
        .and_then(|w| Reflect::get(&w, &"__DDI_DEBUG".into()).ok())
        .and_then(|d| Reflect::get(&d, &"replay".into()).ok())
        .filter(JsValue::is_object)
    else {
        return;
    };
    let _ = Reflect::set(&o, &"fed".into(), &(fed as f64).into());
    let _ = Reflect::set(&o, &"edges".into(), &(edges as f64).into());
}
