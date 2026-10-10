//! What each element of a skin is, without running the skin's Lua: the
//! `Load` of the fallbacks' `NoteSkin.lua` reimplemented
//! (`NoteSkins/common/common/NoteSkin.lua`, `NoteSkins/dance/default/NoteSkin.lua`,
//! 5_1-new), applied to every skin. A skin's own `NoteSkin.lua` cannot run
//! yet and is reported ([`Skin::ignored_scripts`]).

use crate::builtin::{NativeLua, Virtual};
use crate::skin::{BASE, COMMON, FileRef, PathError, Skin};

/// A dance button (`src/GameManager.cpp:513-518`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Button {
    Left,
    Down,
    Up,
    Right,
    UpLeft,
    UpRight,
}

impl Button {
    pub const ALL: [Button; 6] = [
        Button::Left,
        Button::Down,
        Button::Up,
        Button::Right,
        Button::UpLeft,
        Button::UpRight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Button::Left => "Left",
            Button::Down => "Down",
            Button::Up => "Up",
            Button::Right => "Right",
            Button::UpLeft => "UpLeft",
            Button::UpRight => "UpRight",
        }
    }

    /// The button of a dance lane, by the lane's name (`left`, `upleft`).
    pub fn from_lane(name: &str) -> Option<Button> {
        Button::ALL
            .into_iter()
            .find(|b| b.name().eq_ignore_ascii_case(name))
    }

    /// `dance/default`'s `Rotate` table, degrees.
    fn rotation(self) -> f32 {
        match self {
            Button::Up => 180.0,
            Button::Down => 0.0,
            Button::Left => 90.0,
            Button::Right => -90.0,
            Button::UpLeft => 135.0,
            Button::UpRight => 225.0,
        }
    }
}

/// The elements the engine asks for, per column (`src/NoteDisplay.cpp:432-454`,
/// `src/ReceptorArrow.cpp:35-37`, `src/GhostArrowRow.cpp:29-39`).
pub const ENGINE_ELEMENTS: &[&str] = &[
    "Tap Note",
    "Tap Mine",
    "Tap Lift",
    "Tap Fake",
    "Hold Head Active",
    "Hold Head Inactive",
    "Hold Topcap Active",
    "Hold Topcap Inactive",
    "Hold Body Active",
    "Hold Body Inactive",
    "Hold Bottomcap Active",
    "Hold Bottomcap Inactive",
    "Hold Tail Active",
    "Hold Tail Inactive",
    "Roll Head Active",
    "Roll Head Inactive",
    "Roll Topcap Active",
    "Roll Topcap Inactive",
    "Roll Body Active",
    "Roll Body Inactive",
    "Roll Bottomcap Active",
    "Roll Bottomcap Inactive",
    "Roll Tail Active",
    "Roll Tail Inactive",
    "Receptor",
    "Explosion",
];

/// The parts `common/common`'s `Fallback Explosion.lua` loads for each
/// column.
pub const EXPLOSION_PARTS: &[&str] = &[
    "Hold Explosion",
    "Roll Explosion",
    "Tap Explosion Dim",
    "Tap Explosion Bright",
    "HitMine Explosion",
];

/// The parts `common/common`'s `_Tap Lead-in Receptor.lua` loads.
pub const RECEPTOR_PARTS: &[&str] = &["Ready Receptor", "Go Receptor"];

/// What a file is to `LoadActor` (`02 ActorDef.lua`, `ActorUtil::GetFileType`,
/// `ActorUtil::ResolvePath`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A bitmap: a sprite of the skin's.
    Image(FileRef),
    /// A stand-in's image: drawn by the built-in skin.
    BuiltInArt(FileRef),
    /// A stand-in's Lua, done natively or drawn built in.
    Native(NativeLua, FileRef),
    /// The skin's own Lua (a `.lua` file, or a folder holding
    /// `default.lua`): needs the Lua step.
    Lua(FileRef),
    /// A MilkShape 3D model (`.txt`).
    Model(FileRef),
    /// A movie: a sprite with a movie texture.
    Movie(FileRef),
    /// Anything else: a folder (`BGAnimation`), a sound, `.xml`, an unknown
    /// type.
    Other(FileRef),
}

impl Source {
    pub fn file(&self) -> &FileRef {
        match self {
            Source::Image(f)
            | Source::BuiltInArt(f)
            | Source::Native(_, f)
            | Source::Lua(f)
            | Source::Model(f)
            | Source::Movie(f)
            | Source::Other(f) => f,
        }
    }

    /// Makes a sprite (`SpriteOnly` asks for one): images and movies do,
    /// Lua may.
    fn can_be_sprite(&self) -> bool {
        matches!(
            self,
            Source::Image(_) | Source::BuiltInArt(_) | Source::Movie(_) | Source::Lua(_)
        )
    }
}

/// An element of a column.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolved {
    /// `Blank`: nothing is drawn.
    Blank,
    /// A file, turned by `rotation` degrees (`BaseRotationZ`).
    File { source: Source, rotation: f32 },
    /// A hold cap or body that is not a sprite: StepMania reports "must be
    /// a Sprite" and draws a blank one (`NoteSkinManager.cpp:512-522`).
    NotASprite(Source),
    /// No file: StepMania reports an error and shows nothing; the built-in
    /// skin stands in.
    Missing(PathError),
}

/// Which fallback script is in effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conventions {
    /// `common/common`'s: no redirects, rotation or blanks.
    Common,
    /// `dance/default`'s on top of common's.
    DanceDefault,
}

impl Conventions {
    /// `dance/default`'s `Blank`.
    fn blank(self, element: &str) -> bool {
        self == Conventions::DanceDefault
            && matches!(
                element,
                "Hold Explosion"
                    | "Roll Explosion"
                    | "Hold Topcap Active"
                    | "Hold Topcap Inactive"
                    | "Roll Topcap Active"
                    | "Roll Topcap Inactive"
                    | "Hold Tail Active"
                    | "Hold Tail Inactive"
                    | "Roll Tail Active"
                    | "Roll Tail Inactive"
            )
    }

    /// `dance/default`'s `Redir`: every button to `Down`, heads and fakes to
    /// the tap note (`string.find(sElement, "Head")`).
    fn redir(self, button: Button, element: &str) -> (Button, String) {
        match self {
            Conventions::Common => (button, element.to_string()),
            Conventions::DanceDefault => {
                let element = if element.contains("Head") || element == "Tap Fake" {
                    "Tap Note"
                } else {
                    element
                };
                (Button::Down, element.to_string())
            }
        }
    }

    /// `dance/default`'s `PartsToRotate`, indexed by the element asked for.
    fn rotates(self, element: &str) -> bool {
        self == Conventions::DanceDefault
            && matches!(
                element,
                "Receptor"
                    | "Tap Note"
                    | "Tap Lift"
                    | "Tap Fake"
                    | "Ready Receptor"
                    | "Tap Explosion Bright"
                    | "Tap Explosion Dim"
                    | "Hold Head Active"
                    | "Hold Head Inactive"
                    | "Roll Head Active"
                    | "Roll Head Inactive"
            )
    }
}

/// The parts the engine loads as sprites only (`NoteDisplay.cpp:277-280`).
pub fn sprite_only(element: &str) -> bool {
    ["Topcap", "Body", "Bottomcap"]
        .iter()
        .any(|p| element.contains(&format!(" {p} ")))
}

impl Skin<'_> {
    /// The script that decides the redirects. The `NoteSkin.lua` files run
    /// from the last fallback back to the skin, each wrapping the previous
    /// table, and common's replaces `Redir`, `Rotate`, `PartsToRotate` and
    /// `Blank` outright, so the base nearest the skin decides. The skin's
    /// own scripts would wrap that; they cannot run yet
    /// ([`Skin::ignored_scripts`]).
    pub fn conventions(&self) -> Conventions {
        for &d in &self.chain {
            let name = self.dir_name(d);
            if !self.has_script(d) {
                continue;
            }
            if name.eq_ignore_ascii_case(BASE) {
                return Conventions::DanceDefault;
            }
            if name.eq_ignore_ascii_case(COMMON) {
                return Conventions::Common;
            }
        }
        Conventions::Common
    }

    /// The `NoteSkin.lua` files that would run and cannot yet: those of the
    /// skin and of its fallbacks other than `default` and `common` (whose
    /// scripts are reimplemented).
    pub fn ignored_scripts(&self) -> Vec<String> {
        self.chain
            .iter()
            .filter_map(|&d| self.folder(d))
            .filter(|f| !f.name.eq_ignore_ascii_case(BASE) && !f.name.eq_ignore_ascii_case(COMMON))
            .filter(|f| f.has_file("NoteSkin.lua"))
            .map(|f| format!("{}/NoteSkin.lua", f.name))
            .collect()
    }

    /// What `LoadActor` makes of `file`.
    pub(crate) fn classify(&self, file: FileRef) -> Source {
        match file.virt {
            Some(Virtual::Art) => return Source::BuiltInArt(file),
            Some(Virtual::Lua(n)) => return Source::Native(n, file),
            Some(Virtual::Redir(_)) | Some(Virtual::Other) | None => {}
        }
        if file.is_dir {
            // `ResolvePath`: a folder holding `default.lua` is that file.
            return match self.dir_has(file.dir, &format!("{}/default.lua", file.path)) {
                Some(path) => Source::Lua(FileRef {
                    path,
                    is_dir: false,
                    ..file
                }),
                None => Source::Other(file),
            };
        }
        match file.extension().to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "bmp" => Source::Image(file),
            "lua" => Source::Lua(file),
            "txt" => Source::Model(file),
            "avi" | "f4v" | "flv" | "mkv" | "mp4" | "mpeg" | "mpg" | "mov" | "ogv" | "webm"
            | "wmv" => Source::Movie(file),
            _ => Source::Other(file),
        }
    }

    /// `Load()` for `element` of `button`'s column.
    pub fn resolve(&self, button: Button, element: &str) -> Resolved {
        let conventions = self.conventions();
        if conventions.blank(element) {
            return Resolved::Blank;
        }
        let (to_button, to_element) = conventions.redir(button, element);
        match self.get_path(to_button.name(), &to_element) {
            Ok(file) => {
                let source = self.classify(file);
                if sprite_only(element) && !source.can_be_sprite() {
                    return Resolved::NotASprite(source);
                }
                Resolved::File {
                    source,
                    rotation: if conventions.rotates(element) {
                        button.rotation()
                    } else {
                        0.0
                    },
                }
            }
            Err(e) => Resolved::Missing(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::BuiltIn;
    use crate::skin::Dir;
    use crate::skin::tests::folder;

    #[test]
    fn a_skin_on_dance_default_redirects_to_down_and_rotates() {
        let folders = [folder(
            "dance",
            "s",
            &[
                ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
                ("NoteSkin.lua", ""),
                ("Down Tap Note 4x2.png", ""),
                ("Down Hold Body Active.png", ""),
                ("Down Receptor.lua", ""),
                ("Down Tap Mine.txt", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        assert_eq!(skin.ignored_scripts(), ["s/NoteSkin.lua"]);
        let tap = skin.resolve(Button::Left, "Tap Note");
        let Resolved::File {
            source: Source::Image(f),
            rotation,
        } = tap
        else {
            panic!("{tap:?}")
        };
        assert_eq!((f.path.as_str(), rotation), ("Down Tap Note 4x2.png", 90.0));
        // A hold head is the tap note, rotated as a head.
        let head = skin.resolve(Button::UpRight, "Hold Head Active");
        assert!(
            matches!(head, Resolved::File { source: Source::Image(_), rotation } if rotation == 225.0)
        );
        // Fakes too.
        assert!(
            matches!(skin.resolve(Button::Up, "Tap Fake"), Resolved::File { source: Source::Image(_), rotation } if rotation == 180.0)
        );
        // Bodies do not rotate.
        assert!(
            matches!(skin.resolve(Button::Right, "Hold Body Active"), Resolved::File { source: Source::Image(_), rotation } if rotation == 0.0)
        );
        // The blanks.
        assert_eq!(
            skin.resolve(Button::Down, "Hold Topcap Active"),
            Resolved::Blank
        );
        assert_eq!(
            skin.resolve(Button::Down, "Hold Explosion"),
            Resolved::Blank
        );
        // The skin's Lua and models wait for later steps.
        assert!(matches!(
            skin.resolve(Button::Down, "Receptor"),
            Resolved::File {
                source: Source::Lua(_),
                ..
            }
        ));
        assert!(matches!(
            skin.resolve(Button::Down, "Tap Mine"),
            Resolved::File {
                source: Source::Model(_),
                ..
            }
        ));
        // The explosion row is common's, done natively.
        let Resolved::File {
            source: Source::Native(kind, f),
            rotation,
        } = skin.resolve(Button::Down, "Explosion")
        else {
            panic!()
        };
        assert_eq!(
            (kind, f.path.as_str(), rotation),
            (NativeLua::Explosion, "Fallback Explosion.lua", 0.0)
        );
        assert_eq!(skin.conventions(), Conventions::DanceDefault);
        // Its dim flash goes through dance/default's redirect to `_glow`.
        assert!(matches!(
            skin.resolve(Button::Left, "Tap Explosion Dim"),
            Resolved::File { source: Source::BuiltInArt(f), rotation } if f.path == "_glow (doubleres).png" && rotation == 90.0
        ));
        // Roll bodies come from dance/default.
        assert!(matches!(
            skin.resolve(Button::Left, "Roll Body Inactive"),
            Resolved::File { source: Source::BuiltInArt(f), .. } if f.dir == Dir::BuiltIn(BuiltIn::DanceDefault)
        ));
    }

    #[test]
    fn a_skin_on_common_has_commons_conventions() {
        // `FallbackNoteSkin=common`: the chain is skin, common, default,
        // common, and common's script, nearest the skin, runs last and
        // replaces dance/default's redirects.
        let folders = [folder(
            "dance",
            "retroish",
            &[
                ("metrics.ini", "[Global]\nFallbackNoteSkin=common\n"),
                ("UpLeft Tap Note.png", ""),
                ("Down Tap Note.png", ""),
                ("Down Hold Body Active.txt", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "retroish").unwrap();
        assert_eq!(skin.conventions(), Conventions::Common);
        assert!(matches!(
            skin.resolve(Button::UpLeft, "Tap Note"),
            Resolved::File { source: Source::Image(f), rotation } if f.path == "UpLeft Tap Note.png" && rotation == 0.0
        ));
        // A hold body must be a sprite.
        assert!(matches!(
            skin.resolve(Button::Down, "Hold Body Active"),
            Resolved::NotASprite(Source::Model(_))
        ));
        assert!(sprite_only("Roll Bottomcap Inactive") && !sprite_only("Hold Head Active"));
    }

    #[test]
    fn folders_with_default_lua_are_lua() {
        let folders = [folder(
            "dance",
            "s",
            &[
                ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
                ("Down Tap Explosion Bright/default.lua", ""),
                ("Down Tap Explosion Dim/frame.png", ""),
                ("Down Tap Mine.avi", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        assert!(matches!(
            skin.resolve(Button::Down, "Tap Explosion Bright"),
            Resolved::File { source: Source::Lua(f), .. } if f.path == "Down Tap Explosion Bright/default.lua"
        ));
        assert!(
            matches!(skin.resolve(Button::Down, "Tap Explosion Dim"), Resolved::File { source: Source::Other(f), .. } if f.is_dir)
        );
        assert!(matches!(
            skin.resolve(Button::Down, "Tap Mine"),
            Resolved::File {
                source: Source::Movie(_),
                ..
            }
        ));
    }

    #[test]
    fn a_skin_with_no_fallbacks_has_common_conventions() {
        let folders = [folder(
            "dance",
            "alone",
            &[
                ("metrics.ini", "[Global]\nFallbackNoteSkin=\n"),
                ("Left Tap Note.png", ""),
                ("Down Hold Tail Active.png", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "alone").unwrap();
        assert!(matches!(
            skin.resolve(Button::Left, "Tap Note"),
            Resolved::File { source: Source::Image(f), rotation } if f.path == "Left Tap Note.png" && rotation == 0.0
        ));
        // Nothing is blank.
        assert!(matches!(
            skin.resolve(Button::Down, "Hold Tail Active"),
            Resolved::File { .. }
        ));
        assert!(matches!(
            skin.resolve(Button::Down, "Tap Note"),
            Resolved::Missing(_)
        ));
    }

    #[test]
    fn common_on_its_own_redirects_through_its_files() {
        let skin = Skin::load(&[], "dance", "default").unwrap();
        // Through dance/default's Redir to Down, then common's
        // `Fallback Hold Tail Active.redir` to `_blank`… but dance/default
        // blanks tails first.
        assert_eq!(
            skin.resolve(Button::Down, "Hold Tail Active"),
            Resolved::Blank
        );
        assert!(matches!(
            skin.resolve(Button::Left, "Tap Mine"),
            Resolved::File { source: Source::Native(NativeLua::TapMine, f), .. } if f.path == "Fallback Tap Mine.lua"
        ));
        assert!(matches!(
            skin.resolve(Button::Left, "Ready Receptor"),
            Resolved::File { source: Source::BuiltInArt(f), rotation } if f.path == "Fallback Ready Receptor 3x1.png" && rotation == 90.0
        ));
        // Go Receptor is not rotated (not in `PartsToRotate`).
        assert!(matches!(
            skin.resolve(Button::Left, "Go Receptor"),
            Resolved::File { source: Source::BuiltInArt(f), rotation } if f.path == "Fallback Ready Receptor 3x1.png" && rotation == 0.0
        ));
        assert!(matches!(
            skin.resolve(Button::Down, "Nope"),
            Resolved::Missing(_)
        ));
    }

    #[test]
    fn lanes_name_buttons() {
        assert_eq!(Button::from_lane("upleft"), Some(Button::UpLeft));
        assert_eq!(Button::from_lane("Down"), Some(Button::Down));
        assert_eq!(Button::from_lane("onigiri"), None);
    }
}
