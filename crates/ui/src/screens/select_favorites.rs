//! Favorite songs on song select (`docs/plans/song-navigation.md`,
//! "Favorites"; the maintainer, 2026-11-18).
//!
//! - **A song is a favorite or not** (the song, not a chart), kept on the
//!   device ([`crate::state::favorites`]).
//! - **The heart** on the song's side, at its title's right (the look's
//!   `FavoriteSlot` in its `SongInfo`): an outline, filled for a
//!   favorite, a button a tap or a click turns; F (by its place on the
//!   keyboard) does the same, its word in the footer; More has it for a
//!   controller ("Add to favorites", "Remove from favorites"). A change is
//!   at once, asks nothing and says nothing; the heart pops (not under
//!   reduced motion). A favorite's row carries a small filled heart
//!   (`Favorite`, `NotFavorite` on the row: the look's `Fav`).
//! - **The favorites' pack** at the top of the list, above every pack,
//!   while one of its songs is in the library: each favorite once, from
//!   whatever pack it is in (listed there too), sorted as a pack's songs
//!   are (by title). It opens, closes and keeps its place as a pack does
//!   (its key [`crate::state::place::FAVORITES_KEY`]); while searching it
//!   is left out, so a song shows once, under its own pack, with its
//!   heart. Nothing a pack's header offers (Manage, Share…) is offered on
//!   its header. A song taken out of it while it is open gives the focus
//!   to the next of its songs (its header after the last); the pack gone,
//!   to the first pack's header. A library of fewer than ten songs has no
//!   packs, so no favorites' pack either: their rows' hearts alone.
//! - **Where**: singleplayer, and in a room the picker's own list (a
//!   follower's side and a share's songs have no heart and no pack).
//!
//! A child module of `select` (it reads the screen's own fields).

use super::*;
use crate::state::favorites;
use crate::state::place::FAVORITES_PACK;

/// The song's heart's button (a tap or a click turns it) and More's item.
pub(super) const FAVORITE: &str = "Favorite";

/// The favorites' pack over `songs`, those `favorite` marks, sorted as a
/// pack's songs are (by title, ignoring case, then as listed); none where
/// there is none.
pub(super) fn favorites_pack(
    songs: &[SongEntry],
    favorite: &[bool],
) -> Option<(Option<String>, Vec<usize>)> {
    let mut list: Vec<usize> = (0..songs.len())
        .filter(|&i| favorite.get(i).copied().unwrap_or(false))
        .collect();
    if list.is_empty() {
        return None;
    }
    list.sort_by_cached_key(|&i| (songs[i].title.to_lowercase(), i));
    Some((Some(FAVORITES_PACK.to_string()), list))
}

impl SelectScreen {
    /// Whether favorites apply to a list of `data`: not a share's songs,
    /// not a room's follower's side (the picker's own list has them).
    pub(super) fn favorites_apply(&self, data: &SelectData) -> bool {
        // Never on a room's own songs (revision 17): one flat list.
        data.shared.is_none() && !self.following() && !self.room_listing()
    }

    /// Whether the list has the favorites' pack (always its first).
    pub(super) fn has_favorites_pack(&self) -> bool {
        self.packs
            .first()
            .is_some_and(|(name, _)| is_favorites(name.as_deref()))
    }

    /// The first pack not in `closed` (never the favorites': it opens only
    /// where it was open).
    pub(super) fn first_open(&self, closed: &HashSet<&Option<String>>) -> Option<usize> {
        self.packs
            .iter()
            .position(|(n, _)| !is_favorites(n.as_deref()) && !closed.contains(n))
    }

    /// Song `s`'s own pack (never the favorites').
    pub(super) fn own_pack(&self, s: usize) -> Option<usize> {
        self.packs
            .iter()
            .position(|(n, v)| !is_favorites(n.as_deref()) && v.contains(&s))
    }

    /// The pack song `s` is in as the list shows it: the open one where
    /// it holds the song (the favorites' too), else its own.
    pub(super) fn pack_of(&self, s: usize) -> Option<usize> {
        self.open_pack
            .filter(|&p| self.packs.get(p).is_some_and(|(_, v)| v.contains(&s)))
            .or_else(|| self.own_pack(s))
    }

    /// The song's heart: whether the song shown is a favorite; `None`
    /// where it shows none (favorites do not apply, no song, a room not
    /// known yet).
    pub(super) fn heart(&self) -> Option<bool> {
        if !self.favorites_apply(&self.data) || self.room_joining() {
            return None;
        }
        match self.shown? {
            Item::Song(s) if s < self.data.songs.len() => {
                Some(self.favorite.get(s).copied().unwrap_or(false))
            }
            _ => None,
        }
    }

    /// More's item for the song shown's heart.
    pub(super) fn favorite_item(&self) -> Option<(String, String)> {
        let label = if self.heart()? {
            msg::more_favorite_remove()
        } else {
            msg::more_favorite_add()
        };
        Some((FAVORITE.to_string(), label.into()))
    }

    /// The song's heart on its side, as it is now (`pop`: just turned, so
    /// it pops, but under reduced motion): in the look's `FavoriteSlot`,
    /// else at the info's upper right (a look without one).
    pub(super) fn fill_heart(&self, root: &mut Actor, pop: bool) {
        let Some(info) = hit::find_mut(root, "Info") else {
            return;
        };
        let shown = self.heart();
        if part(info, "FavoriteSlot").is_none() {
            let mut slot = frame("FavoriteSlot", vec![quad("Extent", [20.0, 20.0], [0.0; 4])]);
            if let Some(e) = part(&mut slot, "Extent") {
                e.visible = false;
            }
            call(&mut slot, "xy", &[self.layout.info_w - 14.0, 14.0]);
            if let Kind::Frame(children) = &mut info.kind {
                children.push(slot);
            }
        }
        let Some(slot) = part(info, "FavoriteSlot") else {
            return;
        };
        let height = super::extent(slot, [20.0, 20.0])[1];
        let Kind::Frame(children) = &mut slot.kind else {
            return;
        };
        children.retain(|c| c.name != FAVORITE);
        let Some(on) = shown else {
            return;
        };
        // The heart, and round it a hit area larger than it (drawn,
        // faintly: `crate::hit`).
        let side = height + 16.0;
        let hit = quad("Hit", [side, side], [0.0, 0.0, 0.0, 0.004]);
        let mut heart = crate::heart::heart(&self.env, "Heart", on, height);
        if pop && !self.reduced {
            look::run(&mut heart, "stoptweening;zoom,1.3;decelerate,0.22;zoom,1");
        }
        children.push(frame(FAVORITE, vec![hit, heart]));
    }

    /// Turns the song shown a favorite or not: at once, kept at once, its
    /// heart and its row, and the favorites' pack, follow.
    pub(super) fn toggle_favorite(&mut self, root: &mut Actor, cx: &mut Cx) {
        if self.heart().is_none() {
            return;
        }
        self.follow();
        let Some(Item::Song(s)) = self.shown else {
            return;
        };
        let id = self.data.songs[s].id.clone();
        let on = favorites::toggle(&id);
        if self.favorite.len() != self.data.songs.len() {
            self.favorite = vec![false; self.data.songs.len()];
        }
        self.favorite[s] = on;
        self.relist_favorites(s, on);
        self.follow();
        // The song shown may have changed (taken out of the open pack).
        if self.shown == Some(Item::Song(s)) {
            self.fill_heart(root, true);
        } else {
            self.left_dirty = true;
        }
        self.hints = None;
        self.publish_dirty = true;
        self.status(format!(
            "{} {id}",
            if on { "favorite" } else { "unfavorite" }
        ));
        let _ = cx;
    }

    /// The favorites' pack made again after song `s` was turned (`on`: a
    /// favorite now): its rows, the pack open kept, the selection where it
    /// was (a song taken out of the open favorites' pack gives way to the
    /// next, its header after the last, the first pack's header with the
    /// pack gone), the selected row kept where it shows.
    fn relist_favorites(&mut self, s: usize, on: bool) {
        let had = self.has_favorites_pack();
        let old: Vec<usize> = if had {
            self.packs[0].1.clone()
        } else {
            Vec::new()
        };
        let in_favorites = had && self.open_pack == Some(0) && self.matches.is_none();
        let selected = self.selected_item();
        let top = self.list.selected().and_then(|i| self.list.item_top(i));
        if had {
            self.packs.remove(0);
        }
        let pack = if self.flat() {
            None
        } else {
            favorites_pack(&self.data.songs, &self.favorite)
        };
        let has = pack.is_some();
        if let Some(pack) = pack {
            self.packs.insert(0, pack);
        }
        let now: &[usize] = if has { &self.packs[0].1 } else { &[] };
        if had != has || old != now {
            self.packs_version += 1;
        }
        // A real pack's place moves by the favorites' pack come or gone.
        let moved = |p: usize| p - usize::from(had) + usize::from(has);
        self.open_pack = match self.open_pack {
            Some(0) if had => has.then_some(0),
            Some(p) => Some(moved(p)),
            None => None,
        };
        let target = match selected {
            Some(Item::Header(0)) if had => Some(Item::Header(0)),
            Some(Item::Header(p)) => Some(Item::Header(moved(p))),
            Some(Item::Song(x)) if x == s && in_favorites && !on => {
                if has {
                    let k = old.iter().position(|&i| i == s).unwrap_or(0);
                    Some(
                        self.packs[0]
                            .1
                            .get(k)
                            .map_or(Item::Header(0), |&next| Item::Song(next)),
                    )
                } else {
                    // The pack gone: the first pack's header.
                    Some(Item::Header(0))
                }
            }
            other => other,
        };
        self.lay_out_items();
        self.reload();
        let at = target.and_then(|t| self.items.iter().position(|it| *it == t));
        if let Some(at) = at {
            if let Some(top) = top {
                self.list.keep_item_at(at, top);
            }
            self.list.select(at);
        }
    }
}
