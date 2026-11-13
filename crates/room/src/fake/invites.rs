//! The room's links as the server answers them (`../ddi-server`'s
//! `RoomCore::preview` and its audience join, `docs/plans/multiplayer-audience.md`,
//! "The server, as built"), so that tests read one description of the
//! server rather than answers of their own:
//!
//! - [`FakeServer::invite_preview`]: `GET /invites/<link>` (the room's own
//!   link `play`, its watch-only link `watch`, its audience link
//!   `audience` with `audience_count`; `audience: true` on the audience
//!   link and at the member bound where the room's audience is open;
//!   `404 invite_unknown` for a link that is none of them).
//! - [`FakeServer::audience_join_answer`]: `POST /rooms/<id>/audience
//!   {invite}`: `{ticket, relay, expires_at_ms}`, or the refusal's status
//!   and `{error}` (any of the three links joins the audience; another
//!   is `404 audience_link_unknown`).
//!
//! The links are the fake's own ([`FakeLinks::of`]), or a test's
//! ([`FakeServer::set_links`]). Invites do not expire or rotate here.

use super::{FakeServer, Latency};
use crate::audience::AudienceRefusal;
use crate::member::{MemberKind, Role};
use crate::rules::Admission;

/// The room's three links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeLinks {
    /// The room's own invite (`kind: play`).
    pub invite: String,
    /// Its watch-only link (`kind: watch`).
    pub watch: String,
    /// Its audience link (`kind: audience`), where its policy has one.
    pub audience: String,
}

impl FakeLinks {
    /// Room `room`'s links as the fake makes them.
    pub fn of(room: &str) -> FakeLinks {
        FakeLinks {
            invite: format!("{room}.abcdefghijABCDEFGHIJ01"),
            watch: format!("{room}.watchwatchWATCHWATCH01"),
            audience: format!("{room}.audienceAUDIENCEaudien01"),
        }
    }
}

/// Which of the room's links a link is (the preview's `kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Play,
    Watch,
    Audience,
}

impl LinkKind {
    fn word(self) -> &'static str {
        match self {
            LinkKind::Play => "play",
            LinkKind::Watch => "watch",
            LinkKind::Audience => "audience",
        }
    }
}

/// Seconds an invite lasts in the server's answer (the fake's never end).
const EXPIRES_AT_MS: i64 = 9_999_999_999_999;

impl FakeServer {
    pub fn links(&self) -> &FakeLinks {
        &self.links
    }

    pub fn set_links(&mut self, links: FakeLinks) {
        self.links = links;
    }

    /// Which of the room's links `link` is (the audience link only where
    /// the room has an audience).
    pub fn link_kind(&self, link: &str) -> Option<LinkKind> {
        if link == self.links.invite {
            Some(LinkKind::Play)
        } else if link == self.links.watch {
            Some(LinkKind::Watch)
        } else if link == self.links.audience && self.audience.is_some() {
            Some(LinkKind::Audience)
        } else {
            None
        }
    }

    /// People present (not left), as the server counts members.
    fn present_humans(&self) -> usize {
        self.state()
            .members
            .values()
            .filter(|m| !m.left && matches!(m.kind, MemberKind::Human { .. }))
            .count()
    }

    /// Whether a newcomer could enter now, as the preview says it.
    fn admission_word(&self) -> &'static str {
        let s = self.state();
        if s.access.admission == Admission::Closed {
            "closed"
        } else if self.present_humans() >= usize::from(s.policy.max_members) {
            "full"
        } else if s.phase.kind().match_running() {
            "next_match"
        } else {
            "open"
        }
    }

    /// `GET /invites/<link>`: the server's answer's body, or its status
    /// and error code.
    pub fn invite_preview(&self, link: &str) -> Result<String, (u16, &'static str)> {
        if self.state().closed.is_some() {
            return Err((410, "room_closed"));
        }
        let kind = self.link_kind(link).ok_or((404, "invite_unknown"))?;
        let s = self.state();
        let owner = s.members.values().find(|m| m.role == Role::Owner);
        let host = owner
            .and_then(|m| match &m.kind {
                MemberKind::Human { name, .. } => Some(name.clone()),
                MemberKind::Cpu { .. } => None,
            })
            .unwrap_or_default();
        let admission = self.admission_word();
        let open = self.audience.as_ref().is_some_and(|a| a.hub.is_open());
        let audience = open && (kind == LinkKind::Audience || admission == "full");
        let mut preview = serde_json::json!({
            "room_id": s.room.0,
            "room_name": self.room_name,
            "host_display_name": host,
            "host_bot": owner.is_some_and(|m| m.is_bot()),
            "phase": s.phase.kind(),
            "admission": admission,
            "players_full": s.players_full(),
            "kind": kind.word(),
            "members": self.present_humans(),
            "expires_at_ms": EXPIRES_AT_MS,
        });
        if audience {
            preview["audience"] = true.into();
        }
        if kind == LinkKind::Audience
            && let Some(a) = &self.audience
        {
            preview["audience_count"] = a.hub.count().into();
        }
        Ok(preview.to_string())
    }

    /// `POST /rooms/<id>/audience {invite: link}` for `identity`: the
    /// server's answer's status and body (`{ticket, relay,
    /// expires_at_ms}`, or `{error}`), the seat's socket `latency` away.
    pub fn audience_join_answer(
        &mut self,
        identity: &str,
        link: &str,
        latency: Option<Latency>,
    ) -> (u16, String) {
        let refused = |r: AudienceRefusal| (r.status(), format!(r#"{{"error":"{}"}}"#, r.code()));
        if self.state().closed.is_some() {
            return refused(AudienceRefusal::RoomClosed);
        }
        if self.audience.is_none() {
            return refused(AudienceRefusal::NoAudience);
        }
        // Any of the room's links joins its audience; another is unknown.
        let key = match self.link_kind(link) {
            Some(_) => self.audience_key().unwrap_or_default(),
            None => return refused(AudienceRefusal::BadLink),
        };
        match self.audience_join(identity, &key, latency) {
            Ok(ticket) => {
                let relay = self.audience_ticket_relay(&ticket).unwrap_or(0);
                (
                    200,
                    serde_json::json!({
                        "ticket": ticket,
                        "relay": relay,
                        "expires_at_ms": self.now() + 60_000,
                    })
                    .to_string(),
                )
            }
            Err(r) => refused(r),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Identity, Intent, ManualClock, RoomPolicy};

    fn fake(policy: RoomPolicy) -> FakeServer {
        let mut f = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            policy,
            ManualClock::new(1_000),
        );
        f.set_room_name("After Hours");
        f.join(Identity::new("dev:bob"), "bob", Intent::Play).unwrap();
        f
    }

    fn json(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn each_link_is_previewed_as_the_server_answers_it() {
        let f = fake(RoomPolicy::default().with_live().with_audience());
        let links = f.links().clone();
        let play = json(&f.invite_preview(&links.invite).unwrap());
        assert_eq!(play["kind"], "play");
        assert_eq!(play["room_name"], "After Hours");
        assert_eq!(play["host_display_name"], "alice");
        assert_eq!(play["phase"], "lobby");
        assert_eq!(play["admission"], "open");
        assert_eq!(play["members"], 2);
        assert_eq!(play["players_full"], false);
        assert!(play.get("audience").is_none() && play.get("audience_count").is_none());
        assert_eq!(json(&f.invite_preview(&links.watch).unwrap())["kind"], "watch");
        let aud = json(&f.invite_preview(&links.audience).unwrap());
        assert_eq!(aud["kind"], "audience");
        assert_eq!(aud["audience"], true);
        assert_eq!(aud["audience_count"], 0);
        assert_eq!(f.invite_preview("Room000001.nothing"), Err((404, "invite_unknown")));
        // A room without an audience has no audience link.
        let plain = fake(RoomPolicy::default().with_live());
        assert_eq!(
            plain.invite_preview(&plain.links().audience.clone()),
            Err((404, "invite_unknown"))
        );
    }

    #[test]
    fn the_audience_join_answers_with_a_ticket_or_the_refusal() {
        let mut f = fake(RoomPolicy::default().with_live().with_audience());
        let links = f.links().clone();
        for link in [&links.audience, &links.invite, &links.watch] {
            let (status, body) = f.audience_join_answer("dev:dora", link, None);
            if status == 200 {
                let b = json(&body);
                assert!(b["ticket"].as_str().unwrap().starts_with("aud-"));
                assert!(b["relay"].as_u64().is_some() && b["expires_at_ms"].as_i64().is_some());
            } else {
                // Two seats an identity: the third is refused.
                assert_eq!((status, json(&body)["error"].clone()), (409, "too_many_seats".into()));
            }
        }
        let (status, body) = f.audience_join_answer("dev:erin", "Room000001.other", None);
        assert_eq!((status, json(&body)["error"].as_str().unwrap()), (404, "audience_link_unknown"));
        let mut plain = fake(RoomPolicy::default().with_live());
        let link = plain.links().invite.clone();
        let (status, body) = plain.audience_join_answer("dev:dora", &link, None);
        assert_eq!((status, json(&body)["error"].as_str().unwrap()), (404, "no_audience"));
    }
}
