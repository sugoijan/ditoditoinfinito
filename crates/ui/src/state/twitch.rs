//! What this device asks of Twitch (`docs/plans/multiplayer-twitch.md`,
//! part A): the scopes a feature needs, asked when the feature is turned
//! on, through the server's second trip through Twitch's consent.
//!
//! INTEGRATION POINT (round TW2): the trip itself is the Twitch link's
//! (A1, A5), not built yet. [`want_chat_scope`] is what Options calls when
//! "Twitch chat in rooms" is turned on; today it only records the wish,
//! which the link's round turns into the trip (`user:read:chat`) and the
//! screens read back through [`chat_scope_wished`].

use std::cell::Cell;

thread_local! {
    static CHAT_WISHED: Cell<Option<ddi_chat::Level>> = const { Cell::new(None) };
}

/// "Twitch chat in rooms" was turned on at `level` on this device: the
/// device will need `user:read:chat` (A1). Recorded only, for now.
pub fn want_chat_scope(level: ddi_chat::Level) {
    CHAT_WISHED.set((level != ddi_chat::Level::Off).then_some(level));
}

/// The chat's scope wished for this session, if it was.
pub fn chat_scope_wished() -> Option<ddi_chat::Level> {
    CHAT_WISHED.get()
}
