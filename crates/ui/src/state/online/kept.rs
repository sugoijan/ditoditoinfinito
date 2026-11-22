//! What a shell keeps of the online session: the online state, the
//! online room shown last (by its id) and the room watched with its
//! audience (by its link), each made at first by the shell's own makers
//! (its transport, clock and client id) and let go when left. Both shells
//! keep one in a thread-local; nothing is borrowed while a maker runs, so
//! a maker may ask the same holder for the online state.

use std::cell::RefCell;

use super::SharedOnline;
use crate::state::audience::SharedAudience;
use crate::state::room::SharedRoom;

/// The online session's kept clients.
#[derive(Default)]
pub struct Kept {
    online: RefCell<Option<SharedOnline>>,
    room: RefCell<Option<(String, SharedRoom)>>,
    audience: RefCell<Option<(String, SharedAudience)>>,
    /// Online rooms left whose Leave the room has not read yet (their
    /// socket was down): their links driven until it has
    /// ([`Self::drive_leaving`]).
    leaving: RefCell<Vec<SharedRoom>>,
}

impl Kept {
    pub const fn new() -> Kept {
        Kept {
            online: RefCell::new(None),
            room: RefCell::new(None),
            audience: RefCell::new(None),
            leaving: RefCell::new(Vec::new()),
        }
    }

    /// The online state, made by `make` at first.
    pub fn online_or(&self, make: impl FnOnce() -> SharedOnline) -> SharedOnline {
        if let Some(o) = self.online.borrow().clone() {
            return o;
        }
        let o = make();
        *self.online.borrow_mut() = Some(o.clone());
        o
    }

    /// The online state, where it was made (never made for the asking).
    pub fn online(&self) -> Option<SharedOnline> {
        self.online.borrow().clone()
    }

    /// The signed-in player's display name, where the online state exists
    /// and is signed in (never made for the asking): CPU practice's own
    /// row shows it with a "You" badge.
    pub fn signed_in_name(&self) -> Option<String> {
        let online = self.online.borrow().clone()?;
        let o = online.borrow();
        o.me().map(|me| me.display_name.clone())
    }

    /// The person's Twitch access as `/me` last said it, for Options →
    /// Data's row (round TW1b; never made for the asking).
    pub fn twitch_row(&self) -> crate::screens::options_model::TwitchRow {
        let Some(online) = self.online.borrow().clone() else {
            return Default::default();
        };
        let o = online.borrow();
        match o.twitch() {
            Some(t) => crate::screens::options_model::TwitchRow {
                signed_in: true,
                features: t.features.clone(),
                login: t.login.clone().unwrap_or_default(),
            },
            None => Default::default(),
        }
    }

    /// Disconnect the person's Twitch access (Data's Disconnect, A4): the
    /// server deletes it and revokes it at Twitch; the kept room whose
    /// Twitch setting is theirs turns it off at once (nobody removed),
    /// rather than at its next check.
    pub fn disconnect_twitch(&self) {
        if let Some((_, room)) = self.room.borrow().clone() {
            let mut r = room.borrow_mut();
            if r.twitch().is_some_and(|t| t.by == r.me()) {
                r.set_twitch(ddi_room::TwitchMirror::Off, ddi_room::TwitchChat::Off);
            }
        }
        if let Some(online) = self.online.borrow().clone() {
            online.borrow_mut().disconnect_twitch();
        }
        // Round TB: not asked for again at the next sign-in, whatever
        // this run knew of the access.
        crate::state::twitch::forget_used();
    }

    /// Online room `id`'s client: the one kept where it is that room's,
    /// else made by `make` (and joined) in its place, another room kept
    /// left first (told, its socket closed: review3 web-1, never a member
    /// who is gone but still joined).
    pub fn room_or(&self, id: &str, make: impl FnOnce() -> SharedRoom) -> SharedRoom {
        // A link that never reached the room is no room to go back to:
        // one opened at the room's address before its invite was joined
        // (feedback round AA: not signed in, or no member then) is made
        // anew, so the join enters the room rather than its old refusal.
        let never_in = |r: &SharedRoom| {
            r.try_borrow().is_ok_and(|r| {
                matches!(
                    r.link_state(),
                    ddi_room::LinkState::SignInToEnter | ddi_room::LinkState::NotAMember
                )
            })
        };
        let kept = self
            .room
            .borrow()
            .as_ref()
            .filter(|(i, _)| i == id)
            .filter(|(_, r)| !never_in(r))
            .map(|(_, r)| r.clone());
        if let Some(r) = kept {
            return r;
        }
        let other = self.room.borrow_mut().take();
        if let Some((_, old)) = other {
            if let Ok(mut r) = old.try_borrow_mut() {
                r.leave();
            }
            self.keep_leaving(old);
        }
        let room = make();
        *self.room.borrow_mut() = Some((id.into(), room.clone()));
        room
    }

    /// The online room kept, if any.
    pub fn room(&self) -> Option<SharedRoom> {
        self.room.borrow().as_ref().map(|(_, r)| r.clone())
    }

    /// The online room kept's id, if any.
    pub fn room_id(&self) -> Option<String> {
        self.room.borrow().as_ref().map(|(id, _)| id.clone())
    }

    /// Leave room: the room's client let go (its socket closed once the
    /// room answered), handed back for the shell to forget wherever else
    /// it holds it.
    pub fn discard_room(&self) -> Option<SharedRoom> {
        let room = self.room.borrow_mut().take().map(|(_, r)| r)?;
        self.keep_leaving(room.clone());
        Some(room)
    }

    /// A room left whose Leave the room has not read yet kept to be
    /// driven ([`crate::state::room::RoomClient::leaving`], review 2026-11-22, room-client 4).
    fn keep_leaving(&self, room: SharedRoom) {
        if room.try_borrow().is_ok_and(|r| r.leaving()) {
            self.leaving.borrow_mut().push(room);
        }
    }

    /// The rooms left drive their links (`RoomClient::drive_leave`): the
    /// socket opened again and the Leave told, each let go once the room
    /// read it or it was given up (`room::LEAVE_WAIT_MS`). Whether any is
    /// still to be driven (the shell's timer runs while one is).
    pub fn drive_leaving(&self, now_ms: u64) -> bool {
        let rooms = std::mem::take(&mut *self.leaving.borrow_mut());
        let mut still: Vec<SharedRoom> = rooms
            .into_iter()
            .filter(|r| r.try_borrow_mut().map_or(true, |mut c| c.drive_leave(now_ms)))
            .collect();
        let mut leaving = self.leaving.borrow_mut();
        still.append(&mut leaving);
        *leaving = still;
        !leaving.is_empty()
    }

    /// Whether a room left is still to be driven ([`Self::drive_leaving`]).
    pub fn leaving(&self) -> bool {
        !self.leaving.borrow().is_empty()
    }

    /// The room watched by `link`'s seat: the one kept where it is that
    /// link's, else the kept one left ([`Self::discard_audience`]) and
    /// one made by `make` in its place.
    pub fn audience_or(&self, link: &str, make: impl FnOnce() -> SharedAudience) -> SharedAudience {
        let kept = self
            .audience
            .borrow()
            .as_ref()
            .filter(|(l, _)| l == link)
            .map(|(_, a)| a.clone());
        if let Some(a) = kept {
            return a;
        }
        self.discard_audience();
        let a = make();
        *self.audience.borrow_mut() = Some((link.into(), a.clone()));
        a
    }

    /// The seat kept, with its link, if any.
    pub fn audience(&self) -> Option<(String, SharedAudience)> {
        self.audience.borrow().clone()
    }

    /// Stop watching: the seat left (its socket closed) and let go.
    pub fn discard_audience(&self) {
        let kept = self.audience.borrow_mut().take();
        if let Some((_, a)) = kept {
            a.borrow_mut().leave();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::online::Platform;
    use crate::state::room::RoomClient;
    use std::rc::Rc;

    /// A room is kept by its id, made once, and let go whole when left:
    /// nothing of the holder keeps its client alive after.
    #[test]
    fn a_kept_room_is_made_once_and_let_go_when_left() {
        let kept = Kept::new();
        let mut made = 0;
        let room = kept.room_or("Room000001", || {
            made += 1;
            RoomClient::shared(1, 1)
        });
        let again = kept.room_or("Room000001", || unreachable!("kept"));
        assert!(Rc::ptr_eq(&room, &again));
        assert_eq!(made, 1);
        // Another room takes its place.
        let other = kept.room_or("Room000002", || RoomClient::shared(2, 2));
        assert!(!Rc::ptr_eq(&room, &other));
        assert!(Rc::ptr_eq(&kept.room().unwrap(), &other));
        let gone = Rc::downgrade(&other);
        drop((room, again, other));
        let left = kept.discard_room().expect("handed back");
        drop(left);
        assert!(gone.upgrade().is_none(), "the left room's client lives on");
        assert!(kept.room().is_none());
    }

    /// A room replaced by another is left, told (review3 web-1): never a
    /// member gone from the screens who stays joined, connected and
    /// waited for.
    #[test]
    fn a_room_replaced_by_another_is_left() {
        use crate::state::online::tests::room::{client, person, room_of, tick};
        use std::cell::RefCell;
        let clock = ddi_room::ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        let kept = Kept::new();
        let bob = kept.room_or("Room000001", || {
            Rc::new(RefCell::new(client(&server, person(&server, "bob"), "bob", 1_000)))
        });
        for _ in 0..3 {
            tick(&clock, &mut [&mut bob.borrow_mut()], 400);
        }
        assert_eq!(bob.borrow().link_state(), ddi_room::LinkState::Connected);
        let _other = kept.room_or("Room000002", || RoomClient::shared(2, 2));
        for _ in 0..3 {
            tick(&clock, &mut [&mut bob.borrow_mut()], 400);
        }
        let left = server
            .borrow()
            .state()
            .member(ddi_room::ParticipantId(2))
            .is_some_and(|m| m.left);
        assert!(left, "bob left the room he no longer shows");
    }

    /// The online state is made once, and a maker may ask the holder for
    /// it (as a room's maker does); the signed-in name is never made for
    /// the asking.
    #[test]
    fn the_online_state_is_made_once_and_never_for_a_name() {
        let kept = Kept::new();
        assert_eq!(kept.signed_in_name(), None);
        assert!(kept.online.borrow().is_none(), "not made for the asking");
        let room = kept.room_or("Room000001", || {
            let _online = kept.online_or(|| crate::screens::sample::online(Platform::Web));
            RoomClient::shared(1, 1)
        });
        let online = kept.online_or(|| unreachable!("kept"));
        assert!(Rc::ptr_eq(&online, &kept.online_or(|| unreachable!())));
        assert_eq!(kept.signed_in_name(), None, "signed out");
        drop(room);
    }
}
