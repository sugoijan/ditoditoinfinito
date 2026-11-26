//! A well-behaved source over generated songs, written on the SDK
//! (`ddi_source_test_plugins::example`): the host's tests' model plugin,
//! and an example of every capability.
//!
//! - 24 songs, `song-1` to `song-24`, in two packs; searched by title
//!   with a level range, a style choice and an artist filter, a page at a
//!   time;
//! - browsed by pack; opened from a link (`https://songs.invalid/<pack>`);
//! - fetched as streamed files, `pack-a.zip` as a zip, `big-song` into the
//!   cache folder the game gives (its size is past the memory budget), and
//!   `local` where it is, a folder in the plugin's data folder;
//! - `slow-song` takes a while, telling progress and checking for a
//!   cancel;
//! - `song-24` is not for online rooms (its own room rule).

fn main() {
    ddi_source_sdk::main(ddi_source_test_plugins::example::Example);
}
