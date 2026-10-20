//! Messages (`docs/plans/ui-core.md`, P7): a named event with parameters,
//! broadcast to every actor of the screens shown, as StepMania's
//! `MessageManager::Broadcast` reaches every subscribed actor, which
//! plays its `<Name>MessageCommand` (`src/MessageManager.cpp`,
//! `Actor::HandleMessage` in `src/Actor.cpp`). Actors react through the
//! command mechanism they already have: a command named `<Name>Message`
//! ([`ddi_actor::Actor::set_command`]), played on the screen's whole
//! tree as a skin's `HandleMessage` plays one (`ddi_noteskin`'s Lua host,
//! `LuaSkin::play`). Native commands take no parameters; the screen's own
//! code gets them ([`crate::Screen::message`]), and a Lua host (themes,
//! later) will hand them over as the message's table.

use ddi_actor::{Actor, Problems};

/// A message parameter (`Message::SetParam`).
#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    Str(String),
    Int(i64),
    Num(f64),
    Bool(bool),
}

/// A named message and its parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub name: String,
    pub params: Vec<(String, Param)>,
}

impl Message {
    pub fn new(name: impl Into<String>) -> Message {
        Message {
            name: name.into(),
            params: Vec::new(),
        }
    }

    /// With parameter `key` set to `value`.
    pub fn with(mut self, key: impl Into<String>, value: Param) -> Message {
        let key = key.into();
        self.params.retain(|(k, _)| *k != key);
        self.params.push((key, value));
        self
    }

    pub fn get(&self, key: &str) -> Option<&Param> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The command actors react with: `<Name>Message`.
    pub fn command(&self) -> String {
        format!("{}Message", self.name)
    }

    /// Plays the message's command on `root` and every actor inside it.
    pub fn deliver(&self, root: &mut Actor, problems: &mut Problems) {
        root.play(&self.command(), problems);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_actor::Kind;
    use ddi_actor::commands::parse;

    #[test]
    fn actors_play_their_message_command() {
        let mut child = Actor::new(Kind::Actor);
        child.set_command("SongChangedMessage", parse("x,40").unwrap());
        let mut other = Actor::new(Kind::Actor);
        other.set_command("OtherMessage", parse("x,99").unwrap());
        let mut root = Actor::new(Kind::Frame(vec![child, other]));
        let m = Message::new("SongChanged").with("Song", Param::Str("u-1".into()));
        assert_eq!(m.command(), "SongChangedMessage");
        assert_eq!(m.get("Song"), Some(&Param::Str("u-1".into())));
        m.deliver(&mut root, &mut Problems::default());
        assert_eq!(root.children()[0].current().pos[0], 40.0);
        assert_eq!(root.children()[1].current().pos[0], 0.0);
    }
}
