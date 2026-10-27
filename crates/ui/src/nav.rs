//! Menu navigation (`docs/plans/ui-core.md`, P3): semantic actions, held
//! directions repeated by us, focus moved by explicit neighbours or by
//! where things are.
//!
//! StepMania's menus take buttons through `InputMapper` and repeat a held
//! one after 0.375 s, 8 times a second (`TIME_BEFORE_REPEATS`,
//! `REPEATS_PER_SEC`, `src/InputFilter.cpp:117-119`); a screen may change
//! the rate, nothing here does yet. Keys are repeated by us, not by the OS
//! (whose `repeat` events are ignored), so a keyboard and a dance pad,
//! which never repeats on its own, behave the same. Only directions
//! repeat, and only the one pressed last, as an OS repeats only the last
//! key.
//!
//! The keyboard is read by physical code ([`key_action`]), so the arrows
//! and Enter are found on any layout.

use ddi_engine::MenuInput;

/// A menu button (StepMania's `MenuUp` … `MenuStart`, `Back`, `Select`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuAction {
    Up,
    Down,
    Left,
    Right,
    Start,
    ControllerStart,
    More,
    Previous,
    Next,
    Back,
    Select,
}

impl MenuAction {
    pub fn is_direction(self) -> bool {
        matches!(
            self,
            MenuAction::Up | MenuAction::Down | MenuAction::Left | MenuAction::Right
        )
    }
}

impl From<MenuInput> for MenuAction {
    /// The bindings' menu inputs (`ddi_engine::ControlBindings::menu_input`).
    fn from(m: MenuInput) -> MenuAction {
        match m {
            MenuInput::Up => MenuAction::Up,
            MenuInput::Down => MenuAction::Down,
            MenuInput::Left => MenuAction::Left,
            MenuInput::Right => MenuAction::Right,
            MenuInput::Confirm => MenuAction::Start,
            MenuInput::Start => MenuAction::ControllerStart,
            MenuInput::More => MenuAction::More,
            MenuInput::Previous => MenuAction::Previous,
            MenuInput::Next => MenuAction::Next,
            MenuInput::Back => MenuAction::Back,
            MenuInput::Select => MenuAction::Select,
        }
    }
}

/// The keyboard's menu keys, by physical code: the arrows (and the number
/// pad's 8 2 4 6, the arrows of the Single layout's second set, whatever
/// Num Lock says), Enter (and the keypad's) and Space to start,
/// Escape/Backspace to go back, Tab to select (it is not a focus key here:
/// focus moves with the arrows).
pub fn key_action(code: &str) -> Option<MenuAction> {
    Some(match code {
        "ArrowUp" | "Numpad8" => MenuAction::Up,
        "ArrowDown" | "Numpad2" => MenuAction::Down,
        "ArrowLeft" | "Numpad4" => MenuAction::Left,
        "ArrowRight" | "Numpad6" => MenuAction::Right,
        "Enter" | "NumpadEnter" | "Space" => MenuAction::Start,
        "Escape" | "Backspace" => MenuAction::Back,
        "Tab" => MenuAction::Select,
        _ => return None,
    })
}

/// Seconds a direction is held before it repeats.
pub const REPEAT_DELAY: f32 = 0.375;
/// Repeats a second after that.
pub const REPEAT_RATE: f32 = 8.0;

/// What held an action down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A key, by physical code.
    Key(String),
    /// A controller's button, by slot and action: each held one repeats
    /// on its own, released alone.
    Pad(u32, MenuAction),
}

#[derive(Clone, Debug)]
struct Held {
    source: Source,
    action: MenuAction,
    held: f32,
}

/// Held directions and their repeats.
#[derive(Clone, Debug, Default)]
pub struct Repeater {
    held: Vec<Held>,
}

impl Repeater {
    /// `action` went down on `source`; directions start repeating.
    pub fn press(&mut self, source: Source, action: MenuAction) {
        self.held.retain(|h| h.source != source);
        if action.is_direction() {
            self.held.push(Held {
                source,
                action,
                held: 0.0,
            });
        }
    }

    pub fn release(&mut self, source: &Source) {
        self.held.retain(|h| &h.source != source);
    }

    /// Lets go of everything (focus lost, the screen changed).
    pub fn clear(&mut self) {
        self.held.clear();
    }

    pub fn holding(&self) -> bool {
        !self.held.is_empty()
    }

    /// `dt` seconds pass: the repeats of the last direction pressed (every
    /// held one counts its time, so letting go of the last hands over to
    /// the one before at its own pace), as
    /// `InputFilter::Update` makes them (one at [`REPEAT_DELAY`], then
    /// one each `1 / REPEAT_RATE`; a long frame may make several).
    pub fn update(&mut self, dt: f32) -> Vec<MenuAction> {
        let dt = dt.max(0.0);
        let Some(old) = self.held.last().map(|h| h.held) else {
            return Vec::new();
        };
        for h in &mut self.held {
            h.held += dt;
        }
        let h = &self.held[self.held.len() - 1];
        let count = |t: f32| -> i64 {
            if t < REPEAT_DELAY {
                0
            } else {
                1 + ((t - REPEAT_DELAY) * REPEAT_RATE + 1e-4).floor() as i64
            }
        };
        let n = (count(h.held) - count(old)).clamp(0, 16);
        vec![h.action; n as usize]
    }
}

/// A focusable actor of a screen, by name, and where focus goes from it.
/// Neighbours left `None` are found by where actors are ([`spatial`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Focusable {
    pub name: String,
    pub up: Option<String>,
    pub down: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    /// A list: its focus is on one of its own items, whose taps the
    /// screen takes; a tap on the rest of it (its empty space) focuses it
    /// but does not activate it, as Start would its focused item.
    pub list: bool,
}

impl Focusable {
    pub fn new(name: impl Into<String>) -> Focusable {
        Focusable {
            name: name.into(),
            ..Focusable::default()
        }
    }

    /// A list ([`Focusable::list`]) named `name`.
    pub fn new_list(name: impl Into<String>) -> Focusable {
        Focusable {
            list: true,
            ..Focusable::new(name)
        }
    }

    pub fn neighbour(&self, action: MenuAction) -> Option<&str> {
        match action {
            MenuAction::Up => self.up.as_deref(),
            MenuAction::Down => self.down.as_deref(),
            MenuAction::Left => self.left.as_deref(),
            MenuAction::Right => self.right.as_deref(),
            _ => None,
        }
        .filter(|n| !n.is_empty())
    }
}

/// The candidate nearest to `from` in `direction`, by rectangles
/// (`[left, top, right, bottom]`): only those whose centre lies beyond
/// `from`'s in that direction count; the score is the distance along the
/// direction from edge to edge plus twice the gap across it (zero where
/// they overlap), so a row's neighbour straight below wins over a nearer
/// one off to the side, as directional traversal in Flutter and Godot
/// prefers. Ties go to the earlier candidate.
pub fn spatial(from: [f32; 4], direction: MenuAction, candidates: &[[f32; 4]]) -> Option<usize> {
    let centre = |r: &[f32; 4]| [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
    let c = centre(&from);
    let mut best: Option<(f32, usize)> = None;
    for (i, r) in candidates.iter().enumerate() {
        let rc = centre(r);
        // Along, across: the direction's axis, the other one.
        let (along, gap_along, gap_across) = match direction {
            MenuAction::Down => (
                rc[1] - c[1],
                r[1] - from[3],
                gap(from[0], from[2], r[0], r[2]),
            ),
            MenuAction::Up => (
                c[1] - rc[1],
                from[1] - r[3],
                gap(from[0], from[2], r[0], r[2]),
            ),
            MenuAction::Right => (
                rc[0] - c[0],
                r[0] - from[2],
                gap(from[1], from[3], r[1], r[3]),
            ),
            MenuAction::Left => (
                c[0] - rc[0],
                from[0] - r[2],
                gap(from[1], from[3], r[1], r[3]),
            ),
            _ => return None,
        };
        if along <= 0.5 {
            continue;
        }
        let score = gap_along.max(0.0) + 2.0 * gap_across;
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, i));
        }
    }
    best.map(|(_, i)| i)
}

/// The gap between two intervals; 0 where they overlap.
fn gap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    (b0 - a1).max(a0 - b1).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_as_stepmania() {
        let mut r = Repeater::default();
        r.press(Source::Pad(0, MenuAction::Down), MenuAction::Down);
        // Nothing before 0.375 s; one then; then 8 a second.
        assert!(r.update(0.37).is_empty());
        assert_eq!(r.update(0.01), vec![MenuAction::Down]);
        assert!(r.update(0.11).is_empty());
        assert_eq!(r.update(0.02), vec![MenuAction::Down]);
        // A long frame makes several.
        assert_eq!(r.update(0.5).len(), 4);
        // Start does not repeat.
        r.press(Source::Key("Enter".into()), MenuAction::Start);
        r.release(&Source::Pad(0, MenuAction::Down));
        assert!(!r.holding());
        // The last direction pressed repeats; letting it go hands over.
        r.press(Source::Key("ArrowUp".into()), MenuAction::Up);
        r.press(Source::Key("ArrowLeft".into()), MenuAction::Left);
        assert_eq!(r.update(0.4), vec![MenuAction::Left]);
        r.release(&Source::Key("ArrowLeft".into()));
        assert_eq!(r.update(0.12), vec![MenuAction::Up]);
        r.clear();
        assert!(r.update(1.0).is_empty());
    }

    #[test]
    fn spatial_prefers_straight_ahead() {
        // A 3×2 grid of 100×40 cells with gaps of 20.
        let cell = |col: usize, row: usize| {
            let (x, y) = (col as f32 * 120.0, row as f32 * 60.0);
            [x, y, x + 100.0, y + 40.0]
        };
        let cells: Vec<[f32; 4]> = (0..2)
            .flat_map(|row| (0..3).map(move |col| cell(col, row)))
            .collect();
        assert_eq!(spatial(cell(1, 0), MenuAction::Down, &cells), Some(4));
        assert_eq!(spatial(cell(1, 0), MenuAction::Right, &cells), Some(2));
        assert_eq!(spatial(cell(1, 1), MenuAction::Left, &cells), Some(3));
        assert_eq!(spatial(cell(0, 0), MenuAction::Up, &cells), None);
        // A wide button under the grid is reached from any column.
        let mut with_wide = cells.clone();
        with_wide.push([0.0, 150.0, 340.0, 190.0]);
        assert_eq!(spatial(cell(2, 1), MenuAction::Down, &with_wide), Some(6));
        // Straight below but far beats near but off to the side.
        let near_side = [300.0, 100.0, 340.0, 120.0];
        let far_below = [0.0, 300.0, 100.0, 340.0];
        assert_eq!(
            spatial(cell(0, 0), MenuAction::Down, &[near_side, far_below]),
            Some(1)
        );
    }

    #[test]
    fn keys_by_physical_code() {
        assert_eq!(key_action("ArrowDown"), Some(MenuAction::Down));
        assert_eq!(key_action("NumpadEnter"), Some(MenuAction::Start));
        // The number pad's arrows, as the Single layout has them.
        assert_eq!(key_action("Numpad8"), Some(MenuAction::Up));
        assert_eq!(key_action("Numpad2"), Some(MenuAction::Down));
        assert_eq!(key_action("Numpad4"), Some(MenuAction::Left));
        assert_eq!(key_action("Numpad6"), Some(MenuAction::Right));
        assert_eq!(key_action("Numpad5"), None);
        assert_eq!(key_action("KeyQ"), None);
        assert_eq!(MenuAction::from(MenuInput::Confirm), MenuAction::Start);
    }
}
