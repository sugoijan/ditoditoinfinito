//! Layout frames (`docs/plans/ui-core.md`, P8), behind the `layout`
//! feature: a tree of boxes with flexbox or grid styles (taffy's), laid
//! out in a rectangle; what comes back is where each named box is, for a
//! screen to place and build its actors there (a results screen's panels
//! in a grid that changes with the ruleset and the player's choice).
//! Everything else stays absolute.

use taffy::prelude::*;

pub use taffy;

/// A box: its style, a name to find it by, what it holds.
#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: Option<String>,
    pub style: Style,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(style: Style) -> Node {
        Node {
            name: None,
            style,
            children: Vec::new(),
        }
    }

    pub fn named(name: &str, style: Style) -> Node {
        Node {
            name: Some(name.into()),
            style,
            children: Vec::new(),
        }
    }

    pub fn with(mut self, children: Vec<Node>) -> Node {
        self.children = children;
        self
    }
}

/// The named boxes of `root` laid out in `rect` (`[x, y, w, h]`), as
/// `(name, [x, y, w, h])` in the same space, in tree order.
pub fn compute(root: &Node, rect: [f32; 4]) -> Vec<(String, [f32; 4])> {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    fn add(tree: &mut TaffyTree<()>, n: &Node) -> Option<NodeId> {
        let kids: Option<Vec<NodeId>> = n.children.iter().map(|c| add(tree, c)).collect();
        tree.new_with_children(n.style.clone(), &kids?).ok()
    }
    let Some(top) = add(&mut tree, root) else {
        return Vec::new();
    };
    let mut style = root.style.clone();
    style.size = Size {
        width: length(rect[2]),
        height: length(rect[3]),
    };
    let _ = tree.set_style(top, style);
    if tree
        .compute_layout(
            top,
            Size {
                width: AvailableSpace::Definite(rect[2]),
                height: AvailableSpace::Definite(rect[3]),
            },
        )
        .is_err()
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    fn walk(
        tree: &TaffyTree<()>,
        n: &Node,
        id: NodeId,
        origin: [f32; 2],
        out: &mut Vec<(String, [f32; 4])>,
    ) {
        let Ok(l) = tree.layout(id) else { return };
        let at = [origin[0] + l.location.x, origin[1] + l.location.y];
        if let Some(name) = &n.name {
            out.push((name.clone(), [at[0], at[1], l.size.width, l.size.height]));
        }
        let ids = tree.children(id).unwrap_or_default();
        for (c, cid) in n.children.iter().zip(ids) {
            walk(tree, c, cid, at, out);
        }
    }
    walk(&tree, root, top, [rect[0], rect[1]], &mut out);
    out
}

/// A grid of `columns` equal columns and as many equal rows as needed for
/// `names`, `gap` between cells: the shape a results panel grid starts
/// from.
pub fn grid(names: &[&str], columns: u16, gap: f32) -> Node {
    let rows = (names.len() as u16).div_ceil(columns.max(1)).max(1);
    Node::new(Style {
        display: Display::Grid,
        grid_template_columns: evenly_sized_tracks(columns.max(1)),
        grid_template_rows: evenly_sized_tracks(rows),
        gap: Size {
            width: length(gap),
            height: length(gap),
        },
        ..Style::default()
    })
    .with(
        names
            .iter()
            .map(|n| Node::named(n, Style::default()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_split_the_rectangle() {
        let r = compute(
            &grid(&["A", "B", "C"], 2, 10.0),
            [100.0, 50.0, 210.0, 110.0],
        );
        let get = |n: &str| r.iter().find(|(k, _)| k == n).unwrap().1;
        assert_eq!(get("A"), [100.0, 50.0, 100.0, 50.0]);
        assert_eq!(get("B"), [210.0, 50.0, 100.0, 50.0]);
        assert_eq!(get("C"), [100.0, 110.0, 100.0, 50.0]);
    }

    #[test]
    fn flex_rows_share_by_grow() {
        let root = Node::new(Style {
            flex_direction: FlexDirection::Row,
            ..Style::default()
        })
        .with(vec![
            Node::named(
                "Narrow",
                Style {
                    flex_grow: 1.0,
                    ..Style::default()
                },
            ),
            Node::named(
                "Wide",
                Style {
                    flex_grow: 3.0,
                    ..Style::default()
                },
            ),
        ]);
        let r = compute(&root, [0.0, 0.0, 400.0, 20.0]);
        assert_eq!(r[0].1, [0.0, 0.0, 100.0, 20.0]);
        assert_eq!(r[1].1, [100.0, 0.0, 300.0, 20.0]);
    }
}
