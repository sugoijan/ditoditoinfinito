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
}

impl Kept {
    pub const fn new() -> Kept {
        Kept {
            online: RefCell::new(None),
            room: RefCell::new(None),
            audience: RefCell::new(None),
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

    /// The signed-in player's display name, where the online state exists
    /// and is signed in (never made for the asking): CPU practice's own
    /// row shows it with a "You" badge.
    pub fn signed_in_name(&self) -> Option<String> {
        let online = self.online.borrow().clone()?;
        let o = online.borrow();
        o.me().map(|me| me.display_name.clone())
    }

    /// Online room `id`'s client: the one kept where it is that room's,
    /// else made by `make` (and joined) in its place.
    pub fn room_or(&self, id: &str, make: impl FnOnce() -> SharedRoom) -> SharedRoom {
        let kept = self
            .room
            .borrow()
            .as_ref()
            .filter(|(i, _)| i == id)
            .map(|(_, r)| r.clone());
        if let Some(r) = kept {
            return r;
        }
        let room = make();
        *self.room.borrow_mut() = Some((id.into(), room.clone()));
        room
    }

    /// The online room kept, if any.
    pub fn room(&self) -> Option<SharedRoom> {
        self.room.borrow().as_ref().map(|(_, r)| r.clone())
    }

    /// Leave room: the room's client let go (its socket closed once the
    /// room answered), handed back for the shell to forget wherever else
    /// it holds it.
    pub fn discard_room(&self) -> Option<SharedRoom> {
        self.room.borrow_mut().take().map(|(_, r)| r)
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
