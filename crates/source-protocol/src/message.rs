//! Version 1's messages (`docs/plans/source-plugins.md`, "Messages").
//!
//! Each is a JSON object with its kind in `type` (snake_case), every
//! object refusing fields it does not know. Words never stand where a
//! value belongs: progress steps, failure codes and capabilities are
//! closed sets the app words in the player's language.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::text::Text;

/// An operation's id, chosen by the app (not 0), unique while it runs.
pub type OpId = u32;

/// What the app sends a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppMessage {
    /// The first message: the versions the app speaks, its optional
    /// features, who it is, and the plugin's own folders.
    Hello {
        protocols: Vec<u32>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        features: Vec<String>,
        app: AppInfo,
        dirs: Dirs,
    },
    /// Resolve what the player gave into a scope (`input`).
    Open { op: OpId, input: Input },
    /// A page of search results (`search`, `filters`).
    Search {
        op: OpId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        query: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        filters: BTreeMap<String, FilterValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        page_size: u32,
    },
    /// A page of a node's children and items (`browse`; the scope's top
    /// without a node).
    Browse {
        op: OpId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        page_size: u32,
    },
    /// An item's metadata before it is fetched (`metadata`).
    Describe { op: OpId, item: String },
    /// The item's files, delivered as `deliver` asks (`fetch`).
    Fetch {
        op: OpId,
        item: String,
        deliver: Deliver,
    },
    /// End `op` soon (within five seconds), with `Failed { cancelled }`
    /// or its ordinary end.
    Cancel { op: OpId },
    /// Answer `Bye` and exit.
    Shutdown {},
}

/// What a plugin sends the app.
#[allow(clippy::large_enum_variant)] // Read once each, never kept in bulk.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginMessage {
    /// The answer to the app's Hello: the version chosen (one the app
    /// listed; where none is shared, the newest the plugin speaks, after
    /// which it exits), its optional features, who it is and what it does
    /// (a subset of its manifest's).
    Hello {
        protocol: u32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        features: Vec<String>,
        plugin: PluginInfo,
        capabilities: Vec<Capability>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        inputs: Vec<InputKind>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        filters: Vec<Filter>,
    },
    /// `Open`'s scope, ending it.
    Opened {
        op: OpId,
        scope: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<Text>,
    },
    /// Items of a search's or a browse's page (repeated, at most the
    /// page's size in all).
    Results { op: OpId, items: Vec<ItemMeta> },
    /// Nodes of a browse's page (counted with its items).
    Nodes { op: OpId, nodes: Vec<Node> },
    /// The page's end: the cursor of the next one, if any, and how many
    /// there are in all, if known.
    Done {
        op: OpId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
    },
    /// `Describe`'s answer, ending it.
    Item { op: OpId, item: ItemMeta },
    /// How an operation is going.
    Progress {
        op: OpId,
        step: Step,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        done: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bytes: Option<u64>,
    },
    /// A chunk of a fetched file, its bytes the frame's attachment: the
    /// file's whole `size`, where the chunk starts, and whether it is the
    /// last. A file's chunks come in order; files may interleave.
    File {
        op: OpId,
        path: String,
        size: u64,
        offset: u64,
        last: bool,
    },
    /// The fetch's end: where the item's bytes are.
    Fetched { op: OpId, delivery: Delivery },
    /// The operation failed, as a value, with the plugin's own words as
    /// data.
    Failed {
        op: OpId,
        code: FailCode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// The answer to `Shutdown`: the plugin exits next.
    Bye {},
}

impl PluginMessage {
    /// The operation it belongs to (`None`: Hello and Bye).
    pub fn op(&self) -> Option<OpId> {
        match self {
            PluginMessage::Hello { .. } | PluginMessage::Bye {} => None,
            PluginMessage::Opened { op, .. }
            | PluginMessage::Results { op, .. }
            | PluginMessage::Nodes { op, .. }
            | PluginMessage::Done { op, .. }
            | PluginMessage::Item { op, .. }
            | PluginMessage::Progress { op, .. }
            | PluginMessage::File { op, .. }
            | PluginMessage::Fetched { op, .. }
            | PluginMessage::Failed { op, .. } => Some(*op),
        }
    }
}

/// The app (`AppMessage::Hello`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppInfo {
    /// The game's version.
    pub version: String,
    /// The player's language for the menus (a tag), for text the plugin
    /// chooses.
    pub language: String,
}

/// The plugin's own folders, for its own use: its cache (the app's cache
/// folder's `plugins/<id>/`) and its data (the data folder's
/// `plugin-data/<id>/`). They may not exist yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dirs {
    pub cache: PathBuf,
    pub data: PathBuf,
}

/// Who the plugin is (`PluginMessage::Hello`): its manifest's id and
/// version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginInfo {
    pub id: String,
    pub version: String,
}

/// What a source can do; each is optional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Resolves what the player gives into a scope ([`InputKind`]).
    Input,
    Search,
    /// Search has filters (declared in Hello).
    Filters,
    Browse,
    Metadata,
    Fetch,
    /// Reserved: not in version 1 (the app never offers it).
    Signin,
    /// Reserved: not in version 1 (the app never offers it).
    Upload,
}

impl Capability {
    /// Not in version 1: never offered, whatever a plugin declares.
    pub fn reserved(self) -> bool {
        matches!(self, Capability::Signin | Capability::Upload)
    }
}

/// What the player may give a source (`input`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    /// A folder chosen in the app's folder dialog, as a path.
    Folder,
    /// A pasted link.
    Url,
    /// A link the app routes to the plugin whose manifest claims it.
    Link,
    /// Free text.
    Text,
}

/// `Open`'s input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub kind: InputKind,
    pub value: String,
}

/// Where the app wants a fetched item's bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Deliver {
    /// Streamed over the pipe (`files`, `archive`), or where they already
    /// are (`in_place`); about `budget` bytes in all fit in memory (past
    /// it the app keeps them in its cache folder itself).
    Memory { budget: u64 },
    /// Written into `path`, a folder the app made for this operation in
    /// its cache folder (`cached`): the item is over the memory budget.
    CacheDir { path: PathBuf },
}

/// Where a fetched item's bytes are (`Fetched`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Delivery {
    /// The files streamed as `File` frames: how many and their bytes.
    Files { files: u32, bytes: u64 },
    /// One zip (or `.smzip`) streamed as `File` frames under `path`.
    Archive { path: String, bytes: u64 },
    /// The files written into the `CacheDir` the app gave: how many and
    /// their bytes.
    Cached { files: u32, bytes: u64 },
    /// A folder or a zip already on the player's disk (an absolute path):
    /// the app reads it where it is.
    InPlace { path: PathBuf },
}

/// What an operation is doing: the app words it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Resolving,
    Searching,
    Downloading,
    Reading,
    Converting,
    Checking,
}

/// Why an operation failed: the app words it; the plugin's own words go
/// in `detail`, shown as data in a fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailCode {
    Cancelled,
    Busy,
    InvalidInput,
    UnsupportedInput,
    /// The plugin does not do what was asked (a capability it lacks).
    Unsupported,
    NotFound,
    Gone,
    RefusedBySource,
    RateLimited,
    Unreachable,
    TooLarge,
    UnreadableContent,
    SourceChanged,
    Internal,
}

/// An item: a song or a pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Song,
    Pack,
}

/// What a source says of an item before it is fetched: a description for
/// the screens. What the app plays is what it reads from the files.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemMeta {
    /// The source's id for it, stable for that source.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ItemKind>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_translit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle_translit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist_translit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charts: Vec<ChartMeta>,
    /// The music's length, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// The lowest and highest BPM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<[f64; 2]>,
    /// The bytes a fetch brings, as far as the source knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// A pack's songs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub songs: Option<u32>,
    /// The source's page for it (http or https).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// Its license, as the source states it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Who to credit, as the source states it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// When it last changed (a date as the source writes it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    /// An opaque revision: a fetch of another revision is another item
    /// for the app's memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// The game's `song_key` (`s1:` and 16 hex digits) where the source
    /// can compute it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song_key: Option<String>,
    /// Narrows the manifest's room rule for this item, never widens it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rooms: Option<RoomRule>,
}

/// A chart as a source describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartMeta {
    /// The game's layout id (`dance-single`, `pump-double`…).
    pub style: String,
    pub difficulty: Difficulty,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meter: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// The game's six difficulties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Beginner,
    Easy,
    Medium,
    Hard,
    Challenge,
    Edit,
}

/// A node of a source's tree (a pack, a category, a folder).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub title: String,
    /// Its items, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<u64>,
    /// Its child nodes, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
}

/// Whether a source's songs may be added to an online room.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomRule {
    /// Allowed (the default where a plugin says nothing: decided
    /// 2026-11-26).
    #[default]
    Upload,
    /// Not offered for the songs it fetched.
    None,
}

/// A search filter the app draws from its declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub id: String,
    pub label: Text,
    pub kind: FilterKind,
}

/// What a filter is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FilterKind {
    /// Free text, of at most `max_chars` (and [`crate::limits::MAX_FILTER_TEXT_CHARS`]).
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_chars: Option<u32>,
    },
    /// One of the choices, or several where `multiple`.
    Choice {
        #[serde(default)]
        multiple: bool,
        choices: Vec<Choice>,
    },
    /// A range of numbers.
    Range {
        min: f64,
        max: f64,
        step: f64,
        unit: Unit,
    },
    /// On or off.
    Toggle {},
}

/// A choice filter's choice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: String,
    pub label: Text,
}

/// A range filter's unit, which the app words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Meter,
    Bpm,
    Seconds,
    Bytes,
    Count,
}

/// A filter's value in a search.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FilterValue {
    Text {
        value: String,
    },
    Choice {
        values: Vec<String>,
    },
    Range {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Toggle {
        value: bool,
    },
}
