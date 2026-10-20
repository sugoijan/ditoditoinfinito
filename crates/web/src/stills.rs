//! The song's background from its opening movie
//! (`ddi_library::backgrounds::opening_movie`; the maintainer's decision of
//! 2026-10-16, `docs/plans/video-backgrounds.md`): when a song's first
//! background change is a movie at or before its first note, the frame that
//! movie shows when its change starts is the song's picture in song select,
//! behind the start prompt and in the field before the change, so the cut to
//! the movie is seamless.
//!
//! The frame is taken once, at import ([`fill`] for each song the import
//! stored), decoded as play decodes it (the browser's decoder or the
//! module, `crate::movies::frame_at`), and stored with the song as a JPEG
//! ([`STILL_FILE`], at the movie's size up to [`MAX_SIDE`]), so song select
//! and the prompt never load the video decoder for it. Songs imported before
//! get theirs when first played (the record says nothing yet:
//! `ImportedSong::still` is `None`). A movie that cannot be decoded leaves
//! the song its `#BACKGROUND`, with a line in the import's details.

use ddi_library::backgrounds::{self, BgImage};
use ddi_platform::video::{VideoCodec, VideoContainer, VideoFormat, VideoFrame};
use wasm_bindgen::{Clamped, JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, ImageBitmap, ImageBitmapOptions, ResizeQuality};

use crate::movies::{GrabError, frame_at};
use crate::songs::{ImportedSong, ManifestEntry, Still, file_key, imported_record};
use crate::web::idb::{self, Db, Write};
use crate::web::js_err;
use crate::web::video::VideoWorker;
use crate::web::webcodecs::DirectFrame;

/// The still's file among an imported song's files (`<id>/still.jpg`).
pub(crate) const STILL_FILE: &str = "still.jpg";

/// The still is kept at the movie's size, scaled down to this longest side
/// at most (what background-change images are kept at): the field shows it
/// full size before the movie, song select tiny and blurred.
pub(crate) const MAX_SIDE: u32 = 1280;

/// JPEG quality: every browser's canvas encodes JPEG (Safari's no WebP),
/// about 30–120 KB for a 640×480 to 1280×720 frame.
const QUALITY: f64 = 0.9;

/// Takes stills one song after another, keeping the module's worker for
/// the next (an import takes several).
#[derive(Default)]
pub(crate) struct Taker {
    worker: Option<VideoWorker>,
}

/// What [`fill`] did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Filled {
    /// The record already said.
    Already,
    /// Taken; whether it shows nothing (the menus then keep the song's
    /// `#BACKGROUND`).
    Taken { blank: bool },
    /// No opening movie: the song keeps its `#BACKGROUND`.
    NoMovie,
    /// The opening movie could not be decoded: the song keeps its
    /// `#BACKGROUND` (kept in the record; not tried again).
    Failed { movie: String, reason: String },
    /// Not now (the decoder or the song's files could not be reached);
    /// the record still says nothing, so it is tried again.
    Later(String),
}

/// Whether the song may have an opening movie (one of its movies plays, or
/// a change names one a shared folder may hold): only those are looked at.
pub(crate) fn may_have(entry: &ManifestEntry) -> bool {
    entry.bg_videos.iter().any(|v| v.plays())
        || entry
            .bg_shared
            .iter()
            .any(|n| ddi_library::video::is_movie(n.as_str()))
}

/// Takes imported song `id`'s still, unless its record already says what
/// became of it, and keeps the outcome in the record.
pub(crate) async fn fill(id: &str, taker: &mut Taker) -> Filled {
    match take(id, taker).await {
        Ok(filled) => filled,
        Err(e) => Filled::Later(e),
    }
}

async fn take(id: &str, taker: &mut Taker) -> Result<Filled, String> {
    let db = Db::open().await?;
    let record = imported_record(&db, id).await?;
    // `no_movie` is looked at again: a shared folder imported since may
    // hold the opening movie.
    if matches!(
        record.still,
        Some(Still::Taken { .. } | Still::Failed { .. })
    ) {
        return Ok(Filled::Already);
    }
    let Some(opening) = opening(&record).await? else {
        save(&db, id, Still::NoMovie, None).await?;
        return Ok(Filled::NoMovie);
    };
    let grabbed = frame_at(
        &opening.blob,
        &opening.movie,
        &opening.format,
        opening.position,
        opening.looping,
        &mut taker.worker,
    )
    .await;
    let failed = |reason: String| Still::Failed {
        movie: opening.movie.clone(),
        reason,
    };
    let (still, jpeg) = match grabbed {
        Err(GrabError::Unavailable(e)) => return Ok(Filled::Later(e)),
        Err(GrabError::Failed(e)) => (failed(e), None),
        Ok(frame) => match encode(&frame).await {
            Ok((jpeg, width, height, blank)) => (
                Still::Taken {
                    movie: opening.movie.clone(),
                    position: opening.position,
                    width,
                    height,
                    blank: Some(blank),
                },
                Some(jpeg),
            ),
            Err(e) => (failed(e), None),
        },
    };
    save(&db, id, still.clone(), jpeg).await?;
    Ok(match still {
        Still::Taken { blank, .. } => Filled::Taken {
            blank: blank.unwrap_or(false),
        },
        Still::Failed { movie, reason } => Filled::Failed { movie, reason },
        Still::NoMovie => Filled::NoMovie,
    })
}

/// The song's opening movie, ready to decode.
struct Opening {
    movie: String,
    position: f64,
    looping: bool,
    format: VideoFormat,
    blob: Blob,
}

/// The song's opening movie, as its play's schedule has it (its images and
/// playable movies, its own and those in shared folders), with its file.
async fn opening(record: &ImportedSong) -> Result<Option<Opening>, String> {
    let entry = &record.entry;
    if !may_have(entry) {
        return Ok(None);
    }
    let song = crate::songs::load_chart(&entry.id)
        .await
        .map_err(|e| e.message())?;
    let (images, videos) = crate::songs::shared_found(entry, &record.pack).await?;
    let image_names: Vec<String> = entry
        .bg_images
        .iter()
        .map(|p| p.to_string())
        .chain(images.into_iter().map(|(name, _)| name))
        .collect();
    let movie_names: Vec<String> = entry
        .bg_videos
        .iter()
        .filter(|v| v.plays())
        .map(|v| v.name.to_string())
        .chain(videos.iter().map(|(name, _)| name.clone()))
        .collect();
    let schedule = backgrounds::schedule(
        &song,
        &image_names,
        &movie_names,
        entry.background.is_some(),
    );
    let Some(o) = backgrounds::opening_movie(&song, &schedule) else {
        return Ok(None);
    };
    // Shared movies were not sniffed: the decoder finds out.
    let format = entry.bg_videos.iter().find(|v| v.name == o.movie).map_or(
        VideoFormat {
            container: VideoContainer::Unknown,
            codec: VideoCodec::Unknown,
        },
        |v| v.format.clone(),
    );
    let blob =
        crate::songs::load_background(entry, &BgImage::Movie(o.movie.clone()), &videos, None)
            .await?
            .ok_or_else(|| format!("{} is not stored", o.movie))?;
    Ok(Some(Opening {
        looping: backgrounds::movie_loops(&schedule, o.segment),
        movie: o.movie,
        position: o.position,
        format,
        blob,
    }))
}

/// Imported song `id`'s stored still.
pub(crate) async fn load(id: &str) -> Result<Blob, String> {
    let db = Db::open().await?;
    db.get(idb::FILES, &file_key(id, STILL_FILE))
        .await?
        .and_then(|v| v.dyn_into::<Blob>().ok())
        .ok_or_else(|| "the song's picture is not stored".to_string())
}

/// Keeps `still` (and its picture) in song `id`'s record, read again so
/// nothing written meanwhile is lost; a song removed meanwhile stays gone.
async fn save(db: &Db, id: &str, still: Still, jpeg: Option<Blob>) -> Result<(), String> {
    let Ok(mut record) = imported_record(db, id).await else {
        return Ok(());
    };
    record.still = Some(still);
    let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    let mut writes = Vec::new();
    if let Some(jpeg) = jpeg {
        writes.push(Write::Put {
            store: idb::FILES,
            key: file_key(id, STILL_FILE),
            value: jpeg.into(),
        });
    }
    writes.push(Write::Put {
        store: idb::SONGS,
        key: id.to_string(),
        value: JsValue::from_str(&json),
    });
    db.write(writes).await
}

/// The frame as a JPEG at its size, scaled down to [`MAX_SIDE`] at most:
/// decoded planes converted as the background shader converts them
/// ([`VideoFrame::rgba`]), or the browser's own frame as it draws it (the
/// direct path). Returns the file, its size and whether it is blank
/// (`ddi_platform::video::blank_picture`).
async fn encode(frame: &VideoFrame) -> Result<(Blob, u32, u32, bool), String> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let (w, h) = (frame.width, frame.height);
    if w == 0 || h == 0 {
        return Err("an empty frame".into());
    }
    let scale = (f64::from(MAX_SIDE) / f64::from(w.max(h))).min(1.0);
    let (tw, th) = (
        ((f64::from(w) * scale).round() as u32).max(1),
        ((f64::from(h) * scale).round() as u32).max(1),
    );
    let options = ImageBitmapOptions::new();
    options.set_resize_width(tw);
    options.set_resize_height(th);
    options.set_resize_quality(ResizeQuality::High);
    let promise = match &frame.external {
        Some(external) => {
            let direct = external
                .0
                .downcast_ref::<DirectFrame>()
                .ok_or("a frame of another kind")?;
            window.create_image_bitmap_with_image_bitmap_and_image_bitmap_options(
                direct.bitmap(),
                &options,
            )
        }
        None => {
            let rgba = frame.rgba().ok_or("a malformed frame")?;
            let data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), w, h)
                .map_err(|e| js_err("ImageData", e))?;
            window.create_image_bitmap_with_image_data_and_image_bitmap_options(&data, &options)
        }
    }
    .map_err(|e| js_err("createImageBitmap", e))?;
    let bitmap: ImageBitmap = JsFuture::from(promise)
        .await
        .map_err(|e| js_err("createImageBitmap", e))?
        .dyn_into()
        .map_err(|e| js_err("createImageBitmap", e))?;
    let (canvas, context) = crate::web::image::canvas_2d(&document, tw, th)?;
    let drawn = context.draw_image_with_image_bitmap(&bitmap, 0.0, 0.0);
    bitmap.close();
    drawn.map_err(|e| js_err("drawing the frame", e))?;
    let blank = canvas_blank(&context, tw, th)?;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let done = wasm_bindgen::closure::Closure::once_into_js(move |blob: JsValue| {
            let _ = resolve.call1(&JsValue::NULL, &blob);
        });
        if let Err(e) = canvas.to_blob_with_type_and_encoder_options(
            done.unchecked_ref(),
            "image/jpeg",
            &QUALITY.into(),
        ) {
            let _ = reject.call1(&JsValue::NULL, &e);
        }
    });
    let blob: Blob = JsFuture::from(promise)
        .await
        .map_err(|e| js_err("encoding the still", e))?
        .dyn_into()
        .map_err(|_| "the canvas gave no picture")?;
    Ok((blob, tw, th, blank))
}

/// Whether what `context` holds (`width × height`) is blank.
fn canvas_blank(
    context: &web_sys::CanvasRenderingContext2d,
    width: u32,
    height: u32,
) -> Result<bool, String> {
    let data = context
        .get_image_data(0.0, 0.0, f64::from(width), f64::from(height))
        .map_err(|e| js_err("reading the still", e))?;
    Ok(ddi_platform::video::blank_picture(&data.data()))
}

/// Whether the stored still `blob` of song `id` is blank, for a still
/// taken before that was kept; the record keeps the answer.
pub(crate) async fn check_blank(id: &str, blob: &Blob) -> Result<bool, String> {
    const SIDE: u32 = 160;
    let bitmap = crate::web::image::decode(blob, SIDE).await?;
    let (w, h) = (bitmap.width().max(1), bitmap.height().max(1));
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let (_canvas, context) = crate::web::image::canvas_2d(&document, w, h)?;
    let drawn = context.draw_image_with_image_bitmap(&bitmap, 0.0, 0.0);
    bitmap.close();
    drawn.map_err(|e| js_err("drawing the still", e))?;
    let blank = canvas_blank(&context, w, h)?;
    let db = Db::open().await?;
    if let Ok(mut record) = imported_record(&db, id).await
        && let Some(Still::Taken { blank: kept, .. }) = record.still.as_mut()
    {
        *kept = Some(blank);
        let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
        db.write(vec![Write::Put {
            store: idb::SONGS,
            key: id.to_string(),
            value: JsValue::from_str(&json),
        }])
        .await?;
    }
    Ok(blank)
}
