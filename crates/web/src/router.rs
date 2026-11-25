//! Hash routes. The site lives under a sub-path on GitHub Pages, so routing
//! uses the fragment: `#/` (Start), `#/select` (Songs), `#/play?song=<id>&chart=<n>`, `#/options`,
//! `#/calibrate`, `#/credits`, `#/results`, `#/edit?song=<id>&chart=<n>`,
//! `#/video-check`, `#/skin-preview`, `#/ui-test`, `#/ui-dev`; the
//! multiplayer prototype's `#/mp…` and `#/room` (behind no switch since
//! 2026-11-14).

pub(crate) use crate::route::{PlayRoute, Route, has_bars};
use crate::route::{fallback_parent, param_name, params, percent_decode, split_hash};
use ddi_platform::storage::{self, Area};

impl Route {
    /// The route's address, with the testing parameters kept for the
    /// session ([`testing`]).
    pub(crate) fn to_hash(&self) -> String {
        self.to_hash_with(&testing())
    }

    pub(crate) fn current() -> Route {
        let hash = web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default();
        // The page's first address: its testing parameters taken
        // whatever its route (`perf`, `auto`, `mpcpus`…); later ones only
        // where each has always been read.
        let at_load = !LOADED.with(|l| l.replace(true));
        keep_testing_at(&hash, at_load);
        share_switch(&hash);
        if at_load {
            restore_chat(&hash);
        }
        multiplayer_params(&hash, at_load);
        lang_switch(&hash);
        Route::parse(&hash)
    }

    pub(crate) fn navigate(&self) {
        navigate(&self.to_hash());
    }

    /// Return to an existing caller, or replace a direct deep link.
    pub(crate) fn return_to(&self) {
        visit(&self.to_hash(), true);
    }
}

/// The names of the testing parameters kept for the session.
pub(crate) const TESTING_KEYS: [&str; 4] = ["gfx", "auto", "bias", "slow"];

thread_local! {
    /// The testing parameters ([`TESTING_KEYS`]) the page was opened
    /// with, as written.
    static TESTING: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The testing parameters a song select or play address carries, kept
/// for the session: song select passes them to the plays it starts, and
/// the way back keeps them.
///
/// Song select's own address says whether its plays autoplay: `auto`
/// there (or its absence) replaces what was kept, and the switch follows
/// it again (the switch rewrites that address when used,
/// [`set_autoplay`]).
pub(crate) fn keep_testing(hash: &str) {
    keep_testing_at(hash, false);
}

/// [`keep_testing`]; `at_load`: the page's first address, whose testing
/// parameters are taken on any route (`#/options?perf=1`,
/// `#/mp?auto=1`), not only on the start route, song select's, a play's
/// and calibration's.
fn keep_testing_at(hash: &str, at_load: bool) {
    let (path, query) = hash
        .trim_start_matches('#')
        .split_once('?')
        .unwrap_or((hash.trim_start_matches('#'), ""));
    let path = path.trim_start_matches('/');
    if !at_load && !matches!(path, "" | "select" | "play" | "calibrate") {
        return;
    }
    crate::perf::note_address(query);
    // Another route's at load: kept as the start route's are.
    let path = if matches!(path, "" | "select" | "play" | "calibrate") {
        path
    } else {
        ""
    };
    // The chart editor's playtest, the announcer studio's scenes and a
    // practice room's stages say nothing about the session's plays
    // (their `auto` is their own).
    if path == "play"
        && query
            .split('&')
            .any(|kv| kv == "edit=1" || kv.starts_with("scene=") || kv.starts_with("stage="))
    {
        return;
    }
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| TESTING_KEYS.iter().any(|k| *k == param_name(kv)))
        .collect();
    if path == "select" || path.is_empty() && !kept.is_empty() {
        TESTING.with(|t| {
            let mut t = t.borrow_mut();
            let mut parts: Vec<String> = t
                .split('&')
                .filter(|kv| !kv.is_empty() && kv.split('=').next() != Some("auto"))
                .filter(|kv| {
                    // What the address gives again replaces the old.
                    let k = kv.split('=').next();
                    !kept.iter().any(|n| n.split('=').next() == k)
                })
                .map(String::from)
                .collect();
            parts.extend(kept.iter().map(|s| s.to_string()));
            *t = parts.join("&");
        });
        return;
    }
    if !kept.is_empty() {
        TESTING.with(|t| *t.borrow_mut() = kept.join("&"));
    }
}

/// `?lang=TAG` on any address: the UI's language for the session (`ja`,
/// the pseudo-languages `qps-ploc` and `qps-wide`), whatever the
/// settings say, until one is chosen in Options; `lang=` alone gives the
/// choice back to the settings. Pages built after it speak it.
fn lang_switch(hash: &str) {
    let Some(query) = split_hash(hash).1 else {
        return;
    };
    for tag in params(query).filter_map(|(k, v)| v.filter(|_| k == "lang")) {
        if tag.is_empty() {
            ddi_l10n::unforce();
            crate::settings::Settings::load().choose_language();
            crate::ratings::kick();
        } else if !ddi_l10n::force(tag) {
            web_sys::console::warn_1(&format!("lang={tag}: no such language").into());
        }
    }
}

/// The hidden switch of sharing by link (`docs/plans/shared-songs.md`):
/// `#/?share=1` on the start route turns sharing on for the page,
/// `share=0` off; never a row of Options. Never stored (round HY,
/// 2026-11-25: a link must not change a stranger's settings): what is
/// stored is `Settings::sharing`, kept on once a token is set in
/// Options' Sharing section and off once it is cleared ([`sharing`]).
fn share_switch(hash: &str) {
    let (path, Some(query)) = split_hash(hash) else {
        return;
    };
    if !path.trim_start_matches('/').is_empty() {
        return;
    }
    let Some(on) = params(query).find_map(|kv| match kv {
        ("share", Some("1")) => Some(true),
        ("share", Some("0")) => Some(false),
        _ => None,
    }) else {
        return;
    };
    SHARING.with(|s| s.set(Some(on)));
}

/// Whether sharing by link is on: as an address gave it for the page
/// ([`share_switch`]), else as stored.
pub(crate) fn sharing(settings: &crate::settings::Settings) -> bool {
    SHARING.with(std::cell::Cell::get).unwrap_or(settings.sharing)
}

/// `sessionStorage`: TESTING ONLY, the scripted chat's `mpfakechat`,
/// `mpfakechatimg` and `mpfakechatlook` as last given
/// (`<what>\n<images>\n<look>`), for the tab's session.
const CHAT_KEPT: &str = "ddi.mpfakechat";

/// The page loaded on an address without `mpfakechat`: the scripted chat
/// the tab's session was given before (a reload, the way back from a trip
/// through Twitch), as it was given.
fn restore_chat(hash: &str) {
    let given = split_hash(hash)
        .1
        .is_some_and(|q| params(q).any(|(k, _)| k == "mpfakechat"));
    if given {
        return;
    }
    if let Some(kept) = storage::get(Area::Session, CHAT_KEPT) {
        let mut lines = kept.split('\n');
        let what = lines.next().unwrap_or_default();
        let images = lines.next().unwrap_or_default();
        let look = lines.next().unwrap_or_default();
        let socket = lines.next().unwrap_or_default();
        crate::chat::script(
            what,
            (!images.is_empty()).then_some(images),
            (!socket.is_empty()).then_some(socket),
        );
        crate::chat::look((!look.is_empty()).then_some(look));
    }
}

/// The multiplayer prototype's testing parameters
/// (`docs/plans/multiplayer-prototype-progress.md`), read from the address
/// the page loads on, whatever its route, and from the start route's or
/// an invite's link later; never a row of Options. Multiplayer is no
/// longer behind a switch (2026-11-14): `mp=1` and `mp=0`, which older
/// links carry, are read by nothing. DEVELOPMENT ONLY: `mpcpus=N`, practice against N CPUs
/// beyond the cap (`crate::room::develop`), and `mpdevmines=BEAT`, a
/// section of battle shocks (M8a); a convenience, `mpattacks=0`: the next
/// practice room starts with its Attacks rule Off, which anyone sets in
/// Rules (`mpattacks=1`, On, is the default); hidden options for the
/// page alone, never stored (round HY, 2026-11-25), `mprules=1`, a room's
/// Rules opening by themselves on entering
/// (`Settings::multiplayer_rules_first`), and `mpwatchlink=1`, More's Copy
/// watch-only link (`Settings::multiplayer_watch_link`); TESTING ONLY,
/// `mpfakeseat=1` (`mpfakeend=S`, `mpfakematch=STAGES,CUT,WAIT`) and
/// `mpfakeroom=S` (`mpfakeready=R`, `mpfakelive=1`, `mpfakefail=N`,
/// `mpfakeslow=S`, `mpfakeelsewhere=S`), and `mpfakechat=<name or
/// rate>` (`mpfakechatimg=<path>`), a scripted chat beside the plays. Each
/// address is read once.
fn multiplayer_params(hash: &str, at_load: bool) {
    let (path, Some(query)) = split_hash(hash) else {
        return;
    };
    let path = path.trim_start_matches('/');
    // At load any route; later the start route, or an invite's link.
    if !at_load
        && !path.is_empty()
        && !path.starts_with("mp/join/")
        && !path.starts_with("mp/audience/")
    {
        return;
    }
    // Once an address: a route read again (a page built again) does not
    // make its practice room again.
    let seen = MULTIPLAYER_READ.with(|r| r.replace(hash.to_string()));
    if seen == hash {
        return;
    }
    let param =
        |key: &str| params(query).find_map(|(k, v)| v.filter(|_| k == key).map(str::to_string));
    // DEVELOPMENT ONLY: `mpcpus=N` makes the next practice room one
    // against N CPUs, beyond the cap (M6b's miniature fields).
    if let Some(n) = param("mpcpus").and_then(|n| n.parse::<u16>().ok()) {
        crate::room::develop(n);
    }
    // `mpattacks=0`: a convenience for tests and screenshots, the next
    // practice room starts with its Attacks rule Off. DEVELOPMENT ONLY
    // (M8a): `mpdevmines=BEAT` puts a section of battle shocks into each
    // stage's play from that beat (`ddi_ui::state::room::Development`).
    let attacks = param("mpattacks").map(|v| v == "1");
    let mines = param("mpdevmines").map(|v| v.parse::<u32>().ok());
    if attacks.is_some() || mines.is_some() {
        let mut d = ddi_ui::state::room::development();
        if let Some(a) = attacks {
            d.attacks = Some(a);
        }
        if let Some(m) = mines {
            d.mines_at_beat = m;
        }
        ddi_ui::state::room::set_development(d);
        crate::room::discard();
    }
    // TESTING ONLY: `mpfakeseat=1`, the next audience seat is one in a
    // fake room's audience in the page (`crate::fake_seat`: the headless
    // browsers' look at a watched stage without a server), `mpfakeend=S`
    // closing that audience S seconds into its stage, and
    // `mpfakematch=STAGES,CUT,WAIT` its match (stages, each cut to CUT
    // seconds, the picks WAIT seconds apart).
    if param("mpfakeseat").as_deref() == Some("1") {
        crate::fake_seat::want(
            param("mpfakeend").and_then(|s| s.parse::<f64>().ok()),
            param("mpfakematch").map(|s| crate::fake_seat::MatchPlan::parse(&s)),
        );
    }
    // TESTING ONLY: `mpfakeroom=S`, online room `Room000001` is a fake
    // room in the page whose scripted owner picks a stage S seconds after
    // the player joined (`crate::fake_room`: the headless browsers' look
    // at a member told their stage is due, without a server);
    // `mpfakeready=R`, she readies R seconds after she has the stage (the
    // room waiting for her meanwhile: a ready player's Ready screen).
    // TESTING ONLY (round TW2): `mpfakechat=<name or rate>`, a scripted
    // chat beside every play where Options → Display's Twitch chat in
    // rooms is on (`crate::chat`), its images from `mpfakechatimg=<path>`
    // where given; `mpfakechat=0` stops it.
    // Kept for the tab's session (round SF): a reload, and the trips
    // through Twitch (signing in, Twitch access), come back to an address
    // without it ([`restore_chat`]).
    // Round TW5: `mpfakechatlook=<as-built|larger|scattered>`, the chat
    // drawn in one of the looks shown to the maintainer.
    // Round TW6: `mpfakechat=live`, the live chat's development script
    // (the signed-in person's own channel in any play), its socket at
    // `mpfakechaturl=<ws address>` where given (a local fake, never
    // Twitch's for a test), the providers' addresses from `mpfakechatimg`.
    if let Some(what) = param("mpfakechat") {
        let images = param("mpfakechatimg");
        let look = param("mpfakechatlook");
        let socket = param("mpfakechaturl");
        crate::chat::script(&what, images.as_deref(), socket.as_deref());
        crate::chat::look(look.as_deref());
        // Round DE: `mpfakechatrules=v1`, the retired limits, for the
        // frame-cost comparison (not kept for the session).
        crate::chat::rules(param("mpfakechatrules").as_deref());
        if what == "0" {
            storage::remove(Area::Session, CHAT_KEPT);
        } else {
            let kept = format!(
                "{what}\n{}\n{}\n{}",
                images.unwrap_or_default(),
                look.unwrap_or_default(),
                socket.unwrap_or_default()
            );
            let _ = storage::set(Area::Session, CHAT_KEPT, &kept);
        }
    }
    if let Some(after) = param("mpfakeroom").and_then(|s| s.parse::<f64>().ok()) {
        crate::fake_room::want(after);
    }
    if let Some(after) = param("mpfakeready").and_then(|s| s.parse::<f64>().ok()) {
        crate::fake_room::ready_after(after);
    }
    // `mpfakelive=1`, the fake room plays live (as a hosted room does:
    // its stage's song zero on the room's clock, a late start's late join,
    // the streams).
    if param("mpfakelive").as_deref() == Some("1") {
        crate::fake_room::live();
    }
    // `mpfakefail=N`, the player's first N downloads of a stage's song
    // fail (its retries, then the Retry beside the status line);
    // `mpfakeslow=S`, each takes S seconds; `mpfakeelsewhere=S`, S seconds
    // after a pick the server closes the player's socket 4429 (a fifth
    // device of theirs: the notice and Use this device; away after 5 s).
    if let Some(n) = param("mpfakefail").and_then(|s| s.parse::<u32>().ok()) {
        crate::fake_room::fail_downloads(n);
    }
    if let Some(s) = param("mpfakeslow").and_then(|s| s.parse::<f64>().ok()) {
        crate::fake_room::slow_downloads(s);
    }
    if let Some(s) = param("mpfakeelsewhere").and_then(|s| s.parse::<f64>().ok()) {
        crate::fake_room::opened_elsewhere(s);
    }
    let flag = |key: &str| match param(key).as_deref() {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    };
    // For the page alone, never stored (round HY): an invite link
    // carrying one changes nothing of the player's settings.
    ddi_ui::state::settings::give_hidden(flag("mprules"), flag("mpwatchlink"));
}

thread_local! {
    /// Sharing by link as an address gave it for the page ([`sharing`]).
    static SHARING: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    /// The last address [`multiplayer_params`] read.
    static MULTIPLAYER_READ: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// The page's first address has been read ([`Route::current`]).
    static LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A parameter of the current address's query, if there.
pub(crate) fn query_param(key: &str) -> Option<String> {
    let hash = web_sys::window()?.location().hash().ok()?;
    params(split_hash(&hash).1?)
        .find_map(|(k, v)| (k == key).then(|| v.map(percent_decode).unwrap_or_default()))
}

/// The testing parameters kept ([`keep_testing`]), `&`-joined.
pub(crate) fn testing() -> String {
    TESTING.with(|t| t.borrow().clone())
}

/// `hash` with the testing parameters kept among `keys` added, as they
/// were written.
pub(crate) fn with_testing(mut hash: String, keys: &[&str]) -> String {
    for kv in testing().split('&') {
        if keys.contains(&param_name(kv)) {
            hash.push('&');
            hash.push_str(kv);
        }
    }
    hash
}

/// Whether plays started from song select are autoplayed (`auto=` in
/// the address).
pub(crate) fn autoplay() -> bool {
    testing()
        .split('&')
        .any(|kv| kv.starts_with("auto=") && kv != "auto=0")
}

const NAVIGATION: &str = "ddiNavigation";

/// Clone rather than replace the host's overlay markers.
pub(crate) fn history_state() -> js_sys::Object {
    let state = web_sys::window()
        .and_then(|w| w.history().ok())
        .and_then(|h| h.state().ok());
    let object = js_sys::Object::new();
    if let Some(state) = state.filter(|s| s.is_object()) {
        js_sys::Object::assign(&object, &js_sys::Object::from(state));
    }
    object
}

fn hash() -> String {
    web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "#/".into())
}

fn trail() -> ddi_ui::navigation::RouteTrail {
    let current = hash();
    let stored = js_sys::Reflect::get(&history_state(), &NAVIGATION.into())
        .ok()
        .and_then(|v| v.as_string())
        .and_then(|s| serde_json::from_str::<ddi_ui::navigation::RouteTrail>(&s).ok());
    stored
        .filter(|t| t.routes.last() == Some(&current))
        .unwrap_or_else(|| ddi_ui::navigation::RouteTrail::at(current))
}

/// Pushes an entry of the address shown over its own, marked `key`, its
/// trail one longer (the way back from it counts it): a page's guard of
/// the browser's Back (a room's stage's play). Whether it was pushed.
pub(crate) fn push_guard(key: &str) -> bool {
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return false;
    };
    let state = history_state();
    put_trail(&state, &trail().push(hash()));
    let _ = js_sys::Reflect::set(&state, &key.into(), &true.into());
    history.push_state_with_url(&state, "", None).is_ok()
}

fn put_trail(state: &js_sys::Object, trail: &ddi_ui::navigation::RouteTrail) {
    if let Ok(text) = serde_json::to_string(trail) {
        let _ = js_sys::Reflect::set(state, &NAVIGATION.into(), &text.into());
    }
}

fn changed() {
    history_changed();
    if let Some(w) = web_sys::window()
        && let Ok(event) = web_sys::Event::new("hashchange")
    {
        let _ = w.dispatch_event(&event);
    }
}

pub(crate) fn navigate(next: &str) {
    visit(next, false);
}

fn visit(next: &str, returning: bool) {
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let current = trail();
    // Already there (an address differing only in what the route does
    // not read, as testing parameters): nothing to do. Matched below as
    // the trail's own last entry, it would be `history.go(0)`, a reload.
    let here = hash();
    if next == here || Route::parse(next) == Route::parse(&here) {
        return;
    }
    // An explicit return to an ancestor (including song/results returns)
    // consumes its intervening visits instead of adding a history loop.
    if let Some(i) = current
        .routes
        .iter()
        .rposition(|s| Route::parse(s) == Route::parse(next))
    {
        let _ = history.go_with_delta(i as i32 - current.routes.len() as i32 + 1);
        return;
    }
    if returning {
        let state = js_sys::Object::new();
        put_trail(&state, &ddi_ui::navigation::RouteTrail::at(next.into()));
        if history
            .replace_state_with_url(&state, "", Some(next))
            .is_ok()
        {
            changed();
        }
        return;
    }
    let state = history_state();
    put_trail(&state, &current);
    let _ = history.replace_state_with_url(&state, "", None);
    let next_state = js_sys::Object::new();
    put_trail(&next_state, &current.push(next.into()));
    if history
        .push_state_with_url(&next_state, "", Some(next))
        .is_ok()
    {
        changed();
    }
}

pub(crate) fn back() {
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let current = trail();
    if current.parent().is_some() {
        let _ = history.back();
        return;
    }
    let parent = fallback_parent(Route::current()).to_hash();
    let state = js_sys::Object::new();
    put_trail(&state, &ddi_ui::navigation::RouteTrail::at(parent.clone()));
    if history
        .replace_state_with_url(&state, "", Some(&parent))
        .is_ok()
    {
        changed();
    }
}

/// Where [`back`] goes from here: the trail's last route before this one,
/// else the route's own parent.
pub(crate) fn back_target() -> String {
    match trail().parent() {
        Some(parent) => parent.to_string(),
        None => fallback_parent(Route::current()).to_hash(),
    }
}

/// A fresh launch or reload starts the title menu without focus. History
/// still restores its focus when returning from another route in this run.
pub(crate) fn reset_start_focus() {
    if Route::parse(&hash()) != Route::Home {
        return;
    }
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let state = history_state();
    if js_sys::Reflect::has(&state, &"ddiFocus".into()).unwrap_or(false) {
        let _ = js_sys::Reflect::delete_property(&state, &"ddiFocus".into());
        let _ = history.replace_state_with_url(&state, "", None);
        history_changed();
    }
}

pub(crate) fn saved_focus() -> Option<String> {
    js_sys::Reflect::get(&history_state(), &"ddiFocus".into())
        .ok()?
        .as_string()
}

pub(crate) fn saved_options() -> Option<(String, String, f32)> {
    let text = js_sys::Reflect::get(&history_state(), &"ddiOptions".into())
        .ok()?
        .as_string()?;
    serde_json::from_str(&text).ok()
}

/// What [`save_view`] last wrote, and into which history entry (the
/// [`history_changed`] count then).
type Saved = (u64, Option<String>, Option<(String, String, f32)>);

thread_local! {
    /// Bumped each time the history's entry may have changed: a visit, a
    /// return, a pop, a hash change, a dialog's marker.
    static GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static SAVED: std::cell::RefCell<Option<Saved>> = const { std::cell::RefCell::new(None) };
    /// When the view is written (`ddi_ui::back_stack::ViewSaves`: four
    /// times a second at most), what is held back meanwhile, and the
    /// page's going away, which writes it.
    static SAVES: std::cell::RefCell<ddi_ui::back_stack::ViewSaves> =
        std::cell::RefCell::new(Default::default());
    static HELD: std::cell::RefCell<Option<Saved>> = const { std::cell::RefCell::new(None) };
    static ON_HIDE: std::cell::RefCell<Option<gloo::events::EventListener>> =
        const { std::cell::RefCell::new(None) };
}

/// The history's entry may be another now: the next [`save_view`] writes.
pub(crate) fn history_changed() {
    GENERATION.with(|g| g.set(g.get() + 1));
}

/// Keeps the focus and Options' place in the history's entry, written
/// only when they changed or the entry did (called every frame), and four
/// times a second at most (round Y3: Firefox refuses a page's history
/// changes past about 200 in 10 s, which a key held down a long page
/// reached): what changes meanwhile is held and written at the first
/// frame it may be, so the view at rest is always written; `must`, at
/// once (before a navigation, a dialog's entry pushed); never while a
/// `go` of ours is on its way (`busy`, `ddi_ui::back_stack::Entries`); and
/// what is held when the page goes away.
pub(crate) fn save_view(
    focus: Option<&str>,
    theme: Option<&ddi_ui::screens::ThemeState>,
    busy: bool,
    must: bool,
) {
    let options = theme
        .filter(|t| t.screen == "options")
        .map(|t| (t.section.clone(), t.option_id.clone(), t.scroll));
    let now: Saved = (
        GENERATION.with(|g| g.get()),
        focus.map(str::to_string),
        options,
    );
    if SAVED.with_borrow(|s| s.as_ref() == Some(&now)) {
        HELD.with_borrow_mut(|h| *h = None);
        return;
    }
    let t = web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now() / 1000.0);
    if !SAVES.with_borrow_mut(|s| s.may_write(t, busy, must)) {
        HELD.with_borrow_mut(|h| *h = Some(now));
        ON_HIDE.with_borrow_mut(|l| {
            if l.is_none()
                && let Some(w) = web_sys::window()
            {
                *l = Some(gloo::events::EventListener::new(&w, "pagehide", |_| {
                    if let Some(held) = HELD.with_borrow_mut(Option::take) {
                        write_view(held);
                    }
                }));
            }
        });
        return;
    }
    HELD.with_borrow_mut(|h| *h = None);
    write_view(now);
}

/// [`save_view`]'s write of `now` into the history's entry.
fn write_view(now: Saved) {
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let state = history_state();
    let mut changed = false;
    for (key, value) in [
        ("ddiFocus", now.1.clone()),
        (
            "ddiOptions",
            now.2.as_ref().and_then(|o| serde_json::to_string(o).ok()),
        ),
    ] {
        if let Some(value) = value
            && js_sys::Reflect::get(&state, &key.into())
                .ok()
                .and_then(|v| v.as_string())
                .as_deref()
                != Some(&value)
        {
            let _ = js_sys::Reflect::set(&state, &key.into(), &value.into());
            changed = true;
        }
    }
    if changed {
        let _ = history.replace_state_with_url(&state, "", None);
    }
    SAVED.with_borrow_mut(|s| *s = Some(now));
}

