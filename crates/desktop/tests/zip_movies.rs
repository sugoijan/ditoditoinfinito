//! Movies in zips (`play::load::Files::movie`): a compressed one is
//! extracted once into the cache's `movies/`, two loads at once each
//! getting it whole, a load let go giving its extraction up, a stored one
//! read in place. Needs the `zip` command (skipped without it).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use ddi_desktop::library::files::ZipReader;
use ddi_desktop::play::load::Files;

#[test]
fn compressed_movies_extract_once_and_safely() {
    let dir = std::env::temp_dir().join(format!("ddi-zip-movies-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Pack/Song")).unwrap();
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    // 40 MB that compress well, and a small one stored.
    let big: Vec<u8> = (0..40_000_000u32).map(|i| (i / 4096 % 7) as u8).collect();
    std::fs::write(dir.join("Pack/Song/clip.avi"), &big).unwrap();
    std::fs::write(dir.join("Pack/Song/small.mp4"), &big[..10_000]).unwrap();
    let zip = dir.join("pack.zip");
    let made = std::process::Command::new("zip")
        .current_dir(&dir)
        .args(["-q", "-X", "-9", "pack.zip", "Pack/Song/clip.avi"])
        .status()
        .is_ok_and(|s| s.success())
        && std::process::Command::new("zip")
            .current_dir(&dir)
            .args(["-q", "-X", "-0", "pack.zip", "Pack/Song/small.mp4"])
            .status()
            .is_ok_and(|s| s.success());
    if !made {
        eprintln!("no zip command: skipped");
        return;
    }
    let files = || Files::Zip {
        zip: zip.clone(),
        reader: Arc::new(ZipReader::open(&zip, &ddi_desktop::library::shift_jis).unwrap()),
        dir: "Pack/Song".into(),
    };
    // Two at once: both get the whole movie, at the same place.
    let (a, b) = (files(), files());
    let t1 = std::thread::spawn(move || a.movie("clip.avi"));
    let t2 = std::thread::spawn(move || b.movie("clip.avi"));
    let (p1, p2) = (t1.join().unwrap().unwrap(), t2.join().unwrap().unwrap());
    assert_eq!(p1, p2);
    assert_eq!(p1.1, None);
    assert_eq!(std::fs::read(&p1.0).unwrap(), big);
    let movies = p1.0.parent().unwrap().to_path_buf();
    let parts = |d: &std::path::Path| {
        std::fs::read_dir(d)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "part"))
            .count()
    };
    assert_eq!(parts(&movies), 0);
    // Again: the cache's.
    assert_eq!(files().movie("clip.avi").unwrap(), p1);
    // Given up: an error, nothing left behind.
    std::fs::remove_file(&p1.0).unwrap();
    let cancel = AtomicBool::new(true);
    assert!(files().movie_unless("clip.avi", &cancel).is_err());
    assert_eq!(parts(&movies), 0);
    assert!(!p1.0.exists());
    // Stored: read in place.
    let (path, range) = files().movie("small.mp4").unwrap();
    assert_eq!(path, zip);
    assert_eq!(range.map(|r| r.1), Some(10_000));
    let _ = std::fs::remove_dir_all(&dir);
}
