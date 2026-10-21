//! Windows (MSVC): the executable's icon and version, as Explorer, the
//! taskbar's pins and the file's properties show them. They are written
//! here as a `.res` file, the format `rc.exe` writes, with no resource
//! compiler, and handed to the linker, which takes one as it is.
//!
//! The icon is the packages' (`tools/package/icon.svg`) drawn at the
//! sizes Windows asks for, by resvg: a build dependency on Windows hosts
//! only, where the game is built (CI's release), so other builds do not
//! compile it twice. A cross build (checks from a Mac) writes the version
//! alone.

use std::path::{Path, PathBuf};

/// `RT_ICON`, `RT_GROUP_ICON` and `RT_VERSION`.
#[cfg(windows)]
const RT_ICON: u16 = 3;
#[cfg(windows)]
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;

/// English (United States), the resources' language, and Unicode, their
/// text's code page.
const LANGUAGE: u16 = 0x0409;
const CODE_PAGE: u16 = 1200;

/// What the version resource says, beside the version.
const PRODUCT: &str = "Dito Dito Infinito";
const COPYRIGHT: &str =
    "MIT licensed; bundled songs, skins and libraries under their own licenses (Credits).";

/// The icon's sides, in pixels: Windows' small and large icons at 100% to
/// 200% scale, and Explorer's largest view.
#[cfg(windows)]
const ICON_SIDES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !windows {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut res = Res::new();
    icon(&mut res, &manifest.join("../../tools/package/icon.svg"));
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    res.push(RT_VERSION, 1, 0x0030, &version_info(&version));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("ditoditoinfinito.res");
    std::fs::write(&out, &res.0).unwrap();
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// The icon: each size an `RT_ICON`, and the `RT_GROUP_ICON` listing them
/// (`GRPICONDIR`), its id 1, the first, which Explorer shows.
#[cfg(windows)]
fn icon(res: &mut Res, svg: &Path) {
    println!("cargo:rerun-if-changed={}", svg.display());
    let text = std::fs::read_to_string(svg).unwrap();
    let tree = resvg::usvg::Tree::from_str(&text, &resvg::usvg::Options::default()).unwrap();
    let mut group = Vec::new();
    group.extend(0u16.to_le_bytes());
    group.extend(1u16.to_le_bytes());
    group.extend((ICON_SIDES.len() as u16).to_le_bytes());
    for (id, side) in (1u16..).zip(ICON_SIDES) {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(side, side).unwrap();
        let scale = side as f32 / tree.size().width();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        // PNG at 256 pixels, as icon files have it; a bitmap below, which
        // every reader of icons takes.
        let image = if side == 256 {
            pixmap.encode_png().unwrap()
        } else {
            let pixels: Vec<[u8; 4]> = pixmap
                .pixels()
                .iter()
                .map(|p| {
                    let c = p.demultiply();
                    [c.red(), c.green(), c.blue(), c.alpha()]
                })
                .collect();
            bitmap(side, &pixels)
        };
        res.push(RT_ICON, id, 0x1010, &image);
        // `GRPICONDIRENTRY`: 256 is written 0.
        group.extend([side as u8, side as u8, 0, 0]);
        group.extend(1u16.to_le_bytes());
        group.extend(32u16.to_le_bytes());
        group.extend((image.len() as u32).to_le_bytes());
        group.extend(id.to_le_bytes());
    }
    res.push(RT_GROUP_ICON, 1, 0x1030, &group);
}

/// No icon: resvg is not built on this host (above).
#[cfg(not(windows))]
fn icon(_: &mut Res, _: &Path) {}

/// An icon image as a bitmap (`BITMAPINFOHEADER`, then the colors and the
/// mask): 32-bit BGRA rows bottom up, the header's height doubled for the
/// 1-bit mask after them, set where a pixel is transparent.
#[cfg(windows)]
fn bitmap(side: u32, rgba: &[[u8; 4]]) -> Vec<u8> {
    let side_px = side as usize;
    let mask_row = side_px.div_ceil(32) * 4;
    let mut b = Vec::new();
    b.extend(40u32.to_le_bytes());
    b.extend((side as i32).to_le_bytes());
    b.extend((2 * side as i32).to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(32u16.to_le_bytes());
    b.extend(0u32.to_le_bytes()); // BI_RGB
    b.extend(((side_px * side_px * 4 + mask_row * side_px) as u32).to_le_bytes());
    b.extend([0u8; 16]); // resolution, palette
    for row in rgba.chunks(side_px).rev() {
        for &[r, g, bl, a] in row {
            b.extend([bl, g, r, a]);
        }
    }
    for row in rgba.chunks(side_px).rev() {
        let mut bits = vec![0u8; mask_row];
        for (x, px) in row.iter().enumerate() {
            if px[3] < 128 {
                bits[x / 8] |= 0x80 >> (x % 8);
            }
        }
        b.extend(bits);
    }
    b
}

/// The version resource (`VS_VERSIONINFO`): the numbers Windows compares
/// (`VS_FIXEDFILEINFO`) and the text the file's properties show.
fn version_info(version: &str) -> Vec<u8> {
    let number = |i: usize| {
        version
            .split(['.', '-', '+'])
            .nth(i)
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let (major, minor, patch) = (number(0), number(1), number(2));
    let mut fixed = Vec::new();
    for word in [
        0xFEEF_04BD,           // dwSignature
        0x0001_0000,           // dwStrucVersion
        (major << 16) | minor, // dwFileVersionMS
        patch << 16,           // dwFileVersionLS
        (major << 16) | minor, // dwProductVersionMS
        patch << 16,           // dwProductVersionLS
        0x3F,                  // dwFileFlagsMask
        0,                     // dwFileFlags
        0x0004_0004,           // dwFileOS: VOS_NT_WINDOWS32
        1,                     // dwFileType: VFT_APP
        0,                     // dwFileSubtype
        0,                     // dwFileDateMS
        0u32,                  // dwFileDateLS
    ] {
        fixed.extend(word.to_le_bytes());
    }
    let strings = [
        ("FileDescription", PRODUCT),
        ("FileVersion", version),
        ("InternalName", "ditoditoinfinito"),
        ("LegalCopyright", COPYRIGHT),
        ("OriginalFilename", "ditoditoinfinito.exe"),
        ("ProductName", PRODUCT),
        ("ProductVersion", version),
    ]
    .map(|(key, value)| {
        let text = utf16z(value);
        node(key, Value::Text(&text), &[])
    });
    let table = node(
        &format!("{LANGUAGE:04X}{CODE_PAGE:04X}"),
        Value::None,
        &strings,
    );
    let mut translation = Vec::new();
    translation.extend(LANGUAGE.to_le_bytes());
    translation.extend(CODE_PAGE.to_le_bytes());
    let children = [
        node("StringFileInfo", Value::None, &[table]),
        node(
            "VarFileInfo",
            Value::None,
            &[node("Translation", Value::Binary(&translation), &[])],
        ),
    ];
    node("VS_VERSION_INFO", Value::Binary(&fixed), &children)
}

/// A version resource block's value.
enum Value<'a> {
    None,
    Binary(&'a [u8]),
    /// UTF-16 with its terminating zero.
    Text(&'a [u8]),
}

/// A version resource block: its length, its value's length (in bytes,
/// or in characters for text, the zero counted), whether it is text, its
/// key, its value and its children, each of them starting on four bytes.
fn node(key: &str, value: Value, children: &[Vec<u8>]) -> Vec<u8> {
    let (bytes, length, text): (&[u8], usize, bool) = match value {
        Value::None => (&[], 0, true),
        Value::Binary(bytes) => (bytes, bytes.len(), false),
        Value::Text(bytes) => (bytes, bytes.len() / 2, true),
    };
    let mut b = vec![0, 0];
    b.extend((length as u16).to_le_bytes());
    b.extend(u16::from(text).to_le_bytes());
    b.extend(utf16z(key));
    align(&mut b);
    b.extend(bytes);
    for child in children {
        align(&mut b);
        b.extend(child);
    }
    let length = b.len() as u16;
    b[..2].copy_from_slice(&length.to_le_bytes());
    b
}

/// `text` in UTF-16, with a terminating zero.
fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Pads `b` with zeros to a multiple of four bytes.
fn align(b: &mut Vec<u8>) {
    b.resize(b.len().next_multiple_of(4), 0);
}

/// A `.res` file: an empty first resource (which marks it 32-bit), then
/// each resource's header (its size, its type and id as numbers, its
/// memory flags and language) and its data, each starting on four bytes.
struct Res(Vec<u8>);

impl Res {
    fn new() -> Res {
        let mut res = Res(Vec::new());
        res.push_with(0, 0, 0, 0, &[]);
        res
    }

    fn push(&mut self, kind: u16, id: u16, flags: u16, data: &[u8]) {
        self.push_with(kind, id, flags, LANGUAGE, data);
    }

    fn push_with(&mut self, kind: u16, id: u16, flags: u16, language: u16, data: &[u8]) {
        let b = &mut self.0;
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(32u32.to_le_bytes()); // the header's size
        for word in [0xFFFF, kind, 0xFFFF, id] {
            b.extend(word.to_le_bytes());
        }
        b.extend(0u32.to_le_bytes()); // DataVersion
        b.extend(flags.to_le_bytes());
        b.extend(language.to_le_bytes());
        b.extend(0u32.to_le_bytes()); // Version
        b.extend(0u32.to_le_bytes()); // Characteristics
        b.extend(data);
        align(b);
    }
}
