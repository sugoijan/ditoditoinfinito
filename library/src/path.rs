//! Logical paths: where a file is inside an import, a song folder, a pack
//! or a zip, written with `/` on every OS.
//!
//! A pack's song folders, a zip's entry names, the files a manifest entry
//! names and the inputs of ids and cache keys are all such paths, and they
//! must read the same everywhere: the web's ids, its stored records and
//! desktop's library cache are identical on macOS, Linux and Windows. On
//! desktop they meet real files, and a `/`-separated string joined onto a
//! [`PathBuf`] in one go puts `/` inside a Windows path. So these paths have
//! a type of their own, [`RelPath`] (borrowed) and [`RelPathBuf`] (owned),
//! that is not an OS path: it does not convert to [`Path`] implicitly
//! (no `AsRef<Path>`), and [`RelPath::to_path`] is the one way to an OS
//! path, component by component, never above its base.
//!
//! The text is kept as it was given (no normalization on the way in, and
//! JSON reads and writes it as a plain string), and equality, order and
//! hashing are the string's, so sorting or keying by these paths is
//! sorting or keying by their text, as before they had a type.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A `/`-separated path relative to a root (an import, a song folder, a
/// zip), borrowed. See the [module documentation](self).
#[repr(transparent)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelPath(str);

/// An owned [`RelPath`], serialized as its plain string.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelPathBuf(String);

impl RelPath {
    /// `s` as a logical path, unchanged.
    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> &RelPath {
        let s: &str = s.as_ref();
        // SAFETY: `RelPath` is `repr(transparent)` over `str`.
        unsafe { &*(s as *const str as *const RelPath) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn to_rel_path_buf(&self) -> RelPathBuf {
        RelPathBuf(self.0.to_string())
    }

    /// Its non-empty components (`.` and `..` included as written).
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> + '_ {
        self.0.split('/').filter(|c| !c.is_empty())
    }

    /// The folder part (`""` at the root); a trailing `/` is ignored.
    pub fn parent(&self) -> &RelPath {
        RelPath::new(
            self.0
                .trim_end_matches('/')
                .rsplit_once('/')
                .map_or("", |(dir, _)| dir),
        )
    }

    /// The last component; a trailing `/` is ignored.
    pub fn file_name(&self) -> &str {
        let path = self.0.trim_end_matches('/');
        path.rsplit_once('/').map_or(path, |(_, name)| name)
    }

    /// The last component's extension, without the dot. A leading dot
    /// alone (`.ssc`) is not one.
    pub fn extension(&self) -> Option<&str> {
        match self.file_name().rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => Some(ext),
            _ => None,
        }
    }

    /// The last component without its extension.
    pub fn file_stem(&self) -> &str {
        let name = self.file_name();
        match name.rsplit_once('.') {
            Some((stem, _)) if !stem.is_empty() => stem,
            _ => name,
        }
    }

    /// `rest` under this path, joined with one `/` (`rest` alone at the
    /// root). Nothing is resolved: [`RelPath::normalize`] does that.
    pub fn join(&self, rest: impl AsRef<str>) -> RelPathBuf {
        let rest = rest.as_ref();
        if self.0.is_empty() {
            RelPathBuf(rest.to_string())
        } else if rest.is_empty() {
            self.to_rel_path_buf()
        } else {
            RelPathBuf(format!("{}/{rest}", &self.0))
        }
    }

    /// Whether the path is `dir` or inside it (whole components; `""` holds
    /// every path).
    pub fn starts_with(&self, dir: &RelPath) -> bool {
        self.strip_prefix(dir).is_some()
    }

    /// The path below `dir` (`""` for `dir` itself), or `None` outside it.
    pub fn strip_prefix(&self, dir: &RelPath) -> Option<&RelPath> {
        if dir.is_empty() {
            return Some(self);
        }
        match self.0.strip_prefix(&dir.0)? {
            "" => Some(RelPath::new("")),
            rest => rest.strip_prefix('/').map(RelPath::new),
        }
    }

    /// `.`, `..` and empty components resolved: `a/b/../c/./d` is `a/c/d`
    /// (an import's files have no folders to walk through). `None` when it
    /// climbs above the root, which nothing may reach.
    pub fn normalize(&self) -> Option<RelPathBuf> {
        let mut parts: Vec<&str> = Vec::new();
        for c in self.components() {
            match c {
                "." => {}
                ".." => {
                    parts.pop()?;
                }
                c => parts.push(c),
            }
        }
        Some(RelPathBuf(parts.join("/")))
    }

    /// A simfile's reference to a file (`#MUSIC`, `#BANNER`, …) from this
    /// folder: trimmed, backslashes read as `/`, `.` and `..` resolved.
    /// `None` for an empty reference or one that climbs above the root.
    pub fn resolve(&self, reference: &str) -> Option<RelPathBuf> {
        let reference = reference.trim().replace('\\', "/");
        if reference.is_empty() {
            return None;
        }
        let path = self.join(&reference).normalize()?;
        (!path.is_empty()).then_some(path)
    }

    /// This path from folder `dir` (both relative to the same root), with
    /// `../` where it leaves `dir`: the form manifest entries name their
    /// files in.
    pub fn relative_to(&self, dir: &RelPath) -> RelPathBuf {
        let d: Vec<&str> = dir.components().collect();
        let p: Vec<&str> = self.components().collect();
        let common = d.iter().zip(&p).take_while(|(a, b)| a == b).count();
        let mut out: Vec<&str> = vec![".."; d.len() - common];
        out.extend(&p[common..]);
        RelPathBuf(out.join("/"))
    }

    /// The OS path of this file under `base`, the one way a logical path
    /// becomes a file's: [normalized](RelPath::normalize), then each
    /// component pushed on its own, so the OS's separator is used
    /// throughout. `None` when the path climbs above `base`, or a
    /// component is not a plain name on this OS (a drive `C:` or a `\` on
    /// Windows), which would leave `base` too.
    pub fn to_path(&self, base: impl AsRef<Path>) -> Option<PathBuf> {
        let normal = self.normalize()?;
        let mut out = base.as_ref().to_path_buf();
        for c in normal.components() {
            let mut parts = Path::new(c).components();
            match (parts.next(), parts.next()) {
                (Some(std::path::Component::Normal(_)), None) => out.push(c),
                _ => return None,
            }
        }
        Some(out)
    }
}

impl RelPathBuf {
    pub fn new() -> RelPathBuf {
        RelPathBuf(String::new())
    }

    pub fn as_rel_path(&self) -> &RelPath {
        RelPath::new(self.0.as_str())
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl Deref for RelPathBuf {
    type Target = RelPath;

    fn deref(&self) -> &RelPath {
        self.as_rel_path()
    }
}

impl AsRef<RelPath> for RelPath {
    fn as_ref(&self) -> &RelPath {
        self
    }
}

impl AsRef<RelPath> for RelPathBuf {
    fn as_ref(&self) -> &RelPath {
        self
    }
}

impl AsRef<RelPath> for str {
    fn as_ref(&self) -> &RelPath {
        RelPath::new(self)
    }
}

impl AsRef<RelPath> for String {
    fn as_ref(&self) -> &RelPath {
        RelPath::new(self)
    }
}

impl AsRef<str> for RelPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for RelPathBuf {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<RelPath> for RelPathBuf {
    fn borrow(&self) -> &RelPath {
        self
    }
}

impl ToOwned for RelPath {
    type Owned = RelPathBuf;

    fn to_owned(&self) -> RelPathBuf {
        self.to_rel_path_buf()
    }
}

impl From<String> for RelPathBuf {
    fn from(s: String) -> RelPathBuf {
        RelPathBuf(s)
    }
}

impl From<&str> for RelPathBuf {
    fn from(s: &str) -> RelPathBuf {
        RelPathBuf(s.to_string())
    }
}

impl From<&String> for RelPathBuf {
    fn from(s: &String) -> RelPathBuf {
        RelPathBuf(s.clone())
    }
}

impl From<&RelPath> for RelPathBuf {
    fn from(p: &RelPath) -> RelPathBuf {
        p.to_rel_path_buf()
    }
}

impl From<RelPathBuf> for String {
    fn from(p: RelPathBuf) -> String {
        p.0
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for RelPathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Debug for RelPathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

macro_rules! eq_str {
    ($($lhs:ty => $rhs:ty),* $(,)?) => {$(
        impl PartialEq<$rhs> for $lhs {
            fn eq(&self, other: &$rhs) -> bool {
                AsRef::<str>::as_ref(self) == AsRef::<str>::as_ref(other)
            }
        }
        impl PartialEq<$lhs> for $rhs {
            fn eq(&self, other: &$lhs) -> bool {
                AsRef::<str>::as_ref(self) == AsRef::<str>::as_ref(other)
            }
        }
    )*};
}

eq_str!(
    RelPath => str,
    RelPath => String,
    RelPathBuf => str,
    RelPathBuf => &str,
    RelPathBuf => String,
    &RelPath => str,
    &RelPath => String,
);

impl PartialEq<&str> for RelPath {
    fn eq(&self, other: &&str) -> bool {
        &self.0 == *other
    }
}

impl PartialEq<RelPath> for &str {
    fn eq(&self, other: &RelPath) -> bool {
        *self == &other.0
    }
}

impl PartialEq<RelPath> for RelPathBuf {
    fn eq(&self, other: &RelPath) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<RelPathBuf> for RelPath {
    fn eq(&self, other: &RelPathBuf) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<&RelPath> for RelPathBuf {
    fn eq(&self, other: &&RelPath) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<RelPathBuf> for &RelPath {
    fn eq(&self, other: &RelPathBuf) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_of_a_path() {
        let p = RelPath::new("Pack/Song A/song.SSC");
        assert_eq!(p.parent(), "Pack/Song A");
        assert_eq!(p.file_name(), "song.SSC");
        assert_eq!(p.extension(), Some("SSC"));
        assert_eq!(p.file_stem(), "song");
        assert_eq!(RelPath::new("song.sm").parent(), "");
        assert_eq!(RelPath::new("Pack/Song/").file_name(), "Song");
        assert_eq!(RelPath::new(".ssc").extension(), None);
        assert_eq!(RelPath::new("").join("a"), "a");
        assert_eq!(RelPath::new("a").join("b/c"), "a/b/c");
        assert!(RelPath::new("Pack/Song").starts_with(RelPath::new("Pack")));
        assert!(!RelPath::new("Pack v2/Song").starts_with(RelPath::new("Pack")));
        assert_eq!(
            RelPath::new("Pack/Song/a.ogg").strip_prefix(RelPath::new("Pack")),
            Some(RelPath::new("Song/a.ogg"))
        );
    }

    #[test]
    fn normalizing_never_leaves_the_root() {
        assert_eq!(RelPath::new("a/b/../c/./d").normalize().unwrap(), "a/c/d");
        assert_eq!(
            RelPath::new("Pack/Song/../Song/x.ogg").normalize().unwrap(),
            "Pack/Song/x.ogg"
        );
        assert_eq!(RelPath::new("a//b/").normalize().unwrap(), "a/b");
        assert_eq!(RelPath::new("a/../..").normalize(), None);
        assert_eq!(RelPath::new("../x").normalize(), None);
        let dir = RelPath::new("P/S");
        assert_eq!(
            dir.resolve(" audio\\song.ogg ").unwrap(),
            "P/S/audio/song.ogg"
        );
        assert_eq!(dir.resolve("../bn.png").unwrap(), "P/bn.png");
        assert_eq!(dir.resolve("../../../etc/passwd"), None);
        assert_eq!(dir.resolve("  "), None);
        assert_eq!(RelPath::new("P/bn.png").relative_to(dir), "../bn.png");
        assert_eq!(RelPath::new("P/S/a/b.ogg").relative_to(dir), "a/b.ogg");
        assert_eq!(RelPath::new("x.ogg").relative_to(RelPath::new("")), "x.ogg");
    }

    #[test]
    fn order_and_equality_are_the_texts() {
        // As strings ' ' sorts before '/': "Pack A b/…" before "Pack A/…".
        let mut v: Vec<RelPathBuf> = ["Pack A/x", "Pack A b/x", "Pack A-c/x"]
            .map(RelPathBuf::from)
            .to_vec();
        v.sort();
        assert_eq!(v, ["Pack A b/x", "Pack A-c/x", "Pack A/x"]);
        assert_ne!(RelPathBuf::from("a//b"), RelPathBuf::from("a/b"));
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"["Pack A b/x","Pack A-c/x","Pack A/x"]"#);
        assert_eq!(serde_json::from_str::<Vec<RelPathBuf>>(&json).unwrap(), v);
    }

    /// The bug this type is for: a path built from a logical one holds the
    /// OS's separator only, on every OS.
    #[test]
    fn os_paths_use_only_the_platforms_separator() {
        let base = std::env::temp_dir().join("ddi-relpath");
        for logical in [
            "Pack/Song A/song.ogg",
            "Songs/Pack/Song/../Song/bn.png",
            "./a//b/c.png",
            "日本語パック/曲/音.ogg",
        ] {
            let path = RelPath::new(logical).to_path(&base).unwrap();
            let below = path.strip_prefix(&base).unwrap();
            let text = below.to_str().unwrap();
            let other = if std::path::MAIN_SEPARATOR == '/' {
                '\\'
            } else {
                '/'
            };
            assert!(!text.contains(other), "{logical}: {text}");
            let normal = RelPath::new(logical).normalize().unwrap();
            let want: Vec<&str> = normal.components().collect();
            assert_eq!(text, want.join(std::path::MAIN_SEPARATOR_STR), "{logical}");
        }
        assert_eq!(RelPath::new("").to_path(&base).unwrap(), base);
        // Never above the base.
        assert_eq!(RelPath::new("a/../../x").to_path(&base), None);
        if cfg!(windows) {
            assert_eq!(RelPath::new("C:/x").to_path(&base), None);
            assert_eq!(RelPath::new("a\\..\\..\\x").to_path(&base), None);
        }
    }
}
