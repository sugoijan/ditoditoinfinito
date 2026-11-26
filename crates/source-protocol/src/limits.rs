//! Every size and count of version 1. The app enforces them whatever the
//! plugin (`docs/plans/source-plugins.md`, "What the game still does,
//! whatever the plugin"); a plugin past one is misbehaving, and the host
//! stops it.

/// A frame after its length: the JSON's length, the JSON and the
/// attachment together.
pub const MAX_FRAME_BYTES: usize = 4 << 20;
/// A frame's attachment (a fetched file's chunk).
pub const MAX_ATTACHMENT_BYTES: usize = 1 << 20;

/// Operations running at once on one plugin.
pub const MAX_OPS: usize = 4;
/// An item's, a node's, a scope's or a filter's id, in bytes.
pub const MAX_ID_BYTES: usize = 256;
/// A paging cursor, in bytes (opaque to the app).
pub const MAX_CURSOR_BYTES: usize = 1024;
/// A search's query, in characters.
pub const MAX_QUERY_CHARS: usize = 256;
/// Items (or nodes) one search or browse answers: a page.
pub const MAX_PAGE: u32 = 50;
/// What the player gives an `Open` (a path, a link, text), in bytes.
pub const MAX_INPUT_BYTES: usize = 4096;
/// A label of the plugin's (its name, a filter's, a choice's), in
/// characters.
pub const MAX_LABEL_CHARS: usize = 128;
/// An item's text (title, artist, license, attribution…), in characters.
pub const MAX_ITEM_TEXT_CHARS: usize = 512;
/// A failure's detail, the plugin's own words, in bytes.
pub const MAX_DETAIL_BYTES: usize = 2048;
/// Languages a label is given in.
pub const MAX_LANGUAGES: usize = 16;
/// A language tag, in bytes.
pub const MAX_TAG_BYTES: usize = 35;
/// Filters a plugin declares.
pub const MAX_FILTERS: usize = 32;
/// Choices a choice filter offers.
pub const MAX_CHOICES: usize = 256;
/// Charts an item's metadata lists.
pub const MAX_CHARTS: usize = 64;
/// Optional features a Hello names (minor additions to a version).
pub const MAX_FEATURES: usize = 32;
/// A feature's name, in bytes.
pub const MAX_FEATURE_BYTES: usize = 64;
/// A link (an item's page, a manifest's homepage), in bytes.
pub const MAX_URL_BYTES: usize = 2048;
/// A plugin's or the app's version, a publisher, in characters.
pub const MAX_NAME_CHARS: usize = 128;
/// A fetched file's path inside its item, in bytes.
pub const MAX_PATH_BYTES: usize = 512;
/// A fetched file's path's components.
pub const MAX_PATH_DEPTH: usize = 16;
/// Files one fetched item holds.
pub const MAX_FILES: usize = 4096;
/// Songs one fetched item holds.
pub const MAX_SONGS: usize = 256;
/// A filter's text value, in characters.
pub const MAX_FILTER_TEXT_CHARS: usize = 256;
/// Links a manifest claims (`input = "link"`).
pub const MAX_LINKS: usize = 16;
/// A manifest file, in bytes.
pub const MAX_MANIFEST_BYTES: usize = 64 << 10;
