//! TESTING ONLY: writes the scripted chat's emote pictures as files, made
//! by code (round TW4, `docs/plans/multiplayer-twitch.md`, "Animated
//! emotes"): a still emote's `<id>.png`, an animated one's `<id>.gif` and
//! `<id>.webp`, for a walk through each shell's decoder (the web's
//! `mpfakechatimg=<path>`, desktop's script with `images_at`).
//!
//! `cargo run -p ddi-desktop --example chat_images -- <dir> [script]`

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: chat_images <dir> [demo|emotes|flood]");
        std::process::exit(2);
    };
    let name = args.next().unwrap_or_else(|| "demo".into());
    let Some(script) = ddi_chat::script::Script::builtin(&name) else {
        eprintln!("{name}: no such script");
        std::process::exit(2);
    };
    match ddi_desktop::chat::made::write(&script, std::path::Path::new(&dir)) {
        Ok(names) => println!("{} files in {dir}: {}", names.len(), names.join(" ")),
        Err(e) => {
            eprintln!("{dir}: {e}");
            std::process::exit(1);
        }
    }
}
