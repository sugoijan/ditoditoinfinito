//! A noteskin in a song folder the settings list (a StepMania root's
//! `NoteSkins/`), read on its thread as play and Options' preview read
//! it: found (the settings are the calling thread's, taken there).

use std::path::Path;
use std::rc::Rc;

use ddi_desktop::library;
use ddi_desktop::play::skins;

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

#[test]
fn a_song_folders_skin_is_read_on_its_thread() {
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let root = std::env::temp_dir().join(format!("ddi-skins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Songs")).unwrap();
    let delta = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/noteskins/dance/Delta");
    copy(&delta, &root.join("NoteSkins/dance/Mine"));
    // In the settings' folders, not this run's own.
    library::set_folders(std::slice::from_ref(&root));
    let layout = ddi_chart::Layout::dance_single();
    let reading = skins::Reading::start("dance/mine", &layout, Default::default(), None);
    let read = loop {
        if let Some(r) = reading.poll() {
            break r;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let made = skins::build(
        read.expect("the folder's skin, read on the thread"),
        &layout,
    )
    .expect("made");
    assert_eq!(made.play.name, "Mine");
    let _ = std::fs::remove_dir_all(&root);
}
