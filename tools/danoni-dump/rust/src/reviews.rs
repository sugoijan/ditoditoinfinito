//! Source-bound reviews, parsed as passive data. No page script is executed.
use crate::{Error, Result, assets::*, model::*, policy};
use html5ever::{
    tendril::StrTendril,
    tokenizer::{
        BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
use unicode_normalization::UnicodeNormalization;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    schema: String,
    source_manifest_sha256: String,
    charts: BTreeMap<String, Value>,
}
pub(crate) struct Reviews {
    pub grids: BTreeMap<String, MusicalGrid>,
    pub motions: BTreeMap<String, NativeMotion>,
    pub ratings: BTreeMap<String, Value>,
    pub artwork: BTreeMap<String, Artwork>,
}
#[derive(Clone)]
pub(crate) struct Artwork {
    pub bytes: Arc<Vec<u8>>,
    pub summary: Value,
}
#[derive(Clone)]
pub(crate) struct Entry {
    pub chart: Chart,
    pub raw_chart: Value,
    pub metadata: Metadata,
    pub reference: Value,
    pub music: RelativeBinding,
    pub source_stars: Option<u16>,
}

fn rejected(message: &str) -> Error {
    Error::new("invalid-review", message)
}
fn document<S: AssetSource>(
    inputs: &mut Inputs<'_, S>,
    binding: &Binding,
    hash: &str,
    schema: &str,
    entries: &[Entry],
    complete: bool,
) -> Result<Document> {
    let raw = inputs.checked(binding, JSON_LIMIT)?;
    let review: Document = json(&raw)?;
    if review.schema != schema || review.source_manifest_sha256 != hash {
        return Err(rejected(
            "Review must bind the exact normalized source manifest",
        ));
    }
    if review
        .charts
        .keys()
        .any(|id| !entries.iter().any(|e| &e.chart.id == id))
        || (complete && review.charts.len() != entries.len())
    {
        return Err(rejected("Review chart coverage does not match the work"));
    }
    Ok(review)
}

#[derive(Default)]
struct Visible {
    ignored: RefCell<Vec<String>>,
    parts: RefCell<String>,
}
impl TokenSink for Visible {
    type Handle = ();
    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        match token {
            Token::CharacterTokens(value) if self.ignored.borrow().is_empty() => {
                self.parts.borrow_mut().push_str(&value)
            }
            Token::TagToken(tag) => {
                let name = tag.name.as_ref();
                let inline = ["span", "font", "b", "strong", "em", "i", "a"].contains(&name);
                if tag.kind == TagKind::StartTag
                    && ["script", "style", "textarea", "title"].contains(&name)
                {
                    self.ignored.borrow_mut().push(name.to_owned());
                    return TokenSinkResult::RawData(match name {
                        "script" => RawKind::ScriptData,
                        "style" => RawKind::Rawtext,
                        _ => RawKind::Rcdata,
                    });
                }
                if tag.kind == TagKind::EndTag && !self.ignored.borrow().is_empty() {
                    if self.ignored.borrow().last().is_some_and(|v| v == name) {
                        self.ignored.borrow_mut().pop();
                    }
                } else if self.ignored.borrow().is_empty() && !inline {
                    self.parts.borrow_mut().push(' ');
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
pub fn visible_text(body: &[u8], encoding: &str) -> Result<String> {
    let (encoding, body) = if encoding.eq_ignore_ascii_case("utf-8-sig") {
        (
            encoding_rs::UTF_8,
            body.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(body),
        )
    } else if encoding.eq_ignore_ascii_case("cp932") {
        (encoding_rs::SHIFT_JIS, body)
    } else {
        (
            encoding_rs::Encoding::for_label(encoding.as_bytes())
                .ok_or_else(|| rejected("Unsupported page encoding"))?,
            body,
        )
    };
    let (decoded, had_errors) = encoding.decode_without_bom_handling(body);
    if had_errors {
        return Err(rejected("Invalid encoded page bytes"));
    }
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from_slice(&decoded));
    let tokenizer = Tokenizer::new(Visible::default(), Default::default());
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    let normalized = tokenizer.sink.parts.borrow().nfkc().collect::<String>();
    Ok(normalized
        .split(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" "))
}
fn filled(raw: &str) -> Result<u16> {
    if !raw
        .chars()
        .all(|c| c == '★' || c == '☆' || c.is_whitespace())
        || !raw.contains('★')
    {
        return Err(rejected("Expected explicit filled stars"));
    }
    u16::try_from(raw.chars().filter(|c| *c == '★').count())
        .map_err(|_| rejected("Oversized star count"))
}
fn integer(raw: &str) -> Result<u16> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(rejected("Expected explicit integer rating"));
    }
    raw.parse()
        .map_err(|_| rejected("Oversized numeric rating"))
}
pub fn rating(notation: &str, token: &str) -> Result<Value> {
    let token = token.nfkc().collect::<String>();
    let (low, high) = match notation {
        "filled-stars" => {
            let count = filled(&token)?;
            (count, count)
        }
        "star-range" => {
            let parts = token
                .split(['→', '〜', '～', '~', '-'])
                .map(str::trim)
                .collect::<Vec<_>>();
            if parts.len() != 2 {
                return Err(rejected("Need two star-range endpoints"));
            }
            (filled(parts[0])?, filled(parts[1])?)
        }
        "star-number" => {
            let Some(first) = token.chars().next() else {
                return Err(rejected("Missing star number"));
            };
            if !['★', '☆'].contains(&first) {
                return Err(rejected("Need an explicit numbered star"));
            }
            let mut rest = token[first.len_utf8()..].trim_start();
            if rest.starts_with(['×', 'x', '*']) {
                rest = &rest[rest.chars().next().unwrap().len_utf8()..];
                rest = rest.trim_start();
            }
            let count = integer(rest.strip_suffix('?').unwrap_or(rest))?;
            (count, count)
        }
        "integer" => {
            let count = integer(&token)?;
            (count, count)
        }
        _ => return Err(rejected("Unknown rating notation")),
    };
    if low > high || high > 999 {
        return Err(rejected(
            "Rating range must be increasing and within 0..999",
        ));
    }
    Ok(
        json!({"kind":if low==high{"single"}else{"range"},"minimum":low,"maximum":high,"rawNotation":token,"notation":notation,"uncertain":token.ends_with('?')}),
    )
}
fn token_count(excerpt: &str, token: &str, integer: bool) -> usize {
    excerpt
        .match_indices(token)
        .filter(|(index, _)| {
            !integer
                || (!excerpt[..*index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_numeric)
                    && !excerpt[*index + token.len()..]
                        .chars()
                        .next()
                        .is_some_and(char::is_numeric))
        })
        .count()
}
fn relative(value: &Value) -> Result<RelativeBinding> {
    Ok(RelativeBinding {
        path: text(value, "path")?.into(),
        sha256: text(value, "sha256")?.into(),
    })
}

pub(crate) fn load<S: AssetSource>(
    inputs: &mut Inputs<'_, S>,
    hash: &str,
    entries: &[Entry],
    swf: Option<&str>,
    bindings: &crate::work::ReviewBindings,
) -> Result<Reviews> {
    let (mut grids, mut motions, mut ratings, mut artwork) = (
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    );
    if let Some(binding) = &bindings.grid {
        for (id, row) in document(
            inputs,
            binding,
            hash,
            "ddi-reviewed-musical-grid/experimental-1",
            entries,
            true,
        )?
        .charts
        {
            let grid: MusicalGrid =
                serde_json::from_value(row).map_err(|e| rejected(&e.to_string()))?;
            crate::timing::validate_grid(&grid)?;
            grids.insert(id, grid);
        }
    }
    if let Some(binding) = &bindings.motion {
        for (id, row) in document(
            inputs,
            binding,
            hash,
            "ddi-reviewed-native-scroll/experimental-1",
            entries,
            true,
        )?
        .charts
        {
            let motion: NativeMotion =
                serde_json::from_value(row.clone()).map_err(|e| rejected(&e.to_string()))?;
            policy::base_ratio(Some(&motion), 75.0, 1.0)?;
            if row["evidence"]["path"].is_string() {
                let body = inputs.checked(
                    &binding.sibling(&relative(&row["evidence"])?)?,
                    EVIDENCE_LIMIT,
                )?;
                let measured: Value = json(&body)?;
                if measured["schema"] != "ddi-native-motion-measurement/experimental-1"
                    || measured["baseArrowHeightsPerSecond"] != row["baseArrowHeightsPerSecond"]
                    || swf.is_some_and(|s| measured["sourceSwfSha256"] != s)
                {
                    return Err(rejected("Native motion disagrees with source evidence"));
                }
            }
            motions.insert(id, motion);
        }
    }
    if let Some(binding) = &bindings.ratings {
        let doc = document(
            inputs,
            binding,
            hash,
            "ddi-reviewed-source-ratings/1",
            entries,
            false,
        )?;
        let mut texts = BTreeMap::new();
        for (id, row) in doc.charts {
            let entry = entries.iter().find(|e| e.chart.id == id).unwrap();
            let label = entry
                .metadata
                .get("difficultyLabel")
                .or_else(|| entry.metadata.get("label"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if row["chartIdentity"] != json!({"mode":entry.chart.mode.label(),"label":label}) {
                return Err(rejected("Rating chart identity mismatch"));
            }
            let mut extracted = Vec::new();
            for evidence in row["ratings"]
                .as_array()
                .ok_or_else(|| rejected("Need rating evidence rows"))?
            {
                let relative = relative(&evidence["evidence"])?;
                let body = inputs.checked(&binding.sibling(&relative)?, EVIDENCE_LIMIT)?;
                let url = text(&evidence["evidence"], "url")?;
                if !(url.starts_with("http://") || url.starts_with("https://"))
                    || text(&evidence["evidence"], "retrievedAt")?.is_empty()
                {
                    return Err(rejected(
                        "Rating evidence needs public retrieval provenance",
                    ));
                }
                let encoding = text(evidence, "encoding")?;
                let key = (relative.sha256.clone(), encoding.to_owned());
                if !texts.contains_key(&key) {
                    texts.insert(key.clone(), visible_text(&body, encoding)?);
                }
                let page = &texts[&key];
                let excerpt = text(evidence, "excerpt")?;
                let token = text(evidence, "token")?;
                let notation = text(evidence, "notation")?;
                if excerpt.is_empty()
                    || token.is_empty()
                    || page.matches(excerpt).count() != 1
                    || token_count(excerpt, token, notation == "integer") != 1
                {
                    return Err(rejected(
                        "Rating excerpt/token must be unambiguous in saved HTML",
                    ));
                }
                let mut value = rating(notation, token)?;
                let scale = text(evidence, "scale")?;
                if scale.is_empty() {
                    return Err(rejected("Missing publisher rating scale"));
                }
                for (key, value_) in [
                    ("scale", json!(scale)),
                    ("authority", json!(text(evidence, "authority")?)),
                    ("excerpt", json!(excerpt)),
                    ("source", evidence["evidence"].clone()),
                    ("encoding", json!(encoding)),
                    (
                        "method",
                        json!(
                            "reviewed exact visible-HTML excerpt; numeric notation parsed by tool"
                        ),
                    ),
                ] {
                    value[key] = value_;
                }
                extracted.push(value);
            }
            let selected = row["selectedRating"]
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .ok_or_else(|| rejected("Invalid rating selection"))?;
            let chosen = extracted
                .get(selected)
                .ok_or_else(|| rejected("Rating selection escapes evidence"))?;
            if chosen["uncertain"] == true {
                return Err(rejected(
                    "An uncertain publisher rating cannot set the meter",
                ));
            }
            let stars = chosen["maximum"].as_u64().unwrap() as u16;
            if entry.source_stars.is_some_and(|n| n != stars) {
                return Err(rejected("Page rating conflicts with runtime rating"));
            }
            let meter_policy = if chosen["kind"] == "single" {
                "source count"
            } else {
                "maximum of published source range"
            };
            ratings.insert(id,json!({"ratings":extracted,"selectedRating":selected,"meter":stars,"selectionReason":text(&row,"selectionReason")?,"meterPolicy":meter_policy}));
        }
    }
    if let Some(binding) = &bindings.artwork {
        for (id, row) in document(
            inputs,
            binding,
            hash,
            "ddi-reviewed-song-artwork/1",
            entries,
            false,
        )?
        .charts
        {
            let image = inputs.checked(
                &binding.sibling(&relative(&row["banner"])?)?,
                EVIDENCE_LIMIT,
            )?;
            if image.len() < 33
                || &image[..16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"
                || u32::from_be_bytes(image[16..20].try_into().unwrap()) != 640
                || u32::from_be_bytes(image[20..24].try_into().unwrap()) != 200
            {
                return Err(rejected("Need a reviewed 640x200 PNG banner"));
            }
            let evidence_binding = relative(&row["evidence"])?;
            let raw = inputs.checked(&binding.sibling(&evidence_binding)?, EVIDENCE_LIMIT)?;
            let evidence: Value = json(&raw)?;
            let image_hash = sha256(&image);
            match text(&row, "origin")? {
                "native-title-panel" => {
                    if evidence["schema"] != "ddi-native-title-panel/1"
                        || swf.is_none_or(|s| evidence["sourceSwfSha256"] != s)
                        || evidence["files"]["banner.png"] != image_hash
                        || evidence["titleReplaced"] != false
                    {
                        return Err(rejected(
                            "Native title panel is not bound to the source/banner",
                        ));
                    }
                }
                "plain-title" => {
                    if evidence["schema"] != "ddi-artwork-study/experimental-1"
                        || evidence["template"] != "plain-title/1"
                        || !evidence["records"].as_array().is_some_and(|rs| {
                            rs.iter()
                                .any(|r| r["role"] == "banner" && r["pngSha256"] == image_hash)
                        })
                    {
                        return Err(rejected("Plain-title evidence does not match banner"));
                    }
                }
                _ => return Err(rejected("Unknown artwork origin")),
            }
            artwork.insert(id,Artwork { bytes:image,summary:json!({"sha256":image_hash,"dimensions":[640,200],"origin":row["origin"],"evidenceSha256":evidence_binding.sha256}) });
        }
    }
    Ok(Reviews {
        grids,
        motions,
        ratings,
        artwork,
    })
}
