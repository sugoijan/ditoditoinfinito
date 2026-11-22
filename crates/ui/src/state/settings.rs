//! The player's settings (`docs/plans/ui-core.md`, P12), kept under
//! `ddi.settings.v1` in the installed store ([`ddi_platform::storage`]):
//! on the web `localStorage`, as JSON, as they always were, so players keep
//! theirs; older shapes migrate on load ([`Settings::load`]).

mod hand;
pub use hand::HandLayout;

use std::collections::BTreeMap;

pub use ddi_library::generative::{Look, Palette};

pub use ddi_engine::PadBindings;
use ddi_engine::clock::ClockOptions;
use ddi_engine::player::PlayOptions;
use ddi_engine::rules::StepManiaOptions;
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::ddi::DdiOptions;
use ddi_engine::rules::presets::{HoldStyle, RulesetMode};
use ddi_engine::scroll::{ScrollAction, ScrollOptions, SpeedMod, SpeedSource};
use ddi_engine::{Appearance, ControlBindings, FieldPlace, LaneCover, TransformOptions};
use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

/// Where the settings are kept.
pub const KEY: &str = "ddi.settings.v1";

/// The ruleset new settings play by: DDI, the game's own.
pub const DEFAULT_RULESET: &str = "ddi";
/// The default before it, which settings naming it moved from once.
const OLD_DEFAULT_RULESET: &str = "itg";

/// The name [`Settings::defaults_reset_2026_11_17`] is stored by, which
/// the web page's loader reads (`site/boot.js`).
pub const DEFAULTS_RESET: &str = "defaults_reset_2026_11_17";

/// The name [`Settings::announcer_moved`] is stored by.
pub const ANNOUNCER_MOVED: &str = "announcer_moved";
/// The announcer new settings have: the built-in one.
pub const BUILT_IN_ANNOUNCER: &str = ddi_library::announcer::BUILT_IN;
/// The bundled packs the built-in announcer replaced (2026-11-17): Mr.
/// Clank's and ガキンさん's keys (escaped: data, never drawn).
const OLD_BUNDLED_ANNOUNCERS: [&str; 2] = [
    "bundled:mr. clank",
    "bundled:\u{30ac}\u{30ad}\u{30f3}\u{3055}\u{3093}",
];
/// Announcers whose values are kept, at most.
const MAX_ANNOUNCERS_WITH_PARAMS: usize = 64;

/// What a chart's number is ([`Settings::chart_ratings`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartRatings {
    /// The number the song's file gives it (its radar the song's own
    /// Groove values).
    FromSong,
    /// MSD where the chart has one (four lanes), else the song's.
    Msd,
    /// The DDI estimate where the chart has one (an original Single chart
    /// inside what the estimate is qualified for), else the song's (a try
    /// since round Z2, `docs/plans/msd-ratings.md`). Stored as
    /// `"Estimate"`; a build before it reads that as an unknown value,
    /// `ddi_engine::lenient`'s default, the song's number (and keeps that
    /// once it saves the settings). The default since round SLB
    /// (2026-11-20, "DDI estimate now"; [`Settings::ratings_moved`]).
    #[default]
    Estimate,
}

impl ChartRatings {
    fn is_default(&self) -> bool {
        *self == ChartRatings::Estimate
    }
}

/// The start prompt's countdowns Options offers, seconds (0: off).
pub const START_COUNTDOWNS: [f32; 4] = [0.0, 3.0, 5.0, 10.0];

/// Song offsets closer to zero than this (half a millisecond) are dropped.
const SONG_OFFSET_EPSILON: f64 = 0.0005;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// X-mod multiplier.
    pub speed: f64,
    /// The speed above everywhere, or a chart's own when it has one.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub speed_source: SpeedSource,
    pub reverse: bool,
    /// Where the field sits across the screen: in the middle, or on a
    /// side (drawing only).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub field_place: FieldPlace,
    /// Boost / Brake / Wave: how notes move on their way to the receptors.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub scroll_action: ScrollAction,
    /// Turn and simplifications applied to the chart of every song (not to
    /// calibration). Simplified plays are marked as assisted.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub transform: TransformOptions,
    /// Offer additional DanOni charts without their onigiri lane.
    pub simplify_onigiri: bool,
    /// Which of the dance charts made from pump charts without their
    /// centers the song list offers (those of difficulties a style has no
    /// chart of, by default; settings stored before it was kept get that,
    /// the struct's defaults filling what they lack, and its old on and
    /// off read as that and none).
    #[serde(deserialize_with = "center_versions")]
    pub simplify_center: CenterVersions,
    /// Offer pump single charts converted to Dancing☆Onigiri's 5 keys.
    pub convert_pump_single: bool,
    /// Offer Dancing☆Onigiri 5-key charts converted to pump single.
    pub convert_five_keys: bool,
    /// Hidden / Sudden / Stealth (drawing only).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub appearance: Appearance,
    /// Lane covers (Sudden+, Hidden+; drawing only), their heights as a
    /// play last left them ([`Settings::keep_cover_heights`]).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub lane_cover: LaneCover,
    /// The import report also lists details for pack authors:
    /// inconsistencies that make no difference in play.
    pub import_details: bool,
    /// Copy a picked folder's songs into the browser even where they can
    /// be linked (Chromium), so they survive the folder moving.
    pub import_copy: bool,
    /// Also copy background-change movies when songs are copied into the
    /// browser (they are most of a pack's size; on by default, since a
    /// copied song's movie left out never plays); linked songs read them
    /// from their folder either way.
    pub import_videos: bool,
    /// The noteskin layouts are drawn with: an imported one's key,
    /// `dance/<name>`, or a bundled one's, `bundled:dance/<name>`; `None`:
    /// the default SVG Infinito ([`Settings::play_skin`]). Older procedural and
    /// Spiral Base choices migrate to Infinito on load.
    pub noteskin: Option<String>,
    /// The pump noteskin pump layouts are drawn with (`pump/<name>`, an
    /// imported one's key); `None`: the noteskin above draws them too.
    pub pump_noteskin: Option<String>,
    /// The values set for skins' parameters
    /// (`docs/plans/skin-parameters.md`), by skin key (Infinito's, [`INFINITO`],
    /// whether chosen or not), then by name; a value equal to its default
    /// is not kept. Infinito's control onigiri, mine shapes, receptor fill and hit
    /// effects; old procedural controls are removed on load.
    pub skin_params: BTreeMap<String, Values>,
    /// Show the lyrics of songs that have them (Dancing☆Onigiri works, a
    /// simfile's `.lrc` file).
    pub lyrics: bool,
    /// Song background brightness, 0..=1; 0 turns the background off.
    pub bg_brightness: f32,
    /// Whether background-change movies play.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub video: VideoMode,
    /// Whether the movies of a `RandomMovies` folder play where StepMania
    /// 5.1 plays them by default (`docs/plans/phase-8-gaps.md`, step 1):
    /// behind a song without background changes, for a `-random-`
    /// change, in place of a file found nowhere.
    pub random_movies: bool,
    /// What a song without a background shows in its place, drawn from
    /// its music (`docs/plans/phase-8-gaps.md`, step 5): a look, or
    /// nothing (the plain ground). Never for a Dancing☆Onigiri work.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub generated_background: GeneratedBackground,
    /// The generated background over a song's own picture too, as a
    /// visualizer.
    pub generated_over_pictures: bool,
    /// The generated background's colors.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub generated_palette: Palette,
    /// The shape backgrounds and movies are fitted into.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub background_frame: BackgroundFrame,
    /// Darkening behind the lanes, one of [`FIELD_FILTERS`] (0 = off).
    pub field_filter: f32,
    /// Layout id the song list shows charts of (`dance-single`, …).
    pub style: String,
    /// Ruleset preset id (`ddi`, the default; `itg`, `sm5`, `ddr-a`).
    ///
    /// Every value is stored, so a default and a choice cannot be told
    /// apart: when the default moved from ITG to DDI, settings naming ITG
    /// moved wholesale ([`Settings::rules_moved`]). Someday the settings
    /// could keep whether a value is the default or was chosen ("X
    /// (default)" against "X (explicit)"), so a change of default moves
    /// only those who never chose (`docs/plans/ddi-ruleset.md`, "Normal
    /// play"); not built.
    pub ruleset: String,
    /// The move from ITG, the old default, to DDI was made (once: a
    /// player who chooses ITG after it keeps it). Settings stored before
    /// it lack the field and read `false`.
    #[serde(default = "not_yet")]
    pub rules_moved: bool,
    /// The defaults of 2026-11-17 were put back, once
    /// ([`DEFAULTS_RESET`]; `docs/plans/localization.md`, "Choosing the
    /// language"): the language back to the system's, the judgment words
    /// to English and the cursor to the Infinito cursor, whatever was
    /// stored (a default and a choice cannot be told apart, see
    /// `ruleset`). Settings stored before it lack the field and read
    /// `false`; a choice made after it stays. The web page's loader reads
    /// it too (`site/boot.js`: a stored language counts only once this is
    /// set).
    #[serde(default = "not_yet")]
    pub defaults_reset_2026_11_17: bool,
    /// The kept plays under these rules (a preset's id) wait to be judged
    /// again under the rules now chosen: set by the move from ITG, taken
    /// by the shells once the play history is read
    /// ([`crate::state::history::Plays::rejudge_asked`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejudge_from: Option<String>,
    /// DDI's options: its difficulty, Boos and Practice
    /// (`docs/plans/ddi-ruleset.md`).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub ddi: DdiOptions,
    /// What the play's HUD and the bests show of a DDI score.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub ddi_score: ScoreDisplay,
    /// DDI's judgment words in English (the default since 2026-11-17), in
    /// a language of their own or in the menus' language
    /// (`docs/plans/ddi-ruleset.md`, round 4): words only, never judging,
    /// a match rule or a best's key.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub judgment_words: JudgmentWords,
    /// Which rules each chart is played by (decision 23): the preset above
    /// for every chart by default.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub ruleset_mode: RulesetMode,
    /// Gauge, judge range and Excessive under the Dancing☆Onigiri rules.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub danoni: DanoniOptions,
    /// How the StepMania presets keep holds: ticked on pump charts by
    /// default (`docs/plans/pump.md`).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub holds: HoldStyle,
    /// The fail type and life and judge difficulty under the StepMania
    /// presets (`docs/plans/rules-options.md`).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub stepmania: StepManiaOptions,
    /// Seconds; positive = you hear the music later than the audio clock says.
    pub audio_offset: f64,
    /// Seconds added to the rendered time only.
    pub visual_offset: f64,
    /// Music gain, 0..=1 (amplitude, not loudness; the options slider maps
    /// it through [`volume_to_slider`]). [`DEFAULT_VOLUME`] until chosen.
    pub volume: f32,
    /// A clap on every row of notes, on the beat (StepMania's assist
    /// clap; off by default).
    pub assist_clap: bool,
    /// A sound when a mine or a shock is stepped on (on, as StepMania).
    pub mine_sound: bool,
    /// The announcer that speaks (`docs/plans/announcers.md`): its key,
    /// the name in lower case (`ddi_library::announcer::key`; an
    /// imported one on the web, one in the song folders on desktop), the
    /// built-in one's [`BUILT_IN_ANNOUNCER`] by default; `None`: off. One
    /// that is gone is let go for the built-in once the shell has listed
    /// them all ([`Settings::let_go_missing_announcer`]).
    pub announcer: Option<String>,
    /// The announcer's volume as a share of the music's (a gain, 0..=1;
    /// all of it by default, so muting the music mutes the voice).
    pub announcer_volume: f32,
    /// The values set for announcers' choices (their Language, the tags
    /// they let the player switch off, their other parameters), by
    /// announcer key, then by name; a value at its default is not kept
    /// (as [`Settings::skin_params`]; `ddi_library::announcer::pool`).
    pub announcer_params: BTreeMap<String, Values>,
    /// The move to the built-in announcer, on by default, was made once
    /// ([`ANNOUNCER_MOVED`]): settings stored before it with the
    /// announcer Off or with one of the two bundled packs it replaced
    /// got the built-in one at its default language; a choice of another
    /// pack stayed. Settings stored before it lack the field and read
    /// `false`; Off chosen after it stays Off.
    #[serde(default = "not_yet")]
    pub announcer_moved: bool,
    /// Song select says once that an announcer now speaks, with a way to
    /// turn it off (set by the move for a player who had it Off; taken
    /// by [`Settings::take_announcer_notice`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub announcer_notice: bool,
    /// Keyboard and controller bindings per layout (`keys`, `pads`; older
    /// `keys_single` is migrated on load).
    #[serde(flatten)]
    pub controls: ControlBindings,
    /// Poll controllers every millisecond during play; off polls once per
    /// frame (less CPU, coarser timing).
    pub fast_pad_poll: bool,
    /// Presentation only, keyed like controller bindings; absent means Auto.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub controller_styles: std::collections::BTreeMap<String, crate::controller_hints::Style>,
    /// The UI's language, a BCP 47 tag (`ja`); none: the system's
    /// (`ddi_l10n::choose`, `docs/plans/localization.md`).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub language: Option<String>,
    /// Reduced motion in the menus (no slides, the art held still): as the
    /// system says, or always, or never.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub reduced_motion: ReducedMotion,
    /// Menu pointer artwork; text and drag cursors keep their system shapes.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub cursor_style: CursorStyle,
    /// Longest artwork side in CSS/logical pixels, independent of UI zoom.
    pub cursor_size: f32,
    /// Seconds a song chosen in song select waits on its start prompt
    /// before it starts by itself; 0: it waits for Start
    /// ([`START_COUNTDOWNS`], `docs/plans/start-prompt.md`).
    pub start_countdown: f32,
    /// The results' panels the player chose ("Overview", "Timing",
    /// "Song"; one a ruleset does not offer falls back to its default).
    pub results_panels: String,
    /// Debug overlay: backend, FPS, clock drift, timing error stats.
    pub debug: bool,
    /// Which graphics backend the app starts on (read once, at boot).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub graphics: Graphics,
    /// Show the signed timing error of each hit under the judgement.
    pub show_deltas: bool,
    /// Show how far ahead of the chart's best play (or behind it) a play
    /// is, under the score (`docs/plans/play-history.md`, Replays).
    pub ghost: bool,
    /// Hand accompaniment; supported plays use at most two simultaneous presses.
    pub dancing_hand: bool,
    /// Whether the dancing hand also plays in a multiplayer stage (Options
    /// → Display, shown while the hand is on): the player's own choice,
    /// never a match rule. Off by default, and where a stored setting
    /// lacks it: a stage then prepares no hand at all
    /// ([`Self::dancing_hand_in`]).
    pub dancing_hand_multiplayer: bool,
    /// What a chart's number is (Options → Display): the song's own, or
    /// a rating of the game's (MSD for now, `docs/plans/msd-ratings.md`)
    /// where the chart has one, and the radar song select draws. The DDI
    /// estimate by default; left out of the stored settings at its
    /// default.
    #[serde(
        deserialize_with = "ddi_engine::lenient",
        skip_serializing_if = "ChartRatings::is_default"
    )]
    pub chart_ratings: ChartRatings,
    /// The default of Chart ratings moved from the song's numbers to the
    /// DDI estimate (once, round SLB: settings that still say the song's
    /// move; MSD stays; a choice made after it stays). Settings stored
    /// before it lack the field and read `false`. (The "Skill radar"
    /// option went with it: the radar is always shown, and a stored
    /// `skill_radar` is ignored.)
    #[serde(default = "not_yet")]
    pub ratings_moved: bool,
    /// A room's players panel on song select folded to its rail (L, its
    /// handle, More; `docs/plans/song-navigation.md`, "In a room"): this
    /// device's choice, open by default, never changed by itself.
    pub players_folded: bool,
    /// How many of a multiplayer stage's opponents have a field beside
    /// the play ([`OpponentFields`]): every one that fits, by default.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub opponent_fields: OpponentFields,
    /// Options → Controls, "Touch controls" (`docs/plans/touch-controls.md`):
    /// how a touch screen plays (`crate::touch_controls::Style`): buttons
    /// under the thumbs by default, the dance mat's pad, or the play
    /// screen's columns (legacy).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub touch_controls: crate::touch_controls::Style,
    /// "Extra button": where an odd layout's extra button goes, and a lone
    /// pad beside the field (right by default).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub touch_hand: crate::touch_controls::Hand,
    /// Options → Display, "Twitch chat in rooms" (`docs/plans/multiplayer-twitch.md`,
    /// C1): how much of a room host's Twitch chat this device shows behind
    /// the play, never above the room's ceiling. Off by default (it needs
    /// the device's own Twitch access to read chat).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub twitch_chat: ddi_chat::Level,
    /// Round TB: the Twitch permissions this device used (`bans`,
    /// `blocks`, `chat`) and when each was last used (milliseconds since
    /// the epoch), so that a sign-in asks Twitch for those used within
    /// `crate::state::twitch::TWITCH_KEEP_DAYS` again in the same step
    /// (`crate::state::twitch::kept_features`); forgotten by Options →
    /// Data's Disconnect. Kept across signing out.
    pub twitch_used: BTreeMap<String, u64>,
    /// "Chat style": the chat crossing the screen or falling (C6).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub chat_style: ddi_chat::Style,
    /// "Chatter names": a message led by its chatter's name (C11); off by
    /// default.
    pub chat_names: bool,
    /// "Emotes from": Twitch's own alone, or 7TV's, BTTV's and FFZ's too
    /// (C4).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub chat_emotes_from: ddi_chat::EmotesFrom,
    /// The chat layer's opacity, percent (C7; 60 to start, tuned by a
    /// study).
    pub chat_opacity: u8,
    /// How much of an online room's song list this device downloads ahead
    /// (Options → Data, [`RoomAhead`]; `docs/plans/multiplayer-room-songs.md`,
    /// stage 2): none stored is the device's own default
    /// ([`Settings::room_ahead`]). Left out of the stored settings then.
    #[serde(
        default,
        deserialize_with = "ddi_engine::lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub room_ahead: Option<RoomAhead>,
    pub hand_layout: HandLayout,
    /// Percent of the play viewport, bottom-aligned.
    pub hand_frame_width: u8,
    pub hand_frame_height: u8,
    /// Draw a note exactly on the receptor in the closest frame.
    pub receptor_snap: bool,
    /// Per-audio-path calibrated offsets, keyed by fingerprint id.
    pub audio_profiles: Vec<OffsetProfile>,
    /// Per-display calibrated offsets, keyed by fingerprint id.
    pub display_profiles: Vec<OffsetProfile>,
    /// Per-song offsets in seconds, keyed by song id; positive moves the
    /// notes later against the music. Corrects songs whose audio is out of
    /// sync with the chart (MP3 decoder delay, badly synced packs), which no
    /// device calibration can fix. Zero entries are not stored.
    pub song_offsets: BTreeMap<String, f64>,
    /// Sharing songs by link is on (`docs/plans/shared-songs.md`): a
    /// hidden switch (`#/?share=1`), never a row of Options; with it
    /// Options gets its Sharing section and song select its Share….
    pub sharing: bool,
    /// The token the uploads carry (kept as the rest of the settings
    /// are: as private as the browser's profile).
    pub share_token: String,
    /// Once the multiplayer prototype's hidden switch (`#/?mp=1`,
    /// desktop's `--multiplayer on`). Multiplayer is always there since
    /// 2026-11-14: this gates nothing and is kept only so stored settings
    /// with either value read as before; nothing sets it any more.
    pub multiplayer: bool,
    /// Entering a multiplayer room opens its Rules pop-up by itself, in
    /// its first form (Reset and OK), as M7b had it. A hidden option, off
    /// by default (the maintainer, 2026-11-12: a room opens straight on
    /// its song select with the default rules; Rules stays behind its
    /// button): `#/?mp=1&mprules=1` (`mprules=0`), desktop's
    /// `--mp-rules-first on|off`; never a row of Options.
    pub multiplayer_rules_first: bool,
    /// More offers Copy watch-only link in an online room. The watch-only
    /// link is kept but hidden (the maintainer, 2026-11-13: "Keep, but
    /// maybe we can keep watch-only hidden"): joining by one works as
    /// ever; its Copy shows only with this hidden option, off by default:
    /// `#/?mp=1&mpwatchlink=1` (`mpwatchlink=0`), desktop's
    /// `--mp-watch-link on|off`; never a row of Options.
    pub multiplayer_watch_link: bool,
    /// The CPU roster the last practice room was left with (names,
    /// levels, personalities and policies, in order; never their seeds):
    /// what the next practice room starts with
    /// (`crate::state::cpu_roster::kept`). Kept as it was stored, read
    /// only when a practice room is made, so a roster this build cannot
    /// read falls back to the default without touching the rest. `None`:
    /// the default roster.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub practice_roster: Option<serde_json::Value>,
    /// Options → Data → Usage statistics (the stats proposal's step 2, on
    /// by default): an online room's socket sends the room's server what
    /// kind of client this is and how its previous socket fared, in fixed
    /// ranges (`ddi_room::client_stats`). Off, nothing of it is sent; what
    /// the server counts by itself goes on.
    pub usage_stats: bool,
    /// Options → Data → Send error reports (the stats proposal's step 4,
    /// on by default; the web alone sends them): the game's own errors (a
    /// crash, a promise nobody caught, a lost or missing graphics device,
    /// the page unable to start the game) as a kind and a scrubbed message
    /// (`ddi_room::error_report`). Off, or with the browser's Global
    /// Privacy Control on, none is sent.
    pub error_reports: bool,
}

/// The music's gain in a fresh profile: 69% on the volume slider (about
/// −10 dB; full volume was too loud to start at). Settings already stored
/// keep theirs.
pub const DEFAULT_VOLUME: f32 = 0.69 * 0.69 * 0.69;

/// Volume slider position (0..=1) for a gain. Loudness is perceived
/// roughly logarithmically, so equal slider steps should be roughly equal
/// steps in decibels: gain = position³ covers about 60 dB that way (50% is
/// −18 dB, 10% is −60 dB) and still reaches silence at 0.
pub fn volume_to_slider(gain: f32) -> f32 {
    gain.clamp(0.0, 1.0).cbrt()
}

/// Gain for a volume slider position; inverse of [`volume_to_slider`].
pub fn slider_to_volume(position: f32) -> f32 {
    position.clamp(0.0, 1.0).powi(3)
}

/// The chat layer's opacity by default, percent (round TW2's study).
pub const CHAT_OPACITY: u8 = 60;

/// How many of a multiplayer stage's opponents have a field beside the
/// play (the feedback round E1, `ddi_shell::opponents::arrange_up_to`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpponentFields {
    /// Every one that fits at a readable size (up to all 72).
    #[default]
    AsManyAsFit,
    /// A room of nine's eight at most, those that matter to the player in
    /// them past eight (as before the round).
    UpToEight,
}

/// How much of an online room's song list a device downloads ahead of
/// the picks (`room-preload-v1`, `crate::state::room`'s preload; the
/// maintainer, 2026-11-19: everything by default, a per-device option to
/// lower it). Movies are never downloaded ahead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomAhead {
    /// Every listed song.
    #[default]
    Everything,
    /// The picked song, the one the picker is on and the newest few, up
    /// to a budget.
    LikelyNext,
    /// None: a song is downloaded when it is picked, as before.
    Off,
}

thread_local! {
    /// This device's default for [`Settings::room_ahead`] where none is
    /// stored: the shell's ([`set_device_room_ahead`]).
    static DEVICE_AHEAD: std::cell::Cell<RoomAhead> = const { std::cell::Cell::new(RoomAhead::Everything) };
}

/// The shell says this device's default for [`Settings::room_ahead`]: the
/// web Likely next on a phone-sized screen or where the browser asks to
/// save data (`navigator.connection.saveData`), else Everything.
pub fn set_device_room_ahead(ahead: RoomAhead) {
    DEVICE_AHEAD.with(|d| d.set(ahead));
}

/// This device's default for [`Settings::room_ahead`].
pub fn device_room_ahead() -> RoomAhead {
    DEVICE_AHEAD.with(std::cell::Cell::get)
}

impl Settings {
    /// How much of a room's song list this device downloads ahead: the
    /// player's choice, else the device's default.
    pub fn room_ahead(&self) -> RoomAhead {
        self.room_ahead.unwrap_or_else(device_room_ahead)
    }
}

/// The pointer offered under Options → Display. Stored by these names;
/// shown as "Infinito cursor" ([`CursorStyle::PointerTip`], the default
/// since 2026-11-17), "Infinito arrow" (the default before) and "System".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorStyle {
    InfinitoArrow,
    #[default]
    PointerTip,
    System,
}

/// Which graphics backend draws (`web::screen`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Graphics {
    /// WebGPU where the browser gives an adapter, else WebGL2.
    #[default]
    Auto,
    /// WebGL2 always (a driver's trouble with WebGPU; testing the
    /// fallback).
    WebGl2,
}

/// Reduced motion in the menus.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReducedMotion {
    /// As the system's setting (the web's `prefers-reduced-motion`;
    /// desktop has none and moves).
    #[default]
    Auto,
    On,
    Off,
}

impl ReducedMotion {
    /// Whether motion is reduced, given the system's preference.
    pub fn reduced(self, system: bool) -> bool {
        match self {
            ReducedMotion::Auto => system,
            ReducedMotion::On => true,
            ReducedMotion::Off => false,
        }
    }
}

/// Whether background-change movies play.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoMode {
    /// Play them, and turn one off for the rest of the song when it costs
    /// frames (the display falls behind or the decoder cannot keep up).
    #[default]
    Auto,
    /// Always play them.
    On,
    /// Never: the decoder is never fetched.
    Off,
}

/// What a song without a background shows in its place
/// ([`Settings::generated_background`]).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneratedBackground {
    /// The plain ground, as before.
    Off,
    Bars,
    Kaleidoscope,
    #[default]
    Field,
    Rain,
}

impl GeneratedBackground {
    /// Its look (`None`: off).
    pub fn look(self) -> Option<Look> {
        match self {
            GeneratedBackground::Off => None,
            GeneratedBackground::Bars => Some(Look::Bars),
            GeneratedBackground::Kaleidoscope => Some(Look::Kaleidoscope),
            GeneratedBackground::Field => Some(Look::Field),
            GeneratedBackground::Rain => Some(Look::Rain),
        }
    }
}

/// The shape backgrounds and movies are fitted into (StepMania's cover
/// fit inside it); outside it the screen shows a blurred extension of it.
/// Pack assets were drawn for a cabinet's screen, so a still and a movie
/// line up in the frame they were made for.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackgroundFrame {
    /// The song's movie picture (its frame less its own black bars), else
    /// the background image's shape.
    #[default]
    Auto,
    FourThree,
    SixteenNine,
    /// The whole screen (StepMania's look on the same display).
    Screen,
}

/// Which of the versions without centers the song list offers
/// ([`Settings::simplify_center`]).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CenterVersions {
    Off,
    /// Those of a difficulty the style has no original chart of (Edits,
    /// each its own chart, always).
    #[default]
    Missing,
    All,
}

/// [`CenterVersions`] as stored, or as an on/off from before it had three
/// values (on: the default, which it was too).
fn center_versions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<CenterVersions, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Known(CenterVersions),
        OnOff(bool),
        Unknown(serde::de::IgnoredAny),
    }
    Ok(match Stored::deserialize(d)? {
        Stored::Known(v) => v,
        Stored::OnOff(false) => CenterVersions::Off,
        Stored::OnOff(true) | Stored::Unknown(_) => CenterVersions::default(),
    })
}

/// Note color scheme (see `ddi_render::note_colors`; the app maps it):
/// The fallback renderer's legacy palette names; SVG Infinito owns its material.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteColors {
    /// By subdivision.
    Quantized,
    /// Cycling rainbow, shifted by position in the beat (the default, as
    /// on DDR from 3rdMIX to 2013).
    #[default]
    Vivid,
    /// Cycling rainbow, all notes alike.
    Flat,
    /// By position in the beat, in four color families with flowing
    /// gradients (DDR 2014–A3).
    Rainbow,
    /// One color.
    Single,
}

fn not_yet() -> bool {
    false
}

/// What is shown of a DDI play's score in play and in the lists: the
/// score (the proportional part plus the absolute score) or the percent
/// (the results show both).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoreDisplay {
    #[default]
    Score,
    Percent,
}

/// DDI's judgment words (`ddi_l10n::judgments`): the ruleset's English
/// (the default since 2026-11-17; the menus' language before), a
/// language's own whatever the menus speak, or the menus' language.
/// Stored as `"English"`, the language's tag (`"ja"`) or `"Localized"`
/// (the menus' language, its name since round 4); anything else reads
/// as English.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum JudgmentWords {
    #[default]
    English,
    /// The words of the language tagged so (one of `ddi_l10n`'s
    /// languages, never English or a pseudo-language).
    Language(&'static str),
    /// The menus' language.
    Game,
}

impl JudgmentWords {
    /// What the words of `tag` are: English's are the ruleset's own.
    pub fn of(tag: &str) -> JudgmentWords {
        match ddi_l10n::find(tag) {
            Some(l) if l.status != ddi_l10n::Status::Pseudo && l.tag != "en" => {
                JudgmentWords::Language(l.tag)
            }
            _ => JudgmentWords::English,
        }
    }

    /// As `ddi_l10n::judgments` shows them.
    pub fn words(self) -> ddi_l10n::judgments::Words {
        use ddi_l10n::judgments::Words;
        match self {
            JudgmentWords::English => Words::English,
            JudgmentWords::Game => Words::Game,
            JudgmentWords::Language(tag) => ddi_l10n::find(tag).map_or(Words::English, Words::In),
        }
    }
}

impl Serialize for JudgmentWords {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            JudgmentWords::English => "English",
            JudgmentWords::Language(tag) => tag,
            JudgmentWords::Game => "Localized",
        })
    }
}

impl<'de> Deserialize<'de> for JudgmentWords {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<JudgmentWords, D::Error> {
        Ok(match String::deserialize(d)?.as_str() {
            "Localized" => JudgmentWords::Game,
            "English" => JudgmentWords::English,
            tag => JudgmentWords::of(tag),
        })
    }
}

/// The rules the settings choose, apart from the rest of them: what
/// builds a chart's ruleset ([`RuleChoice::ruleset_for`]). Kept with a
/// play that waits to be judged again under them
/// (`crate::state::history::PlaySummary::rejudge`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleChoice {
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub mode: RulesetMode,
    pub ruleset: String,
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub danoni: DanoniOptions,
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub holds: HoldStyle,
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub stepmania: StepManiaOptions,
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub ddi: DdiOptions,
}

impl RuleChoice {
    /// The ruleset `chart` is played with.
    pub fn ruleset_for(&self, chart: &ddi_chart::Chart) -> ddi_engine::rules::Ruleset {
        ddi_engine::rules::presets::for_chart(
            self.mode,
            &self.ruleset,
            &self.danoni,
            self.holds,
            chart,
        )
        .with_stepmania(&self.stepmania)
        .with_ddi(&self.ddi)
    }
}

/// Field filter strengths the options screen offers (0 = off).
pub const FIELD_FILTERS: [f32; 4] = [0.0, 0.4, 0.6, 0.8];

/// Song offsets are clamped to this many seconds either way.
pub const MAX_SONG_OFFSET: f64 = 1.0;

/// A calibrated offset for one device fingerprint. `audio_offset` /
/// `visual_offset` on `Settings` remain the defaults for devices without a
/// profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OffsetProfile {
    pub id: String,
    /// The device in English as its fingerprint described it (what older
    /// builds read, and what the screens show where nothing else says).
    pub label: String,
    /// What the device is, as its fingerprint said when last seen (the
    /// screens word it); `None` in a profile kept before, whose id says
    /// it where it can ([`ddi_platform::DeviceFacts::from_id`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facts: Option<ddi_platform::DeviceFacts>,
    /// Seconds.
    pub offset: f64,
    /// Whether a calibration run produced this value (as opposed to a copied
    /// or default one).
    pub calibrated: bool,
    /// Last time this device was seen or calibrated (ms since epoch).
    pub last_seen: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            speed: 2.5,
            speed_source: SpeedSource::Player,
            reverse: false,
            field_place: FieldPlace::Center,
            scroll_action: ScrollAction::Normal,
            transform: TransformOptions::default(),
            simplify_onigiri: false,
            simplify_center: CenterVersions::Missing,
            convert_pump_single: true,
            convert_five_keys: true,
            appearance: Appearance::Visible,
            lane_cover: LaneCover::default(),
            noteskin: None,
            pump_noteskin: None,
            skin_params: BTreeMap::new(),
            lyrics: true,
            import_details: false,
            import_copy: false,
            import_videos: true,
            bg_brightness: 0.4,
            video: VideoMode::Auto,
            random_movies: true,
            generated_background: GeneratedBackground::default(),
            generated_over_pictures: false,
            generated_palette: Palette::default(),
            background_frame: BackgroundFrame::Auto,
            field_filter: 0.4,
            style: "dance-single".into(),
            ruleset: DEFAULT_RULESET.into(),
            rules_moved: true,
            defaults_reset_2026_11_17: true,
            rejudge_from: None,
            ddi: DdiOptions::default(),
            ddi_score: ScoreDisplay::Score,
            judgment_words: JudgmentWords::English,
            ruleset_mode: RulesetMode::AllStepMania,
            danoni: DanoniOptions::default(),
            holds: HoldStyle::ByGame,
            stepmania: StepManiaOptions::default(),
            audio_offset: 0.0,
            visual_offset: 0.0,
            volume: DEFAULT_VOLUME,
            assist_clap: false,
            mine_sound: true,
            announcer: Some(BUILT_IN_ANNOUNCER.into()),
            announcer_volume: 1.0,
            announcer_params: BTreeMap::new(),
            announcer_moved: true,
            announcer_notice: false,
            language: None,
            reduced_motion: ReducedMotion::Auto,
            cursor_style: CursorStyle::default(),
            cursor_size: 28.0,
            start_countdown: 3.0,
            results_panels: "Overview".into(),
            debug: false,
            graphics: Graphics::Auto,
            show_deltas: false,
            ghost: false,
            dancing_hand: true,
            dancing_hand_multiplayer: false,
            chart_ratings: ChartRatings::Estimate,
            ratings_moved: true,
            players_folded: false,
            opponent_fields: OpponentFields::default(),
            touch_controls: Default::default(),
            touch_hand: Default::default(),
            twitch_chat: ddi_chat::Level::Off,
            twitch_used: BTreeMap::new(),
            chat_style: ddi_chat::Style::Across,
            chat_names: false,
            chat_emotes_from: ddi_chat::EmotesFrom::default(),
            chat_opacity: CHAT_OPACITY,
            room_ahead: None,
            hand_layout: HandLayout::default(),
            hand_frame_width: 50,
            hand_frame_height: 100,
            receptor_snap: true,
            audio_profiles: Vec::new(),
            display_profiles: Vec::new(),
            song_offsets: BTreeMap::new(),
            sharing: false,
            share_token: String::new(),
            multiplayer: false,
            multiplayer_rules_first: false,
            multiplayer_watch_link: false,
            practice_roster: None,
            usage_stats: true,
            error_reports: true,
            controls: ControlBindings::default(),
            fast_pad_poll: true,
            controller_styles: Default::default(),
        }
    }
}

/// The noteskin keys' forms, defined with the skins
/// ([`ddi_noteskin::bundled`]): the built-in arrows' value, bundled skins'
/// prefix ([`BUNDLED`]) and Infinito's key.
pub use ddi_noteskin::bundled::{BUILT_IN, INFINITO, KEY_PREFIX as BUNDLED};
/// Skins' parameters' values.
pub use ddi_noteskin::params::{Value as ParamValue, Values};

/// Skins whose parameters' values are kept, at most (a hand edit cannot
/// grow the settings without end).
const MAX_SKINS_WITH_PARAMS: usize = 64;

/// Whether a noteskin key is a pump skin's (`pump/…`, `bundled:pump/…`).
pub fn is_pump_skin(key: &str) -> bool {
    let key = key.strip_prefix(BUNDLED).unwrap_or(key);
    key.split('/')
        .next()
        .is_some_and(|g| g.eq_ignore_ascii_case("pump"))
}

/// `$v` within `$lo..=$hi`, or `$default` when it is not a number.
macro_rules! finite_in {
    ($v:expr, $lo:expr, $hi:expr, $default:expr) => {{
        let v = $v;
        if v.is_finite() {
            v.clamp($lo, $hi)
        } else {
            $default
        }
    }};
}

/// The one of `choices` nearest `v`.
fn nearest<const N: usize>(v: f32, choices: [f32; N]) -> f32 {
    choices
        .into_iter()
        .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
        .unwrap_or(v)
}

impl Settings {
    /// The touch controls the player chose (`docs/plans/touch-controls.md`).
    pub fn touch_choice(&self) -> crate::touch_controls::Choice {
        crate::touch_controls::Choice {
            style: self.touch_controls,
            hand: self.touch_hand,
        }
    }

    /// The noteskin key chosen for layout `layout` (an id): the pump skin
    /// on a pump layout when one is chosen (and draws them), else
    /// [`Settings::noteskin`].
    pub fn noteskin_for(&self, layout: &str) -> Option<&str> {
        match &self.pump_noteskin {
            Some(k) if layout.starts_with("pump") && is_pump_skin(k) => Some(k),
            _ => self.noteskin.as_deref(),
        }
    }

    /// The chosen skin for `layout`, or the default SVG Infinito.
    pub fn play_skin(&self, layout: &str) -> String {
        self.noteskin_for(layout).unwrap_or(INFINITO).to_string()
    }

    /// Lets the chosen skins go that are not `there` (by key; gone, or
    /// chosen on another machine): what is drawn then is what no choice
    /// draws. Whether one was.
    pub fn let_go_missing_skins(&mut self, there: impl Fn(&str) -> bool) -> bool {
        let missing = |k: &Option<String>| k.as_deref().is_some_and(|k| !there(k));
        let (dance, pump) = (missing(&self.noteskin), missing(&self.pump_noteskin));
        if dance {
            self.noteskin = None;
        }
        if pump {
            self.pump_noteskin = None;
        }
        dance || pump
    }

    /// Lets go of the chosen announcer when it is not among those `there`
    /// says are, for the built-in one (never for Off); whether it did.
    pub fn let_go_missing_announcer(&mut self, there: impl Fn(&str) -> bool) -> bool {
        let missing = self
            .announcer
            .as_deref()
            .is_some_and(|k| k != BUILT_IN_ANNOUNCER && !there(k));
        if missing {
            self.announcer = Some(BUILT_IN_ANNOUNCER.into());
        }
        missing
    }

    /// The values stored for announcer `key`'s choices.
    pub fn announcer_params(&self, key: &str) -> Values {
        self.announcer_params.get(key).cloned().unwrap_or_default()
    }

    /// Sets announcer `key`'s value `name` to `value`, or forgets it
    /// (`None`, or `value` its `default`).
    pub fn set_announcer_param(
        &mut self,
        key: &str,
        name: &str,
        value: Option<ParamValue>,
        default: Option<&ParamValue>,
    ) {
        let value = value.filter(|v| Some(v) != default);
        let values = self.announcer_params.entry(key.to_string()).or_default();
        match value {
            Some(v) => {
                values.insert(name.to_string(), v);
            }
            None => {
                values.remove(name);
            }
        }
        if values.is_empty() {
            self.announcer_params.remove(key);
        }
    }

    /// The announcer chosen and its values (none: Off).
    pub fn announcer_choice(&self) -> Option<(String, Values)> {
        let key = self.announcer.clone()?;
        let values = self.announcer_params(&key);
        Some((key, values))
    }

    /// Whether song select is to say once that an announcer now speaks
    /// ([`Settings::announcer_notice`]): taken, so it is said once.
    pub fn take_announcer_notice(&mut self) -> bool {
        std::mem::take(&mut self.announcer_notice)
    }

    /// The announcer's gain in a play: its share of the music's.
    pub fn voice_volume(&self) -> f32 {
        self.volume * self.announcer_volume
    }

    /// The values stored for skin `key`'s parameters (what its
    /// declarations take of them is read as it loads).
    pub fn skin_params(&self, key: &str) -> Values {
        self.skin_params.get(key).cloned().unwrap_or_default()
    }

    /// Sets skin `key`'s parameter `name` to `value`, or forgets it
    /// (`None`, or `value` its `default`).
    pub fn set_skin_param(
        &mut self,
        key: &str,
        name: &str,
        value: Option<ParamValue>,
        default: Option<&ParamValue>,
    ) {
        let value = value.filter(|v| Some(v) != default);
        let values = self.skin_params.entry(key.to_string()).or_default();
        match value {
            Some(v) => {
                values.insert(name.to_string(), v);
            }
            None => {
                values.remove(name);
            }
        }
        if values.is_empty() {
            self.skin_params.remove(key);
        }
    }

    /// Infinito's parameter `name`'s value, as its declaration takes it.
    fn infinito_param(&self, name: &str) -> Option<ParamValue> {
        ddi_noteskin::bundled::infinito_parameters().value(
            self.skin_params.get(INFINITO).unwrap_or(&Values::new()),
            name,
        )
    }

    /// Infinito's selected color scheme, also used if procedural fallback is needed.
    pub fn note_colors(&self) -> NoteColors {
        match self.infinito_param(ddi_noteskin::bundled::infinito::COLORS) {
            Some(ParamValue::Text(name)) => {
                serde_json::from_value(serde_json::Value::String(name)).unwrap_or_default()
            }
            _ => NoteColors::default(),
        }
    }

    /// Infinito's parameter `name` set to `value` as its declaration takes it
    /// (forgotten at its default).
    pub fn set_infinito_param(&mut self, name: &str, value: ParamValue) {
        let declared = ddi_noteskin::bundled::infinito_parameters().get(name);
        let value = declared.and_then(|p| p.accept(&value));
        self.set_skin_param(INFINITO, name, value, declared.map(|p| &p.default));
    }

    /// Retired skin names and controls become the new default Infinito;
    /// preserve the authored Spiral Base options under its stable key.
    pub(crate) fn migrate(&mut self) {
        // The default ruleset moved from ITG to DDI: settings naming ITG
        // move once (the two cannot be told apart, see `ruleset`), and
        // their kept ITG plays are judged again under DDI so the bests
        // are not empty. Other rulesets were chosen and stay.
        // Chart ratings' default moved to the DDI estimate: settings still
        // on the song's numbers move once (a default and a choice cannot
        // be told apart, see `ruleset`); MSD was chosen and stays.
        if !self.ratings_moved {
            self.ratings_moved = true;
            if self.chart_ratings == ChartRatings::FromSong {
                self.chart_ratings = ChartRatings::Estimate;
            }
        }
        if !self.rules_moved {
            self.rules_moved = true;
            if self.ruleset == OLD_DEFAULT_RULESET {
                self.ruleset = DEFAULT_RULESET.into();
                self.rejudge_from = Some(OLD_DEFAULT_RULESET.into());
            }
        }
        // The defaults of 2026-11-17 (the system's language, English
        // judgment words, the Infinito cursor) put back once over what
        // was stored before them; the cursor's size and the rest stay.
        if !self.defaults_reset_2026_11_17 {
            self.defaults_reset_2026_11_17 = true;
            let defaults = Settings::default();
            self.language = defaults.language;
            self.judgment_words = defaults.judgment_words;
            self.cursor_style = defaults.cursor_style;
        }
        // The built-in announcer, on by default (2026-11-17): settings
        // with it Off or with one of the two packs it replaced get it at
        // its default language, once (Off by choice and Off by default
        // cannot be told apart, see `ruleset`); another pack stays. Those
        // who had it Off are told, once.
        if !self.announcer_moved {
            self.announcer_moved = true;
            let was = self.announcer.clone();
            if was.is_none()
                || was
                    .as_deref()
                    .is_some_and(|k| OLD_BUNDLED_ANNOUNCERS.contains(&k))
            {
                self.announcer = Some(BUILT_IN_ANNOUNCER.into());
                self.announcer_params.remove(BUILT_IN_ANNOUNCER);
                self.announcer_notice = was.is_none();
            }
        }
        const SPIRAL: &str = "bundled:dance/spiral base";
        const OLD_DITO: &str = "bundled:dance/dito";
        for key in [&mut self.noteskin, &mut self.pump_noteskin] {
            match key.as_deref() {
                Some(
                    SPIRAL
                    | OLD_DITO
                    | "bundled:dance/dito vivid"
                    | "bundled:dance/dito rainbow"
                    | "bundled:dance/dito flat"
                    | "bundled:dance/dito single",
                ) => {
                    *key = Some(INFINITO.into());
                }
                Some(BUILT_IN) => *key = None,
                _ => {}
            }
        }
        let mut values = self.skin_params.remove(OLD_DITO).unwrap_or_default();
        values.extend(self.skin_params.remove(SPIRAL).unwrap_or_default());
        // An explicit choice under the new key takes precedence.
        values.extend(self.skin_params.remove(INFINITO).unwrap_or_default());
        // What Infinito does not declare or take is let go, and a value at
        // its default is not kept.
        for (name, value) in values {
            self.set_infinito_param(&name, value);
        }
        // The two Peters skins now share one choice and a Colors parameter.
        // Keep the selected look, including in a replay's settings snapshot.
        let colors = match self.noteskin.as_deref() {
            Some("bundled:dance/peters-ddr-note") => Some("Quantized"),
            Some("bundled:dance/peters-ddr-vivid") => Some("Vivid"),
            _ => None,
        };
        if let Some(colors) = colors {
            let old = self.noteskin.take().unwrap();
            let key = ddi_noteskin::bundled::PETERS_DDR;
            if let Some(values) = self.skin_params.remove(&old) {
                self.skin_params
                    .entry(key.into())
                    .or_default()
                    .extend(values);
            }
            self.noteskin = Some(key.into());
            self.set_skin_param(
                key,
                "Colors",
                Some(ParamValue::Text(colors.into())),
                Some(&ParamValue::Text("Vivid".into())),
            );
        }
    }

    pub fn load() -> Settings {
        let stored: Settings = storage::load(Area::Local, KEY).unwrap_or_default();
        let mut s = stored.clone();
        s.sanitize();
        if s != stored {
            // Persist migrations and clamping so storage matches what runs.
            s.save();
        }
        s
    }

    /// [`Settings::load`], read again only when the stored text changed
    /// (for callers on every selection step or turn of the event loop).
    pub fn cached() -> Settings {
        thread_local! {
            /// The settings last loaded and the stored text they came from.
            static CACHED: std::cell::RefCell<Option<(Option<String>, Settings)>> =
                const { std::cell::RefCell::new(None) };
        }
        let raw = storage::get(Area::Local, KEY);
        let hit = CACHED.with_borrow(|c| match c {
            Some((text, s)) if *text == raw => Some(s.clone()),
            _ => None,
        });
        if let Some(s) = hit {
            return s;
        }
        let s = Settings::load();
        // Loading may have stored a sanitized copy.
        let raw = storage::get(Area::Local, KEY);
        CACHED.set(Some((raw, s.clone())));
        s
    }

    /// Clamps stored values to the ranges the options screen allows and
    /// normalises the key table (one row per lane, a key on one lane only),
    /// so a hand-edited or stale `localStorage` entry cannot misbehave.
    pub(crate) fn sanitize(&mut self) {
        let defaults = Settings::default();
        self.migrate();
        // Skins' parameters: within the bounds a skin may declare (what a
        // skin takes of them is read as it loads).
        while self.skin_params.len() > MAX_SKINS_WITH_PARAMS {
            self.skin_params.pop_last();
        }
        for values in self.skin_params.values_mut() {
            ddi_noteskin::params::bound_stored(values);
        }
        self.skin_params.retain(|_, v| !v.is_empty());
        // Round TB: the Twitch permissions used, by the server's words.
        self.twitch_used
            .retain(|k, _| matches!(k.as_str(), "bans" | "blocks" | "chat"));
        while self.announcer_params.len() > MAX_ANNOUNCERS_WITH_PARAMS {
            self.announcer_params.pop_last();
        }
        for values in self.announcer_params.values_mut() {
            ddi_noteskin::params::bound_stored(values);
        }
        self.announcer_params.retain(|_, v| !v.is_empty());
        // Only a pump skin draws the pump layouts.
        if self
            .pump_noteskin
            .as_deref()
            .is_some_and(|k| !is_pump_skin(k))
        {
            self.pump_noteskin = None;
        }
        self.speed = finite_in!(self.speed, 0.25, 10.0, defaults.speed);
        // One of the choices Options offers (a hand edit: the nearest).
        self.cursor_size = finite_in!(self.cursor_size, 16.0, 64.0, defaults.cursor_size).round();
        if !self.start_countdown.is_finite() {
            self.start_countdown = defaults.start_countdown;
        }
        self.start_countdown = nearest(self.start_countdown, START_COUNTDOWNS);
        for v in [&mut self.audio_offset, &mut self.visual_offset] {
            *v = finite_in!(*v, -0.5, 0.5, 0.0);
        }
        self.volume = finite_in!(self.volume, 0.0, 1.0, defaults.volume);
        self.announcer_volume =
            finite_in!(self.announcer_volume, 0.0, 1.0, defaults.announcer_volume);
        self.bg_brightness = finite_in!(self.bg_brightness, 0.0, 1.0, defaults.bg_brightness);
        // Snap to the nearest offered strength so the select shows it.
        self.field_filter = if self.field_filter.is_finite() {
            nearest(self.field_filter, FIELD_FILTERS)
        } else {
            defaults.field_filter
        };
        // Older display ids carried the refresh rate (`display:WxH@dpr:hz`);
        // the rate is not an identity (variable-rate panels change it), so
        // strip it and merge duplicates, keeping a calibrated entry.
        for p in &mut self.display_profiles {
            if let Some(idx) = p.id.rfind(':')
                && idx > "display:".len()
                && p.id[idx + 1..].parse::<f64>().is_ok()
            {
                p.id.truncate(idx);
            }
        }
        let mut merged: Vec<OffsetProfile> = Vec::new();
        for p in self.display_profiles.drain(..) {
            match merged.iter_mut().find(|m| m.id == p.id) {
                Some(m) => {
                    let better = p.calibrated && (!m.calibrated || p.last_seen > m.last_seen);
                    if better {
                        *m = p;
                    }
                }
                None => merged.push(p),
            }
        }
        self.display_profiles = merged;
        for p in self
            .audio_profiles
            .iter_mut()
            .chain(self.display_profiles.iter_mut())
        {
            p.offset = finite_in!(p.offset, -0.5, 0.5, 0.0);
        }
        self.song_offsets
            .retain(|_, v| v.is_finite() && v.abs() >= SONG_OFFSET_EPSILON);
        for v in self.song_offsets.values_mut() {
            *v = v.clamp(-MAX_SONG_OFFSET, MAX_SONG_OFFSET);
        }
        self.controls.sanitize();
        self.stepmania = self.stepmania.sanitized();
        // A preset this build does not have (a hand edit, a later
        // version's): the default.
        if ddi_engine::rules::presets::by_id(&self.ruleset).is_none() {
            self.ruleset = defaults.ruleset.clone();
        }
        if self.style.is_empty() {
            self.style = defaults.style;
        }
    }

    /// Saved bindings for a controller, else the standard defaults when it
    /// has the standard mapping.
    pub fn pad_bindings(&self, id: &str, standard: bool) -> Option<PadBindings> {
        self.controls.pad(id, standard)
    }

    /// Stores a controller's bindings, replacing earlier ones.
    pub fn set_pad_bindings(&mut self, pad: PadBindings) {
        self.controls.set_pad(pad, Self::now_ms());
    }

    pub fn save(&self) {
        let _ = storage::save(Area::Local, KEY, self);
    }

    /// Play options for the given devices: profile offsets when known,
    /// else the defaults. The shuffle seed is left at 0 for the play session
    /// to pick.
    pub fn play_options(&self, devices: Option<&ddi_platform::DeviceProfile>) -> PlayOptions {
        let audio_offset = self.audio_offset_for(devices.map(|d| &d.audio));
        let visual_offset = self.visual_offset_for(devices.map(|d| &d.display));
        PlayOptions {
            scroll: ScrollOptions {
                speed: SpeedMod::XMod(self.speed),
                reverse: self.reverse,
                scroll_action: self.scroll_action,
            },
            transform: self.transform,
            appearance: self.appearance,
            cover: self.lane_cover,
            clock: ClockOptions {
                audio_offset,
                visual_offset,
                rate: 1.0,
            },
            receptor_snap: self.receptor_snap,
            ..PlayOptions::default()
        }
    }

    /// Whether a play of `layout` is accompanied by the dancing hand: it
    /// is on, and plays that layout ([`ddi_character::HAND_LAYOUTS`]).
    pub fn dancing_hand_for(&self, layout: &str) -> bool {
        self.dancing_hand && ddi_character::HAND_LAYOUTS.contains(&layout)
    }

    /// [`Self::dancing_hand_for`] for a play that is a multiplayer stage
    /// (`stage`) or not: in a stage only where the player chose
    /// [`Self::dancing_hand_multiplayer`].
    pub fn dancing_hand_in(&self, layout: &str, stage: bool) -> bool {
        self.dancing_hand_for(layout) && (!stage || self.dancing_hand_multiplayer)
    }

    /// Keeps the heights a play left the lane covers at, as DDR and
    /// beatmania IIDX keep an adjusted cover for the next song (the mode
    /// stays the one chosen: a cover toggled off in play is on again).
    /// Whether anything changed.
    pub fn keep_cover_heights(&mut self, cover: LaneCover) -> bool {
        let before = self.lane_cover;
        self.lane_cover.sudden = cover.sudden;
        self.lane_cover.hidden = cover.hidden;
        self.lane_cover != before
    }

    /// The audio offset a play on output `audio` takes: its profile's
    /// when known, else the default.
    pub fn audio_offset_for(&self, audio: Option<&ddi_platform::DeviceFingerprint>) -> f64 {
        audio
            .and_then(|a| self.audio_profile(&a.id))
            .map_or(self.audio_offset, |p| p.offset)
    }

    /// The visual offset a play on display `display` takes: its profile's
    /// when known, else the default.
    pub fn visual_offset_for(&self, display: Option<&ddi_platform::DeviceFingerprint>) -> f64 {
        display
            .and_then(|d| self.display_profile(&d.id))
            .map_or(self.visual_offset, |p| p.offset)
    }

    pub fn audio_profile(&self, id: &str) -> Option<&OffsetProfile> {
        find_profile(&self.audio_profiles, id).map(|i| &self.audio_profiles[i])
    }

    pub fn display_profile(&self, id: &str) -> Option<&OffsetProfile> {
        find_profile(&self.display_profiles, id).map(|i| &self.display_profiles[i])
    }

    fn now_ms() -> f64 {
        storage::now_ms()
    }

    /// Creates or refreshes the profile for a fingerprint. New profiles start
    /// from `initial` and are marked uncalibrated.
    /// The devices a play starts on, kept with the default offsets where
    /// they have no profile yet (a profile already there is only seen
    /// again): a device meets no banner and no step, it plays on the
    /// defaults until calibrated (`docs/plans/quiet-calibration.md`).
    pub fn register_devices(&mut self, profile: &ddi_platform::DeviceProfile) {
        let (audio, visual) = (self.audio_offset, self.visual_offset);
        Self::touch_profile(&mut self.audio_profiles, &profile.audio, audio);
        Self::touch_profile(&mut self.display_profiles, &profile.display, visual);
    }

    /// The audio path and the display seen last (each profile's
    /// `last_seen`), as a play probably runs on before its own probe
    /// says: the start prompt's quiet line reads the history by them.
    pub fn last_devices(&self) -> Option<(String, String)> {
        let last = |list: &[OffsetProfile]| {
            list.iter()
                .max_by(|a, b| a.last_seen.total_cmp(&b.last_seen))
                .map(|p| p.id.clone())
        };
        Some((last(&self.audio_profiles)?, last(&self.display_profiles)?))
    }

    pub fn touch_profile<'a>(
        profiles: &'a mut Vec<OffsetProfile>,
        fp: &ddi_platform::DeviceFingerprint,
        initial: f64,
    ) -> &'a mut OffsetProfile {
        let now = Self::now_ms();
        let idx = match find_profile(profiles, &fp.id) {
            Some(i) => i,
            None => {
                profiles.push(OffsetProfile {
                    id: fp.id.clone(),
                    label: fp.label(),
                    facts: Some(fp.facts.clone()),
                    offset: initial,
                    calibrated: false,
                    last_seen: now,
                });
                profiles.len() - 1
            }
        };
        let p = &mut profiles[idx];
        p.label = fp.label();
        p.facts = Some(fp.facts.clone());
        p.last_seen = now;
        p
    }

    /// Store a calibrated offset for the audio or display profile.
    pub fn set_profile_offset(
        profiles: &mut Vec<OffsetProfile>,
        fp: &ddi_platform::DeviceFingerprint,
        offset: f64,
    ) {
        let p = Self::touch_profile(profiles, fp, offset);
        p.offset = offset.clamp(-0.5, 0.5);
        p.calibrated = true;
    }

    pub fn song_offset(&self, song_id: &str) -> f64 {
        self.song_offsets.get(song_id).copied().unwrap_or(0.0)
    }

    pub fn set_song_offset(&mut self, song_id: &str, seconds: f64) {
        let seconds = seconds.clamp(-MAX_SONG_OFFSET, MAX_SONG_OFFSET);
        if seconds.abs() < SONG_OFFSET_EPSILON {
            self.song_offsets.remove(song_id);
        } else {
            self.song_offsets.insert(song_id.to_string(), seconds);
        }
    }

    pub fn ruleset(&self) -> ddi_engine::rules::Ruleset {
        ddi_engine::rules::presets::by_id(&self.ruleset)
            .unwrap_or_else(ddi_engine::rules::presets::ddi)
            .with_stepmania(&self.stepmania)
            .with_ddi(&self.ddi)
    }

    /// The rules chosen, apart from the rest of the settings.
    /// Speaks the settings' language (`ddi_l10n::choose`) and shows DDI's
    /// judgment words as they choose ([`Settings::judgment_words`]).
    pub fn choose_language(&self) -> &'static ddi_l10n::Language {
        ddi_l10n::judgments::set_words(self.judgment_words.words());
        self.show_ratings();
        ddi_l10n::choose(self.language.as_deref())
    }

    /// Tells the screens what the chart ratings options show
    /// ([`crate::state::ratings::set_mode`]).
    pub fn show_ratings(&self) {
        use crate::state::ratings::Source;
        let source = match self.chart_ratings {
            ChartRatings::FromSong => Source::Song,
            ChartRatings::Msd => Source::Msd,
            ChartRatings::Estimate => Source::Estimate,
        };
        // The radar is always shown, so ratings are always made.
        crate::state::ratings::set_mode(source, true);
    }

    pub fn rules(&self) -> RuleChoice {
        RuleChoice {
            mode: self.ruleset_mode,
            ruleset: self.ruleset.clone(),
            danoni: self.danoni.clone(),
            holds: self.holds,
            stepmania: self.stepmania,
            ddi: self.ddi,
        }
    }

    /// The ruleset `chart` is played with.
    pub fn ruleset_for(&self, chart: &ddi_chart::Chart) -> ddi_engine::rules::Ruleset {
        self.rules().ruleset_for(chart)
    }

    /// Whether a DDI score shows as its percent ([`ScoreDisplay`]).
    pub fn ddi_percent(&self) -> bool {
        self.ddi_score == ScoreDisplay::Percent
    }

    /// The key the play history keeps a chart's bests under, from what
    /// the song list knows of the chart: its layout, its provenance,
    /// whether it is a
    /// Dancing☆Onigiri work's or an FFR level's, and the excessive setting
    /// such a work's chart gives ([`chart_excessive`]). The same as
    /// `ruleset_for(chart)`'s key's `best_key` (a test holds them equal).
    pub fn best_key_for(
        &self,
        layout: &str,
        derivation: Option<&ddi_chart::simplify::Derivation>,
        danoni: bool,
        ffr: bool,
        excessive: Option<bool>,
    ) -> ddi_engine::rules::RulesetKey {
        let chart = ddi_chart::Chart {
            // A generated version's game (a pump chart's without centers
            // ticks its holds) is part of its rules.
            derivation: derivation.cloned(),
            layout: layout.to_string(),
            difficulty: ddi_chart::Difficulty::Edit,
            meter: 0,
            name: String::new(),
            description: String::new(),
            credit: String::new(),
            notes: Vec::new(),
            timing: None,
            display_bpm: None,
            danoni: danoni.then(Default::default),
            ffr,
        };
        let mut key = self.ruleset_for(&chart).key();
        // Excessive as the chart sets it, where the player left it to the
        // chart.
        if let Some(d) = key.danoni.as_mut()
            && self.danoni.excessive.is_none()
        {
            d.excessive = Some(excessive.unwrap_or(false));
        }
        key.best_key()
    }
}

/// The excessive setting a Dancing☆Onigiri chart gives
/// (`excessiveJdgUse`, `excessiveUse`), for the song list
/// ([`Settings::best_key_for`]); `None` for other charts.
pub fn chart_excessive(chart: &ddi_chart::Chart) -> Option<bool> {
    ddi_engine::rules::danoni::chart_excessive(chart)
}

/// Exact id match, or for audio ids (the web's `audio:<rate>:<base>:<out>`,
/// desktop's `audio:<host>:<device>:<rate>:<bucket>`) the entry whose
/// output-latency bucket is within 10 ms on the same path (rate and base
/// latency; host, device and rate), so jitter across a 5 ms bucket
/// boundary, or a new buffer size, is the same device.
fn find_profile(profiles: &[OffsetProfile], id: &str) -> Option<usize> {
    if let Some(i) = profiles.iter().position(|p| p.id == id) {
        return Some(i);
    }
    let (path, out) = audio_parts(id)?;
    profiles
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let (pp, o) = audio_parts(&p.id)?;
            (pp == path && (o - out).abs() <= 10).then_some((i, o))
        })
        .min_by_key(|(_, o)| (*o - out).abs())
        .map(|(i, _)| i)
}

/// Whether audio ids `a` and `b` name the same output (the same id, or
/// the same path with latency buckets within 10 ms; see
/// [`Settings::audio_profile`]).
pub fn same_audio_device(a: &str, b: &str) -> bool {
    a == b
        || matches!((audio_parts(a), audio_parts(b)), (Some((pa, oa)), Some((pb, ob))) if pa == pb && (oa - ob).abs() <= 10)
}

/// An audio id's path and its latency bucket (ms): the web's
/// `audio:<rate>:<base>:<out>`, desktop's
/// `audio:<host>:<device>:<rate>:<bucket>` (`ddi-desktop`'s
/// `Audio::fingerprint`): everything before the last field, and it.
fn audio_parts(id: &str) -> Option<(&str, i64)> {
    let rest = id.strip_prefix("audio:")?;
    let (path, out) = rest.rsplit_once(':')?;
    // The web's form has its rate and base latency before it.
    if !path.contains(':') {
        return None;
    }
    Some((path, out.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn audio_profiles_match_within_ten_ms_on_both_shells_ids() {
        let p = |id: &str| OffsetProfile {
            id: id.into(),
            label: id.into(),
            facts: None,
            offset: 0.01,
            calibrated: true,
            last_seen: 0.0,
        };
        let list = vec![
            p("audio:48000:5:170"),
            p("audio:CoreAudio:External Headphones:44100:10"),
        ];
        assert_eq!(find_profile(&list, "audio:48000:5:175"), Some(0));
        assert_eq!(find_profile(&list, "audio:48000:10:170"), None);
        assert_eq!(
            find_profile(&list, "audio:CoreAudio:External Headphones:44100:20"),
            Some(1)
        );
        assert_eq!(
            find_profile(&list, "audio:CoreAudio:External Headphones:44100:25"),
            None
        );
        assert_eq!(
            find_profile(&list, "audio:CoreAudio:Speakers:44100:10"),
            None
        );
        assert_eq!(find_profile(&list, "display:x:1x1@1"), None);
    }

    use ddi_platform::storage::MemoryStore;

    /// Settings as an older version stored them: a key table under
    /// `keys_single`, a display id with its refresh rate twice, values out
    /// of range, a field it no longer knows, one it never had.
    const OLD: &str = r#"{
        "speed": 99,
        "reverse": true,
        "keys_single": [["ArrowLeft"], ["ArrowDown"], ["ArrowUp"], ["ArrowRight"]],
        "display_profiles": [
            {"id": "display:1920x1080@2:60", "label": "a", "offset": 0.01, "calibrated": false, "last_seen": 1},
            {"id": "display:1920x1080@2:144", "label": "b", "offset": 0.02, "calibrated": true, "last_seen": 2}
        ],
        "song_offsets": {"s1": 0.0001, "s2": 5},
        "note_colors": "NoSuchScheme",
        "gone": 1
    }"#;

    #[test]
    fn peters_choices_migrate_with_their_colors() {
        use ddi_noteskin::bundled::PETERS_DDR;
        for (old, colors) in [
            ("bundled:dance/peters-ddr-note", Some("Quantized")),
            ("bundled:dance/peters-ddr-vivid", None),
        ] {
            let mut s = Settings {
                noteskin: Some(old.into()),
                ..Settings::default()
            };
            s.sanitize();
            assert_eq!(s.noteskin.as_deref(), Some(PETERS_DDR));
            assert_eq!(
                s.skin_params(PETERS_DDR).get("Colors"),
                colors.map(|c| ParamValue::Text(c.into())).as_ref()
            );
            let migrated = s.clone();
            s.sanitize();
            assert_eq!(s, migrated);
            let mut roundtrip: Settings =
                serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
            roundtrip.sanitize();
            assert_eq!(roundtrip, migrated);
        }
        // Imported skins with the same names still belong to the player.
        let mut imported = Settings {
            noteskin: Some("dance/peters-ddr-note".into()),
            ..Settings::default()
        };
        imported.sanitize();
        assert_eq!(imported.noteskin.as_deref(), Some("dance/peters-ddr-note"));
    }

    #[test]
    fn the_versions_without_centers_read_their_old_on_and_off() {
        let read = |json: &str| {
            serde_json::from_str::<Settings>(json)
                .unwrap()
                .simplify_center
        };
        assert_eq!(
            read(r#"{"simplify_center": true}"#),
            CenterVersions::Missing
        );
        assert_eq!(read(r#"{"simplify_center": false}"#), CenterVersions::Off);
        assert_eq!(read(r#"{"simplify_center": "All"}"#), CenterVersions::All);
        assert_eq!(
            read(r#"{"simplify_center": "Some"}"#),
            CenterVersions::Missing
        );
        let s = Settings {
            simplify_center: CenterVersions::All,
            ..Settings::default()
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.simplify_center, CenterVersions::All);
    }

    #[test]
    fn a_fresh_profile_starts_at_69_percent_and_a_stored_volume_stays() {
        assert_eq!((volume_to_slider(DEFAULT_VOLUME) * 100.0).round(), 69.0);
        // Nothing stored, or settings stored before the volume was kept:
        // the default.
        assert_eq!(Settings::default().volume, DEFAULT_VOLUME);
        let s: Settings = serde_json::from_str(r#"{"speed": 2.0}"#).unwrap();
        assert_eq!(s.volume, DEFAULT_VOLUME);
        // A volume stored (the old default among them) is the player's.
        let s: Settings = serde_json::from_str(r#"{"volume": 0.8}"#).unwrap();
        assert_eq!(s.volume, 0.8);
    }

    #[test]
    fn the_announcer_starts_built_in_at_the_musics_volume_and_goes_back_to_it() {
        let s: Settings = serde_json::from_str(r#"{"speed": 2.0}"#).unwrap();
        assert_eq!(
            (s.announcer.as_deref(), s.announcer_volume),
            (Some(BUILT_IN_ANNOUNCER), 1.0)
        );
        assert_eq!(Settings::default().announcer.as_deref(), Some("bundled:clank"));
        let mut s: Settings = serde_json::from_str(
            r#"{"volume": 0.5, "announcer": "techno", "announcer_volume": 7.0, "announcer_moved": true}"#,
        )
        .unwrap();
        s.sanitize();
        assert_eq!(s.announcer_volume, 1.0);
        s.announcer_volume = 0.5;
        assert_eq!(s.voice_volume(), 0.25);
        assert!(!s.let_go_missing_announcer(|k| k == "techno"));
        // One gone: the built-in speaks for it, never Off.
        assert!(s.let_go_missing_announcer(|k| k == "other"));
        assert_eq!(s.announcer.as_deref(), Some(BUILT_IN_ANNOUNCER));
        assert!(!s.let_go_missing_announcer(|_| false));
        s.announcer = None;
        assert!(!s.let_go_missing_announcer(|_| false));
        assert_eq!(s.announcer, None);
        // Its values, kept at all but their defaults.
        s.set_announcer_param("bundled:clank", "Language", Some(ParamValue::Text("ja".into())), None);
        assert_eq!(
            s.announcer_params("bundled:clank").get("Language"),
            Some(&ParamValue::Text("ja".into()))
        );
        s.set_announcer_param("bundled:clank", "Language", None, None);
        assert!(s.announcer_params.is_empty());
    }

    /// The move to the built-in announcer (2026-11-17): Off and the two
    /// packs it replaced become it at its default language, once; Off is
    /// told; another pack stays; a choice after it stays; fresh settings
    /// have it, untold.
    #[test]
    fn the_built_in_announcer_comes_once_for_off_and_the_old_packs() {
        storage::install(Rc::new(MemoryStore::default()));
        assert!(Settings::default().announcer_moved);
        assert!(!Settings::default().announcer_notice);
        let old = |announcer: serde_json::Value| -> Settings {
            let mut v = serde_json::to_value(Settings::default()).unwrap();
            let o = v.as_object_mut().unwrap();
            assert!(o.remove(ANNOUNCER_MOVED).is_some());
            o.insert("announcer".into(), announcer);
            o.insert(
                "announcer_params".into(),
                serde_json::json!({"bundled:clank": {"Language": "ja"}, "techno": {"Mood": "Calm"}}),
            );
            storage::set(Area::Local, KEY, &v.to_string()).unwrap();
            Settings::load()
        };
        for (was, now, told) in [
            (serde_json::Value::Null, Some(BUILT_IN_ANNOUNCER), true),
            ("bundled:mr. clank".into(), Some(BUILT_IN_ANNOUNCER), false),
            ("bundled:ガキンさん".into(), Some(BUILT_IN_ANNOUNCER), false),
            ("techno".into(), Some("techno"), false),
        ] {
            let mut s = old(was.clone());
            assert!(s.announcer_moved, "{was}");
            assert_eq!(s.announcer.as_deref(), now, "{was}");
            assert_eq!(s.announcer_notice, told, "{was}");
            // The old language is not kept; another pack's values are.
            let built_in = s.announcer_params.contains_key(BUILT_IN_ANNOUNCER);
            assert_eq!(built_in, now == Some("techno"), "{was}");
            assert!(s.announcer_params.contains_key("techno"));
            // Stored so; told once.
            assert_eq!(Settings::load(), s);
            assert_eq!(s.take_announcer_notice(), told);
            assert!(!s.take_announcer_notice());
            // Off chosen after it stays Off.
            s.announcer = None;
            s.save();
            assert_eq!(Settings::load().announcer, None);
        }
    }

    /// DDI's judgment words: English by default (2026-11-17), a
    /// language's own by its tag, the menus' language as "Localized"
    /// (stored so since round 4); anything else English.
    #[test]
    fn judgment_words_read_what_was_stored_and_default_to_english() {
        use ddi_l10n::judgments::Words;
        assert_eq!(Settings::default().judgment_words, JudgmentWords::English);
        let read = |v: &str| {
            serde_json::from_str::<Settings>(&format!(r#"{{"judgment_words": {v}}}"#))
                .unwrap()
                .judgment_words
        };
        assert_eq!(read(r#""English""#), JudgmentWords::English);
        assert_eq!(read(r#""Localized""#), JudgmentWords::Game);
        assert_eq!(read(r#""ja""#), JudgmentWords::Language("ja"));
        assert_eq!(read(r#""JA""#), JudgmentWords::Language("ja"));
        for other in [r#""en""#, r#""qps-ploc""#, r#""xx""#, "5", "null"] {
            assert_eq!(read(other), JudgmentWords::English, "{other}");
        }
        for words in [
            JudgmentWords::English,
            JudgmentWords::Language("ja"),
            JudgmentWords::Game,
        ] {
            let s = Settings {
                judgment_words: words,
                ..Settings::default()
            };
            let back: Settings =
                serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
            assert_eq!(back.judgment_words, words);
        }
        assert_eq!(JudgmentWords::English.words(), Words::English);
        assert_eq!(JudgmentWords::Game.words(), Words::Game);
        assert_eq!(
            JudgmentWords::Language("ja").words(),
            Words::In(ddi_l10n::find("ja").unwrap())
        );
    }

    #[test]
    fn the_fields_place_reads_leniently() {
        let read = |v: &str| {
            serde_json::from_str::<Settings>(&format!(r#"{{"field_place": {v}}}"#))
                .unwrap()
                .field_place
        };
        assert_eq!(read(r#""Left""#), FieldPlace::Left);
        assert_eq!(read(r#""Right""#), FieldPlace::Right);
        // A place a later version has: the middle.
        assert_eq!(read(r#""Top""#), FieldPlace::Center);
    }

    #[test]
    fn the_generated_backgrounds_vivid_reads_as_ink() {
        // Offered for a day (2026-11-05), then left out.
        let s: Settings = serde_json::from_str(r#"{"generated_palette": "Vivid"}"#).unwrap();
        assert_eq!(s.generated_palette, Palette::Ink);
        let s: Settings = serde_json::from_str(r#"{"generated_palette": "Moss"}"#).unwrap();
        assert_eq!(s.generated_palette, Palette::Moss);
    }

    #[test]
    fn older_settings_load_and_migrate() {
        let store = Rc::new(MemoryStore::default());
        storage::install(store.clone());
        storage::set(Area::Local, KEY, OLD).unwrap();
        let s = Settings::load();
        assert_eq!(s.speed, 10.0);
        assert!(s.reverse);
        assert_eq!(s.field_place, FieldPlace::Center);
        // Stored before it was kept, and before the onigiri's: Where
        // missing, as new settings have it.
        assert_eq!(s.simplify_center, CenterVersions::Missing);
        assert_eq!(Settings::default().simplify_center, CenterVersions::Missing);
        assert!(!s.simplify_onigiri);
        assert_eq!(s.note_colors(), NoteColors::Vivid);
        assert!(s.skin_params.is_empty());
        assert_eq!(s.display_profiles.len(), 1);
        assert_eq!(s.display_profiles[0].id, "display:1920x1080@2");
        assert!(s.display_profiles[0].calibrated);
        assert_eq!(s.song_offset("s1"), 0.0);
        assert_eq!(s.song_offset("s2"), MAX_SONG_OFFSET);
        // The migration is stored, and loads as it is.
        let again: Settings = storage::load(Area::Local, KEY).unwrap();
        assert_eq!(again, s);
        // Unreadable: the same as nothing stored (the defaults, sanitized).
        storage::remove(Area::Local, KEY);
        let fresh = Settings::load();
        assert_eq!(fresh.speed, Settings::default().speed);
        storage::set(Area::Local, KEY, "not json").unwrap();
        assert_eq!(Settings::load(), fresh);
    }

    /// The default ruleset moved from ITG to DDI: stored settings naming
    /// ITG move once and ask for their ITG plays to be judged again; any
    /// other ruleset stays, and ITG chosen after the move stays too.
    #[test]
    fn chart_ratings_move_to_the_ddi_estimate_once() {
        storage::install(Rc::new(MemoryStore::default()));
        assert_eq!(Settings::default().chart_ratings, ChartRatings::Estimate);
        assert!(Settings::default().ratings_moved);
        // As stored before the move: no `ratings_moved`; the song's
        // numbers left out (their default then) or written, and a
        // `skill_radar` that is ignored now.
        let stored = |ratings: Option<&str>| {
            let mut v = serde_json::to_value(Settings::default()).unwrap();
            let o = v.as_object_mut().unwrap();
            o.remove("ratings_moved");
            o.remove("chart_ratings");
            if let Some(r) = ratings {
                o.insert("chart_ratings".into(), r.into());
            }
            o.insert("skill_radar".into(), true.into());
            storage::set(Area::Local, KEY, &v.to_string()).unwrap();
        };
        for before in [None, Some("FromSong")] {
            stored(before);
            let s = Settings::load();
            assert_eq!(s.chart_ratings, ChartRatings::Estimate, "{before:?}");
            assert!(s.ratings_moved);
        }
        // MSD was chosen: it stays.
        stored(Some("Msd"));
        assert_eq!(Settings::load().chart_ratings, ChartRatings::Msd);
        // The song's numbers chosen after the move stay.
        let mut s = Settings::load();
        s.chart_ratings = ChartRatings::FromSong;
        s.save();
        assert_eq!(Settings::load().chart_ratings, ChartRatings::FromSong);
    }

    #[test]
    fn settings_naming_itg_move_to_ddi_once() {
        storage::install(Rc::new(MemoryStore::default()));
        assert_eq!(Settings::default().ruleset, "ddi");
        assert!(Settings::default().rules_moved);
        assert_eq!(Settings::default().rejudge_from, None);
        // As stored before the move: every field, no `rules_moved`.
        let stored = |ruleset: &str| {
            let mut v = serde_json::to_value(Settings::default()).unwrap();
            let o = v.as_object_mut().unwrap();
            o.remove("rules_moved");
            o.insert("ruleset".into(), ruleset.into());
            storage::set(Area::Local, KEY, &v.to_string()).unwrap();
        };
        stored("itg");
        let s = Settings::load();
        assert_eq!(s.ruleset, "ddi");
        assert!(s.rules_moved);
        assert_eq!(s.rejudge_from.as_deref(), Some("itg"));
        let chart = |layout: &str| {
            ddi_chart::Chart::plain(layout, ddi_chart::Difficulty::Hard, 9, Vec::new())
        };
        assert_eq!(s.ruleset_for(&chart("dance-single")).id, "ddi");
        // Stored as moved: ITG chosen again stays.
        let mut again = Settings::load();
        assert_eq!(again, s);
        again.ruleset = "itg".into();
        again.rejudge_from = None;
        again.save();
        let kept = Settings::load();
        assert_eq!((kept.ruleset.as_str(), kept.rejudge_from), ("itg", None));
        // Another ruleset was chosen: it stays, and nothing is asked.
        for chosen in ["sm5", "ddr-a", "ffr", "itg-fa+"] {
            stored(chosen);
            let s = Settings::load();
            assert_eq!(s.ruleset, chosen);
            assert!(s.rules_moved);
            assert_eq!(s.rejudge_from, None);
        }
        // A preset this build does not know: the default.
        stored("nonsense");
        assert_eq!(Settings::load().ruleset, "ddi");
        // DDI's options build its ruleset on every chart, pump's too.
        let s = Settings {
            ddi: DdiOptions {
                difficulty: ddi_engine::rules::ddi::Difficulty::Hard,
                boos: true,
                practice: true,
            },
            ..Settings::default()
        };
        let rules = s.ruleset_for(&chart("pump-single"));
        assert_eq!(rules.ddi, Some(s.ddi));
        assert!(!rules.judge.hold.is_ticked());
        assert_eq!(rules.fail, ddi_engine::FailPolicy::ImmediateContinue);
        assert_eq!(s.ruleset().ddi, Some(s.ddi));
    }

    /// The defaults of 2026-11-17 put back once: settings stored before
    /// them (Japanese chosen, the words in the menus' language, the
    /// Infinito arrow) come out as a fresh profile has them, marked,
    /// the rest as stored; a second load changes nothing; a choice made
    /// after the mark stays; a forced language (`?lang=`, `--lang`) is
    /// no stored choice and is untouched.
    #[test]
    fn the_defaults_of_2026_11_17_are_put_back_once() {
        storage::install(Rc::new(MemoryStore::default()));
        assert!(Settings::default().defaults_reset_2026_11_17);
        let mut old = serde_json::to_value(Settings::default()).unwrap();
        let o = old.as_object_mut().unwrap();
        assert!(o.remove(DEFAULTS_RESET).is_some());
        o.insert("language".into(), "ja".into());
        o.insert("judgment_words".into(), "Localized".into());
        o.insert("cursor_style".into(), "InfinitoArrow".into());
        o.insert("cursor_size".into(), 40.0.into());
        o.insert("speed".into(), 3.5.into());
        o.insert("ruleset".into(), "sm5".into());
        storage::set(Area::Local, KEY, &old.to_string()).unwrap();
        let s = Settings::load();
        assert!(s.defaults_reset_2026_11_17);
        assert_eq!(s.language, None);
        assert_eq!(s.judgment_words, JudgmentWords::English);
        assert_eq!(s.cursor_style, CursorStyle::PointerTip);
        // The rest as stored.
        assert_eq!((s.cursor_size, s.speed, s.ruleset.as_str()), (40.0, 3.5, "sm5"));
        // Stored so: a second load reads it as it is.
        let stored: serde_json::Value =
            serde_json::from_str(&storage::get(Area::Local, KEY).unwrap()).unwrap();
        assert_eq!(stored[DEFAULTS_RESET], true);
        assert_eq!(stored["language"], serde_json::Value::Null);
        let text = storage::get(Area::Local, KEY);
        assert_eq!(Settings::load(), s);
        assert_eq!(storage::get(Area::Local, KEY), text);
        // A choice after it stays.
        let mut chosen = s.clone();
        chosen.language = Some("ja".into());
        chosen.judgment_words = JudgmentWords::Game;
        chosen.cursor_style = CursorStyle::InfinitoArrow;
        chosen.save();
        assert_eq!(Settings::load(), chosen);
        // Another old configuration, with its choices other than these.
        let o = old.as_object_mut().unwrap();
        o.insert("language".into(), "en".into());
        o.insert("judgment_words".into(), "English".into());
        o.insert("cursor_style".into(), "System".into());
        storage::set(Area::Local, KEY, &old.to_string()).unwrap();
        let s = Settings::load();
        assert_eq!(
            (s.language, s.judgment_words, s.cursor_style),
            (None, JudgmentWords::English, CursorStyle::PointerTip)
        );
        // A forced language is the run's, never stored or put back.
        assert!(ddi_l10n::force("ja"));
        assert_eq!(Settings::load().choose_language().tag, "ja");
        assert_eq!(Settings::load().language, None);
        ddi_l10n::unforce();
        ddi_l10n::choose(Some("en"));
    }

    #[test]
    fn retired_skins_and_spiral_options_migrate_to_the_new_infinito() {
        storage::install(Rc::new(MemoryStore::default()));
        for key in [
            "builtin",
            "bundled:dance/spiral base",
            "bundled:dance/dito",
            "bundled:dance/dito flat",
        ] {
            storage::set(
                Area::Local,
                KEY,
                &serde_json::json!({
                    "noteskin": key, "note_colors": "Rainbow", "chart_colors": true,
                    "skin_params": {
                        "bundled:dance/dito": {"Lighter": true, "Colors": "Flat", "Explosions": false},
                        "bundled:dance/spiral base": {"ShockArrows": true, "Onigiri": false}
                    }
                })
                .to_string(),
            )
            .unwrap();
            let s = Settings::load();
            assert_eq!(s.play_skin("dance-single"), INFINITO);
            assert_eq!(s.note_colors(), NoteColors::Flat);
            assert_eq!(
                s.skin_params(INFINITO),
                Values::from([
                    ("Colors".into(), ParamValue::Text("Flat".into())),
                    ("Explosions".into(), ParamValue::Bool(false)),
                    ("ShockArrows".into(), ParamValue::Bool(true)),
                    ("Onigiri".into(), ParamValue::Bool(false)),
                ])
            );
            assert!(!s.skin_params.contains_key("bundled:dance/spiral base"));
            assert!(!s.skin_params.contains_key("bundled:dance/dito"));
            assert_eq!(Settings::load(), s);
        }
    }

    #[test]
    fn infinito_lets_go_of_what_it_does_not_take() {
        let mut s = Settings {
            skin_params: [(
                INFINITO.into(),
                Values::from([
                    // Not one of its colors, no longer declared, its default.
                    ("Colors".into(), ParamValue::Text("Quantized".into())),
                    ("Lighter".into(), ParamValue::Bool(true)),
                    ("Explosions".into(), ParamValue::Bool(true)),
                ]),
            )]
            .into(),
            ..Settings::default()
        };
        s.migrate();
        assert!(s.skin_params.is_empty(), "{:?}", s.skin_params);
        assert_eq!(s.note_colors(), NoteColors::Vivid);
    }

    #[test]
    fn renamed_infinito_keeps_current_choices_over_former_keys() {
        let mut s = Settings {
            noteskin: Some("bundled:dance/dito".into()),
            pump_noteskin: Some("bundled:dance/spiral base".into()),
            skin_params: [
                (
                    "bundled:dance/dito".into(),
                    Values::from([
                        ("Colors".into(), ParamValue::Text("Rainbow".into())),
                        ("Explosions".into(), ParamValue::Bool(false)),
                    ]),
                ),
                (
                    INFINITO.into(),
                    Values::from([("Colors".into(), ParamValue::Text("Flat".into()))]),
                ),
            ]
            .into(),
            ..Settings::default()
        };
        s.migrate();
        assert_eq!(s.noteskin.as_deref(), Some(INFINITO));
        assert_eq!(s.pump_noteskin.as_deref(), Some(INFINITO));
        assert_eq!(s.note_colors(), NoteColors::Flat);
        assert_eq!(
            s.skin_params(INFINITO).get("Explosions"),
            Some(&ParamValue::Bool(false))
        );
        assert!(!s.skin_params.contains_key("bundled:dance/dito"));
        let again = s.clone();
        s.migrate();
        assert_eq!(s, again);
    }

    #[test]
    fn skins_parameters_are_kept_within_bounds() {
        let mut s = Settings::default();
        s.set_infinito_param("ShockArrows", ParamValue::Bool(true));
        assert_eq!(
            s.skin_params(INFINITO).get("ShockArrows"),
            Some(&ParamValue::Bool(true))
        );
        // Invalid values are forgotten; defaults are not stored.
        s.set_infinito_param("ShockArrows", ParamValue::Text("Plaid".into()));
        assert!(s.skin_params.is_empty());
        for i in 0..100 {
            let mut values = Values::new();
            for j in 0..40 {
                values.insert(format!("p{j}"), ParamValue::Number(j as f64));
            }
            values.insert("x".repeat(100), ParamValue::Bool(true));
            values.insert("long".into(), ParamValue::Text("y".repeat(1000)));
            values.insert("nan".into(), ParamValue::Number(f64::NAN));
            s.skin_params.insert(format!("dance/skin{i}"), values);
        }
        s.sanitize();
        assert_eq!(s.skin_params.len(), MAX_SKINS_WITH_PARAMS);
        for values in s.skin_params.values() {
            assert_eq!(values.len(), ddi_noteskin::params::MAX_PARAMETERS);
            assert!(!values.contains_key("long") && !values.contains_key("nan"));
        }
    }

    #[test]
    fn the_song_lists_best_key_is_the_plays() {
        use ddi_chart::{Chart, DanoniChart, Difficulty};
        let work = |headers: Vec<(&str, &str)>| Chart {
            derivation: None,
            layout: "danoni-7".into(),
            difficulty: Difficulty::Edit,
            meter: 0,
            name: "Normal".into(),
            description: String::new(),
            credit: String::new(),
            notes: Vec::new(),
            timing: None,
            display_bpm: None,
            danoni: Some(DanoniChart {
                headers: headers
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                ..Default::default()
            }),
            ffr: false,
        };
        let plain = Chart {
            danoni: None,
            layout: "dance-single".into(),
            ..work(Vec::new())
        };
        let level = Chart {
            ffr: true,
            ..plain.clone()
        };
        let pump = Chart {
            layout: "pump-single".into(),
            ..plain.clone()
        };
        // A pump chart's version without centers: drawn as dance, its
        // holds still ticked as its game's.
        let centerless = Chart {
            derivation: Some(ddi_chart::simplify::Derivation::WithoutCenter { source: 0 }),
            ..plain.clone()
        };
        let itg = Settings {
            ruleset: "itg".into(),
            ..Settings::default()
        };
        assert_eq!(
            itg.ruleset_for(&centerless).key().best_key(),
            itg.ruleset_for(&pump).key().best_key()
        );
        assert_ne!(
            itg.ruleset_for(&centerless).key().best_key(),
            itg.ruleset_for(&plain).key().best_key()
        );
        // DDI keeps its own holds on every chart: one key.
        assert_eq!(
            Settings::default().ruleset_for(&pump).key().best_key(),
            Settings::default().ruleset_for(&plain).key().best_key()
        );
        for (mode, preset) in [
            (RulesetMode::Original, "itg"),
            (RulesetMode::AllDanoni, "itg"),
            (RulesetMode::AllStepMania, "itg"),
            (RulesetMode::Original, "ddi"),
            (RulesetMode::AllStepMania, "ddi"),
        ] {
            for excessive in [None, Some(true), Some(false)] {
                let mut s = Settings {
                    ruleset_mode: mode,
                    ruleset: preset.into(),
                    ddi: DdiOptions {
                        difficulty: ddi_engine::rules::ddi::Difficulty::Hard,
                        boos: true,
                        practice: true,
                    },
                    stepmania: StepManiaOptions {
                        fail: ddi_engine::rules::FailType::Off,
                        life: 6,
                        judge: 5,
                        life_type: ddi_engine::rules::LifeType::Battery,
                        lives: 2,
                        gauge: ddi_engine::rules::ddr::DdrGauge::Life4,
                    },
                    ..Settings::default()
                };
                s.danoni.excessive = excessive;
                for chart in [
                    work(vec![("excessiveJdgUse", "true")]),
                    work(vec![("excessiveUse", "true,on")]),
                    work(Vec::new()),
                    plain.clone(),
                    level.clone(),
                    pump.clone(),
                    centerless.clone(),
                ] {
                    let play = s.ruleset_for(&chart).key().best_key();
                    let list = s.best_key_for(
                        &chart.layout,
                        chart.derivation.as_ref(),
                        chart.danoni.is_some(),
                        chart.ffr,
                        chart_excessive(&chart),
                    );
                    assert_eq!(list, play, "{mode:?} {excessive:?} {:?}", chart.danoni);
                }
            }
        }
        assert_eq!(
            chart_excessive(&work(vec![("excessiveJdgUse", "true")])),
            Some(true)
        );
        assert_eq!(chart_excessive(&work(Vec::new())), Some(false));
        assert_eq!(chart_excessive(&plain), None);
        // An FFR level plays by FFR's rules where each chart is left to
        // its game, else by the preset chosen.
        let mut s = Settings {
            ruleset_mode: RulesetMode::Original,
            ..Settings::default()
        };
        assert_eq!(s.ruleset_for(&level).id, "ffr");
        assert_eq!(s.ruleset_for(&plain).id, "ddi");
        s.ruleset_mode = RulesetMode::AllStepMania;
        assert_eq!(s.ruleset_for(&level).id, "ddi");
        s.ruleset = "ffr".into();
        assert_eq!(s.ruleset_for(&plain).id, "ffr");
        s.ruleset_mode = RulesetMode::AllDanoni;
        assert_eq!(s.ruleset_for(&level).id, "danoni");
    }
}

#[cfg(test)]
mod dancing_hand_tests {
    use super::*;
    #[test]
    fn hand_mode_defaults_on_for_supported_layouts() {
        let mut s: Settings = serde_json::from_str("{}").unwrap();
        assert!(s.dancing_hand);
        s.dancing_hand = true;
        assert!(s.dancing_hand_for("dance-single"));
        assert!(s.dancing_hand_for("danoni-5"));
        let original = s.transform;
        let off: Settings = serde_json::from_str(r#"{"dancing_hand":false}"#).unwrap();
        for layout in [
            "dance-single",
            "dance-double",
            "dance-solo",
            "danoni-5",
            "pump-single",
            "pump-halfdouble",
            "pump-double",
            "danoni-7",
            "techno-single8",
            "danoni-9B",
            "danoni-9A",
        ] {
            assert!(s.dancing_hand_for(layout), "{layout}");
            assert!(!off.dancing_hand_for(layout), "{layout}");
        }
        assert!(s.dancing_hand_for("danoni-9A"));
        assert!(s.dancing_hand_for("pump-double"));
        assert_eq!(s.transform, original);
    }

    /// The hand in a multiplayer stage is the player's own choice, Off
    /// by default and where a stored setting predates it; a play that is
    /// no stage is untouched by it.
    #[test]
    fn the_hand_in_multiplayer_is_off_unless_chosen() {
        let stored: Settings = serde_json::from_str(r#"{"dancing_hand":true}"#).unwrap();
        assert!(!stored.dancing_hand_multiplayer);
        assert!(!Settings::default().dancing_hand_multiplayer);
        assert!(stored.dancing_hand_in("dance-single", false));
        assert!(!stored.dancing_hand_in("dance-single", true));
        let mut on = stored.clone();
        on.dancing_hand_multiplayer = true;
        assert!(on.dancing_hand_in("dance-single", true));
        let round: Settings = serde_json::from_str(&serde_json::to_string(&on).unwrap()).unwrap();
        assert!(round.dancing_hand_multiplayer);
        on.dancing_hand = false;
        assert!(!on.dancing_hand_in("dance-single", true));
        assert!(!on.dancing_hand_in("dance-single", false));
    }

    /// Kept until what is stored changes, and changing with it.
    #[test]
    fn cached_settings_follow_what_is_stored() {
        storage::install(std::rc::Rc::new(storage::MemoryStore::default()));
        assert_eq!(Settings::cached(), Settings::load());
        let mut s = Settings::load();
        s.speed = 3.0;
        s.save();
        assert_eq!(Settings::cached().speed, 3.0);
        assert_eq!(Settings::cached().speed, 3.0);
    }

    /// The cases of `locales/system-language-cases.txt`: the system's
    /// languages, the settings as stored, the address's `lang=`, and the
    /// language the game speaks as it starts (the settings loaded, then
    /// their language chosen, then the address's forced, as the web's
    /// start and router do). The web page's loader is held to the same
    /// lines (`xtask`'s `the_loader_chooses_each_case_s_language`).
    #[test]
    fn the_game_chooses_each_case_s_language() {
        use ddi_platform::storage::MemoryStore;
        use std::rc::Rc;
        let cases = include_str!("../../../../locales/system-language-cases.txt");
        let mut n = 0;
        for line in cases.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('|').map(str::trim).collect();
            let [system, stored, forced, want] = fields[..] else {
                panic!("not a case: {line}");
            };
            storage::install(Rc::new(MemoryStore::default()));
            if stored != "-" {
                storage::set(Area::Local, KEY, stored).unwrap();
            }
            let system: Vec<&str> = match system {
                "-" => Vec::new(),
                s => s.split(',').map(str::trim).collect(),
            };
            ddi_l10n::set_system_languages(&system);
            ddi_l10n::unforce();
            Settings::load().choose_language();
            if forced != "-" && !forced.is_empty() {
                ddi_l10n::force(forced);
            }
            assert_eq!(ddi_l10n::current().tag, want, "{line}");
            ddi_l10n::unforce();
            n += 1;
        }
        assert!(n > 10, "{n} cases");
        ddi_l10n::set_system_languages(Vec::<String>::new());
        ddi_l10n::choose(Some("en"));
    }
}
