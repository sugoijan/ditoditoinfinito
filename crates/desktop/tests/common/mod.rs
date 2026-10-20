//! What the desktop's tests share: the bundled songs' and noteskins'
//! indexes (`target/songs/index.json`, `target/noteskins/index.json`),
//! which Trunk's builds and `just test` write, written here when a fresh
//! checkout has none.

use std::path::Path;
use std::sync::Once;

/// Writes the indexes the bundled songs and noteskins are listed from,
/// when they are missing (`cargo run -p xtask -- gen-songs`,
/// `gen-noteskins`), once a test binary.
#[allow(dead_code)]
pub fn indexes() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (index, task) in [
            ("target/songs/index.json", "gen-songs"),
            ("target/noteskins/index.json", "gen-noteskins"),
        ] {
            if root.join(index).is_file() {
                continue;
            }
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let ok = std::process::Command::new(cargo)
                .current_dir(&root)
                .args(["run", "-q", "-p", "xtask", "--", task])
                .status()
                .is_ok_and(|s| s.success());
            assert!(ok, "`cargo run -p xtask -- {task}` failed");
        }
    });
}
