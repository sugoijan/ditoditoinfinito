//! Print a chart estimate as JSON using the same text decoder as library imports.
use std::{env, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("usage: estimate FILE INDEX")?);
    let index = args
        .next()
        .and_then(|x| x.into_string().ok())
        .ok_or("expected zero-based chart index")?
        .parse()?;
    if args.next().is_some() {
        return Err("usage: estimate FILE INDEX".into());
    }
    let song = ddi_chart::formats::sm::parse_simfile(
        &ddi_library::pack::decode_text(&fs::read(&path)?),
        path.extension()
            .and_then(|e| e.to_str())
            .ok_or("missing file extension")?,
    )?;
    println!(
        "{}",
        serde_json::to_string(&ddi_rating::estimate(&song, index)?)?
    );
    Ok(())
}
