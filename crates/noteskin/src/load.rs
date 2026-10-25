//! What each element of a skin is, without running the skin's Lua: the
//! `Load` of the fallbacks' `NoteSkin.lua` reimplemented
//! (`NoteSkins/common/common/NoteSkin.lua`, `NoteSkins/dance/default/NoteSkin.lua`,
//! 5_1-new), applied to every skin. A skin's own `NoteSkin.lua` cannot run
//! yet and is reported ([`Skin::ignored_scripts`]).

use crate::builtin::{NativeLua, Virtual};
use crate::skin::{BASE, COMMON, FileRef, PathError, Skin};

/// A dance button (`src/GameManager.cpp:513-518`), or one another
/// layout's lane is drawn with ([`LaneButton`]): pump's `DownLeft`,
/// `DownRight` and `Center` (techno's too), or `Onigiri`, a name of ours
/// for skins that draw one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Button {
    Left,
    Down,
    Up,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
    Center,
    Onigiri,
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
            Button::DownLeft => "DownLeft",
            Button::DownRight => "DownRight",
            Button::Center => "Center",
            Button::Onigiri => "Onigiri",
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
            // Not in the table: not turned.
            Button::DownLeft | Button::DownRight | Button::Center | Button::Onigiri => 0.0,
        }
    }

    /// The dance button an arrow pointing at `degrees` stands for (0 is
    /// left, 90 up, as `ddi_chart::Glyph::Arrow` counts), and how far from
    /// it the arrow turns more, clockwise as skins turn theirs: a dance
    /// button exactly, else Down's elements turned to the angle (a
    /// down-left or down-right arrow).
    pub fn for_arrow(degrees: f32) -> (Button, f32) {
        let a = degrees.rem_euclid(360.0);
        let exact = [
            (0.0, Button::Left),
            (90.0, Button::Up),
            (180.0, Button::Right),
            (270.0, Button::Down),
            (45.0, Button::UpLeft),
            (135.0, Button::UpRight),
        ]
        .into_iter()
        .find(|(d, _)| (a - d).abs() < 0.01);
        match exact {
            Some((_, b)) => (b, 0.0),
            // A down arrow turned clockwise by `a + 90` points at `a`.
            None => (Button::Down, (a + 90.0).rem_euclid(360.0)),
        }
    }
}

/// What draws a layout's lane with a skin (`docs/plans/danoni-skins.md`):
/// a button's elements, the receptor, notes, heads and explosion (those
/// StepMania's `dance/default` turns with their button) turned `turn`
/// degrees more than the skin turns them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaneButton {
    pub button: Button,
    pub turn: f32,
}

impl Skin<'_> {
    /// How `lane` is drawn with this skin: an arrow by its angle
    /// ([`Button::for_arrow`]), a down-left or down-right one by the
    /// skin's own `DownLeft` or `DownRight` elements first (pump's); the
    /// centre panel by the skin's `Center` elements, else Down's as they
    /// are; the onigiri by the skin's own `Onigiri` elements, else
    /// `Center`, else Down's as they are. Each only from the skin's own
    /// folders and the ones it falls back on (never another skin's);
    /// `None` for the lanes no skin draws (giko, iyo, a work's own).
    pub fn lane_button(&self, lane: &ddi_chart::Lane) -> Option<LaneButton> {
        self.lane_buttons(lane).first().copied()
    }

    /// [`Skin::lane_button`]'s choice and the ones after it, in order: the
    /// play takes the next when a lane's tap note or receptor cannot be
    /// built with one (a skin's Lua that has no redirect for `Center`);
    /// the last, Down, always stays.
    pub fn lane_buttons(&self, lane: &ddi_chart::Lane) -> Vec<LaneButton> {
        use ddi_chart::Glyph;
        // A pump skin has every panel's elements (pump/default's Lua makes
        // the right side's from the left's): each lane its own panel.
        if self.game.eq_ignore_ascii_case("pump") {
            let own = match &lane.glyph {
                Glyph::Arrow { rotation_deg } => match Button::for_arrow(*rotation_deg) {
                    (Button::Down, t) if (t - 45.0).abs() < 0.01 => Some(Button::DownLeft),
                    (Button::Down, t) if (t - 315.0).abs() < 0.01 => Some(Button::DownRight),
                    (b @ (Button::UpLeft | Button::UpRight), _) => Some(b),
                    _ => None,
                },
                Glyph::Center => Some(Button::Center),
                _ => None,
            };
            return own
                .map(|button| LaneButton { button, turn: 0.0 })
                .into_iter()
                .collect();
        }
        match &lane.glyph {
            Glyph::Arrow { rotation_deg } => {
                let (button, turn) = Button::for_arrow(*rotation_deg);
                let own = [(45.0, Button::DownLeft), (315.0, Button::DownRight)]
                    .into_iter()
                    .find(|(t, _)| (turn - t).abs() < 0.01)
                    .map(|(_, b)| b);
                own.filter(|b| self.has_button_elements(b.name()))
                    .map(|button| LaneButton { button, turn: 0.0 })
                    .into_iter()
                    .chain([LaneButton { button, turn }])
                    .collect()
            }
            Glyph::Center => self.own_or_down(&[Button::Center]),
            Glyph::Onigiri => self.own_or_down(&[Button::Onigiri, Button::Center]),
            Glyph::Giko | Glyph::Iyo | Glyph::Custom(_) => Vec::new(),
        }
    }

    /// The buttons the skin declares it draws (`[Global] Buttons`).
    pub fn declared_buttons(&self) -> impl Iterator<Item = &str> {
        self.metrics
            .get("Global", "Buttons")
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|b| !b.is_empty())
    }

    /// The `buttons` the chain has elements of, in order, then Down's
    /// elements unturned.
    fn own_or_down(&self, buttons: &[Button]) -> Vec<LaneButton> {
        buttons
            .iter()
            .copied()
            .filter(|b| self.has_button_elements(b.name()))
            .chain([Button::Down])
            .map(|button| LaneButton { button, turn: 0.0 })
            .collect()
    }

    /// Whether a folder of the chain has elements of button `name` at its
    /// top, where `GetPath(name, element)` finds them (`Center Tap
    /// Note.png`, a `Center Receptor/` folder); not art its Lua reaches by
    /// another name (`_center tap note model.txt`). Or whether the skin
    /// declares it draws that button (`[Global] Buttons=Onigiri,Center`, our
    /// extension: a skin whose Lua draws a button through its redirects and
    /// components, with no file of that name).
    fn has_button_elements(&self, name: &str) -> bool {
        if self
            .declared_buttons()
            .any(|b| b.eq_ignore_ascii_case(name))
        {
            return true;
        }
        let prefix = format!("{name} ");
        self.chain.iter().any(|&d| {
            self.folder(d).is_some_and(|f| {
                f.files.iter().any(|path| {
                    let top = path.split('/').next().unwrap_or(path);
                    top.get(..prefix.len())
                        .is_some_and(|head| head.eq_ignore_ascii_case(&prefix))
                })
            })
        })
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

/// Our opt-in third hold state (`HoldUseDroppedState=1`). Not requested from
/// StepMania skins, whose inactive art and life dimming remain unchanged.
pub const DROPPED_ELEMENTS: &[&str] = &[
    "Hold Head Dropped",
    "Hold Tail Dropped",
    "Hold Topcap Dropped",
    "Hold Body Dropped",
    "Hold Bottomcap Dropped",
    "Roll Head Dropped",
    "Roll Tail Dropped",
    "Roll Topcap Dropped",
    "Roll Body Dropped",
    "Roll Bottomcap Dropped",
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
    /// An actor file of StepMania 3.9's format (`.sprite`, `.actor`,
    /// `.model`, `.xml`; `ActorUtil::MakeActor`), built natively
    /// (`crate::legacy`).
    Legacy(FileRef),
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
            | Source::Legacy(f)
            | Source::Movie(f)
            | Source::Other(f) => f,
        }
    }

    /// Makes a sprite (`SpriteOnly` asks for one): images, movies and
    /// `.sprite` files do, Lua may.
    fn can_be_sprite(&self) -> bool {
        match self {
            Source::Image(_) | Source::BuiltInArt(_) | Source::Movie(_) | Source::Lua(_) => true,
            Source::Legacy(f) => f.virt.is_none() && f.extension().eq_ignore_ascii_case("sprite"),
            _ => false,
        }
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
    /// `pump/default`'s on top of common's: its `bBlanks`,
    /// `ElementRedirs` and the explosion, mine and receptor of `UpLeft`
    /// (its mirroring of the right side, a turn about the y axis, is
    /// not drawn natively).
    PumpDefault,
}

impl Conventions {
    /// `dance/default`'s `Blank`.
    fn blank(self, element: &str) -> bool {
        if self == Conventions::PumpDefault {
            return matches!(element, "Hold Tail Active" | "Roll Tail Inactive");
        }
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
            Conventions::PumpDefault => {
                let element = match element {
                    "Hold Head Active" | "Hold Head Inactive" | "Tap Fake" | "Tap Lift" => {
                        "Tap Note"
                    }
                    "Roll Head Inactive" => "Roll Head Active",
                    "Hold Topcap Inactive" | "Roll Topcap Active" | "Roll Topcap Inactive" => {
                        "Hold Topcap Active"
                    }
                    "Hold Body Inactive" | "Roll Body Active" | "Roll Body Inactive" => {
                        "Hold Body Active"
                    }
                    "Hold Bottomcap Inactive"
                    | "Roll Bottomcap Active"
                    | "Roll Bottomcap Inactive" => "Hold Bottomcap Active",
                    "Hold Tail Inactive" | "Roll Tail Active" | "Roll Tail Inactive" => {
                        "Hold Tail Active"
                    }
                    e => e,
                };
                let button = if matches!(element, "Explosion" | "Tap Mine" | "Receptor") {
                    Button::UpLeft
                } else {
                    button
                };
                (button, element.to_string())
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
        // StepMania 3.9 and OpenITG look elements up by button.
        if self.legacy {
            return Conventions::Common;
        }
        for &d in &self.chain {
            let name = self.dir_name(d);
            if !self.has_script(d) {
                continue;
            }
            if name.eq_ignore_ascii_case(BASE) {
                return if self.game.eq_ignore_ascii_case("pump") {
                    Conventions::PumpDefault
                } else {
                    Conventions::DanceDefault
                };
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
            Some(Virtual::Actor { .. }) => return Source::Legacy(file),
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
            _ if crate::is_image(&file.path) => Source::Image(file),
            "lua" => Source::Lua(file),
            "txt" => Source::Model(file),
            "sprite" | "actor" | "model" | "xml" => Source::Legacy(file),
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
    fn lanes_are_drawn_with_the_skins_own_buttons() {
        use ddi_chart::{Glyph, Layout};
        assert_eq!(Button::for_arrow(0.0), (Button::Left, 0.0));
        assert_eq!(Button::for_arrow(-90.0), (Button::Down, 0.0));
        assert_eq!(Button::for_arrow(135.0), (Button::UpRight, 0.0));
        // Down-left and down-right: Down turned.
        assert_eq!(Button::for_arrow(-45.0), (Button::Down, 45.0));
        assert_eq!(Button::for_arrow(225.0), (Button::Down, 315.0));
        let seven = Layout::builtin("danoni-7").unwrap();
        let onigiri = seven
            .lanes
            .iter()
            .find(|l| l.glyph == Glyph::Onigiri)
            .unwrap();
        let lane = |files: &[(&str, &str)]| {
            let folders = [folder("dance", "s", files)];
            let skin = Skin::load(&folders, "dance", "s").unwrap();
            skin.lane_button(onigiri).unwrap().button
        };
        // The skin's own onigiri, else its centre, else its Down.
        assert_eq!(
            lane(&[("Onigiri Tap Note.png", ""), ("Center Tap Note.png", "")]),
            Button::Onigiri
        );
        assert_eq!(lane(&[("Center Tap Note.png", "")]), Button::Center);
        assert_eq!(lane(&[("Center Receptor/default.lua", "")]), Button::Center);
        assert_eq!(lane(&[("Down Tap Note.png", "")]), Button::Down);
        // Not from art reached by another name (Dake's `_center …`: its Lua
        // draws a placeholder for a button it lacks), nor from a subfolder
        // lookups do not search.
        assert_eq!(lane(&[("_center tap note model.txt", "")]), Button::Down);
        assert_eq!(
            lane(&[("alt Center/Center Tap Note.png", "")]),
            Button::Down
        );
        // From the skin it falls back on.
        let folders = [
            folder(
                "dance",
                "s",
                &[("metrics.ini", "[Global]\nFallbackNoteSkin=base\n")],
            ),
            folder("dance", "base", &[("Center Receptor.lua", "")]),
            folder("dance", "other", &[("Onigiri Tap Note.png", "")]),
        ];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        assert_eq!(skin.lane_button(onigiri).unwrap().button, Button::Center);
        // Declared by the skin, with no file of that name (its Lua draws
        // them): the onigiri, else the centre.
        assert_eq!(
            lane(&[("metrics.ini", "[Global]\nButtons=Onigiri, Center\n")]),
            Button::Onigiri
        );
        assert_eq!(
            lane(&[("metrics.ini", "[Global]\nButtons=Center\n")]),
            Button::Center
        );
        let declared = [folder(
            "dance",
            "s",
            &[("metrics.ini", "[Global]\nButtons=Onigiri,Center\n")],
        )];
        let skin = Skin::load(&declared, "dance", "s").unwrap();
        let buttons: Vec<Button> = skin
            .lane_buttons(onigiri)
            .iter()
            .map(|b| b.button)
            .collect();
        assert_eq!(buttons, [Button::Onigiri, Button::Center, Button::Down]);
        let giko = ddi_chart::Lane {
            glyph: Glyph::Giko,
            ..onigiri.clone()
        };
        assert_eq!(skin.lane_button(&giko), None);
    }

    #[test]
    fn pump_lanes_take_the_skins_own_panels_else_down() {
        use ddi_chart::Layout;
        let pump = Layout::pump_single();
        let buttons = |files: &[(&str, &str)]| -> Vec<Vec<(Button, f32)>> {
            let folders = [folder("dance", "s", files)];
            let skin = Skin::load(&folders, "dance", "s").unwrap();
            pump.lanes
                .iter()
                .map(|l| {
                    skin.lane_buttons(l)
                        .into_iter()
                        .map(|b| (b.button, b.turn))
                        .collect()
                })
                .collect()
        };
        // A dance skin: Down turned for the lower diagonals, Down as it is
        // for the centre.
        assert_eq!(
            buttons(&[]),
            [
                vec![(Button::Down, 45.0)],
                vec![(Button::UpLeft, 0.0)],
                vec![(Button::Down, 0.0)],
                vec![(Button::UpRight, 0.0)],
                vec![(Button::Down, 315.0)],
            ]
        );
        // Its own panels first, unturned, the rest after them.
        let own = buttons(&[
            ("DownLeft Tap Note.png", ""),
            ("DownRight Receptor.lua", ""),
            ("Center Tap Note.png", ""),
        ]);
        assert_eq!(own[0], [(Button::DownLeft, 0.0), (Button::Down, 45.0)]);
        assert_eq!(own[2], [(Button::Center, 0.0), (Button::Down, 0.0)]);
        assert_eq!(own[4], [(Button::DownRight, 0.0), (Button::Down, 315.0)]);
        // The centre never takes the onigiri's elements.
        let folders = [folder("dance", "s", &[("Onigiri Tap Note.png", "")])];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        assert_eq!(
            skin.lane_button(&pump.lanes[2]).unwrap().button,
            Button::Down
        );
    }

    #[test]
    fn lanes_name_buttons() {
        assert_eq!(Button::from_lane("upleft"), Some(Button::UpLeft));
        assert_eq!(Button::from_lane("Down"), Some(Button::Down));
        assert_eq!(Button::from_lane("onigiri"), None);
    }
}
