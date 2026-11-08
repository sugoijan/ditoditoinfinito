# System errors as values

Status: planned (2026-11-08). Not started.

## Why

A failure from outside the game (a file the system would not open, a
browser storage refusal, a server's answer) reaches the screens as the
text the system wrote. `ddi_ui::screens::errors::kind_of` then guesses
its kind back from that text: a DOMException's name, an English phrase,
`"os error N)"`, `"HTTP 404"`. The kind was known where the error was
made and thrown away there.

The guess is wrong across systems. Errno numbers mean different things
on each: `os error 5)` is access denied on Windows but EIO on Linux, and
`os error 1)` and `os error 3)` differ the same way. The phrases are
English only, so an OS that answers in another language is never
classified. Every new phrase is one more string to match.

Desktop's URL importer already does it right: `danoni::report::SystemError`
keeps an `io::ErrorKind` and the words, and words them through
`errors::io_error_text`. This plan makes that the rule.

## What

1. **A shared type.** Add `SystemError { kind: Option<FailureKind>, words: String }`
   in `ddi_platform` (plain data, serde, no platform code).
   - `FailureKind` moves there from `ddi_ui::screens::errors`; the UI
     re-exports it.
   - `From<io::Error>` maps `io::ErrorKind` as `errors::io_error_text`
     already does (NotFound, PermissionDenied, StorageFull, TimedOut, the
     network kinds, InvalidData, Unsupported), which then words a
     `SystemError` instead.
   - The web shell gets `SystemError::from_js(&JsValue)`, which reads a
     DOMException's `name` (QuotaExceededError, NotAllowedError,
     SecurityError, NotFoundError, NetworkError, TimeoutError,
     NotSupportedError, NotReadableError, DataError, EncodingError) and
     its message as the words.
   - An HTTP failure gives its kind from the status: 401 and 403 Denied,
     404 and 410 NotFound, 408 and 504 Timeout. Desktop's
     ureq errors give theirs from the transport error's kind.
2. **Carry it instead of text.** Replace the `String` with `SystemError`:
   - `LoadFailure::{File, Music, Audio, Other}` and `VideoError::{NotLoaded, File}`;
   - the import's `Finding::System`, `Finding::FileFailed` and the other
     findings that carry a system's `error`, and `ImportFailure::System`;
   - `ShareError::Failed`;
   - the studio's `Note::Unreadable`;
   - the shells' `Result<_, String>` where the error is a system's.
     These are mostly `crates/web/src/components/studio_page.rs`,
     `options_page.rs` and `select_page.rs`, desktop `options.rs`,
     `songs.rs` and `danoni/report.rs`, about 50 calls of `error_text` in all.
   - Desktop's `danoni::report::SystemError` becomes the shared one.
3. **Word it from the kind.** `errors::system_text(&SystemError)` says
   the kind in the player's language with the words after it as data,
   or the words alone when the kind is `None`. No new messages: the
   kinds' messages exist (`errors.ftl`).
4. **Keep the text guess only for old records.** Import reports are
   stored (`Finding` is serialized with the web's songs), so text written
   before this change is read back with no kind.
   - The deserializer accepts a plain string and makes it a `SystemError`
     with the kind `kind_of` guesses.
   - `kind_of` then serves only that path and errors that really arrive
     as text: a decoder's message, the importer's protocol. Its errno
     matches go, since a live `io::Error` always has its kind.

## Checks

- Unit tests:
  - `From<io::Error>` for each mapped kind;
  - the web's DOMException names (a `wasm-bindgen-test`, or the mapping
    split into a pure function of the name and tested natively);
  - HTTP statuses.
- A stored import report written as text before this change reads
  back and still shows its kind.
- `l10n_tests.rs`'s walk of the screens keeps passing: the failures' text
  is unchanged for every kind it covers.
- On Windows, from the Mac (`cargo clippy -p ddi-desktop --target
  x86_64-pc-windows-msvc`): no `os error` text match is left for a live error.

## Left out

- Errors that are ours (an import's findings, why a chart did not read)
  are already values worded by the screens; they stay as they are.
- Terminal output and logs keep the system's words in English.
