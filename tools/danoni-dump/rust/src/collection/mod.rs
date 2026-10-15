//! Reviewed, bounded static-index discovery and per-work native imports.
mod cli;
mod discovery;
mod execute;
mod rows;
use crate::{
    Error, Result,
    acquisition::{self, SourceAddress},
    assets::sha256,
};
pub use cli::{discover_job, execute_job};
pub use discovery::*;
pub use execute::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-collection", message)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    #[serde(rename = "autumnleaves-flash/1")]
    AutumnLeaves,
    #[serde(rename = "cw7-catalogue/1")]
    Cw7Catalogue,
    #[serde(rename = "7fes06-works/1")]
    Festival2006,
    #[serde(rename = "iso-flash/1")]
    IsoFlash,
}
impl Route {
    pub fn recognize(address: &SourceAddress) -> Result<Self> {
        let u = url::Url::parse(&address.logical_url).map_err(|e| invalid(e.to_string()))?;
        if u.port().is_some() {
            return Err(Error::new(
                "unsupported-index-route",
                "Non-default index port",
            ));
        }
        let path = u.path();
        let route = match (u.host_str(), path) {
            (Some("autumnleaves.yukishigure.com"), "/danoni.html") => Self::AutumnLeaves,
            (Some("cw7.sakura.ne.jp"), "/event/7fes06/works.html") => Self::Festival2006,
            (Some("eucaly.net"), "/~iso50/flash.html") => Self::IsoFlash,
            (Some("cw7.sakura.ne.jp"), p) if catalogue(p).is_some() => {
                let keys = u
                    .query_pairs()
                    .filter(|(k, _)| k == "keys")
                    .map(|(_, v)| v.into_owned())
                    .collect::<Vec<_>>();
                if keys.len() > 1
                    || keys
                        .iter()
                        .any(|s| !["5", "7", "7i", "9A", "9B"].contains(&s.as_str()))
                {
                    return Err(Error::new(
                        "unsupported-index-mode",
                        "Catalogue key filter is outside supported modes",
                    ));
                }
                Self::Cw7Catalogue
            }
            _ => {
                return Err(Error::new(
                    "unsupported-index-route",
                    "Use a reviewed work-list URL or an individual work URL",
                ));
            }
        };
        Ok(route)
    }
}
fn catalogue(path: &str) -> Option<&str> {
    let s = path.strip_prefix("/lst/ds")?.strip_suffix('/')?;
    (s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit())).then_some(s)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    Candidate,
    Unavailable,
    NavigationRequired,
    UnsupportedAddress,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub row: usize,
    pub column: usize,
    pub label: String,
    pub href: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub kind: EntryKind,
    pub address: Option<SourceAddress>,
    pub label: String,
    pub occurrences: Vec<Occurrence>,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub schema: &'static str,
    pub route: Route,
    pub source: SourceAddress,
    pub body_sha256: String,
    pub encoding: String,
    pub title: Option<String>,
    pub entries: Vec<Entry>,
    pub complete: bool,
    pub omitted: usize,
    pub diagnostics: Vec<String>,
    pub has_inline_script: bool,
}
/// Selection by reviewed row structure. No work page, script, frame or music is
/// fetched here; no source labels/ratings are promoted to song metadata.
pub fn inspect(
    url: &str,
    body: &[u8],
    encoding: Option<&str>,
    maximum_entries: usize,
) -> Result<Inventory> {
    if !(1..=1024).contains(&maximum_entries) {
        return Err(invalid("maximumEntries must be 1–1024"));
    }
    let page = acquisition::inspect_page(url, body, encoding)?;
    let route = Route::recognize(&page.source)?;
    let (text, _) = acquisition::decode(body, Some(&page.encoding))?;
    let parsed = rows::parse(&text)?;
    let mut selected = Vec::new();
    if route == Route::Festival2006 {
        for (i, c) in parsed.music.iter().enumerate() {
            if rows::label(&c.text) != "Music Name" && !rows::label(&c.text).is_empty() {
                selected.push((i, 0, c));
            }
        }
    } else {
        let author = url::Url::parse(&page.source.logical_url)
            .ok()
            .and_then(|u| catalogue(u.path()).map(str::to_owned));
        for (i, r) in parsed.rows.iter().enumerate() {
            if r.len() < 2 {
                continue;
            }
            let first = rows::label(&r[0].text);
            let number = first.nfkc().collect::<String>();
            let admitted = match route {
                Route::AutumnLeaves => {
                    r.len() == 6
                        && first.to_ascii_lowercase().starts_with("no")
                        && first.chars().any(|c| c.is_ascii_digit())
                }
                Route::IsoFlash => {
                    r.len() == 6
                        && !number.is_empty()
                        && number.chars().all(|c| c.is_ascii_digit() || c == '.')
                        && number.chars().any(|c| c.is_ascii_digit())
                }
                Route::Cw7Catalogue => {
                    r.len() >= 9
                        && r[0].links.iter().any(|(href, _)| {
                            page.base
                                .resolve(href)
                                .ok()
                                .and_then(|a| url::Url::parse(&a.logical_url).ok())
                                .is_some_and(|u| {
                                    u.host_str() == Some("cw7.sakura.ne.jp")
                                        && u.path().starts_with(&format!(
                                            "/dtl/ds{}/",
                                            author.as_deref().unwrap_or("")
                                        ))
                                })
                        })
                }
                Route::Festival2006 => false,
            };
            if admitted {
                selected.push((i, 1, &r[1]));
            }
        }
    }
    if selected.is_empty() {
        return Err(Error::new(
            "unsupported-index-structure",
            "Reviewed work rows were not found; no generic crawl fallback",
        ));
    }
    let mut result = Inventory {
        schema: "ddi-danoni-index-inventory/1",
        route,
        source: page.source,
        body_sha256: page.body_sha256,
        encoding: page.encoding,
        title: page.title,
        entries: vec![],
        complete: true,
        omitted: 0,
        diagnostics: page.diagnostics,
        has_inline_script: page.has_inline_script,
    };
    let mut seen = BTreeMap::<String, usize>::new();
    for (row, column, c) in selected {
        let links = if c.links.is_empty() {
            vec![(None, rows::label(&c.text))]
        } else {
            c.links
                .iter()
                .map(|(h, t)| (Some(h.clone()), rows::label(t)))
                .collect()
        };
        for (href, label) in links {
            let label = if label.is_empty() {
                rows::label(&c.text)
            } else {
                label
            };
            let occurrence = Occurrence {
                row,
                column,
                label: label.clone(),
                href: href.clone(),
            };
            let resolved = href.as_deref().map(|h| {
                if h.trim().is_empty() || h.trim().starts_with('#') {
                    Err(invalid("Empty/fragment work destination"))
                } else {
                    page.base.resolve(h)
                }
            });
            let (address, kind, reason) = match resolved {
                None => (
                    None,
                    EntryKind::Unavailable,
                    Some("Publisher row has no work link; no guessed URL".into()),
                ),
                Some(Err(e)) => (None, EntryKind::UnsupportedAddress, Some(e.to_string())),
                Some(Ok(a)) if route == Route::IsoFlash => (
                    Some(a),
                    EntryKind::NavigationRequired,
                    Some(
                        "Reviewed source requires genuine browser navigation/referrer context"
                            .into(),
                    ),
                ),
                Some(Ok(a)) => (Some(a), EntryKind::Candidate, None),
            };
            if let Some(a) = &address
                && let Some(i) = seen.get(&a.retrieval_url)
            {
                result.entries[*i].occurrences.push(occurrence);
                continue;
            }
            if result.entries.len() == maximum_entries {
                result.complete = false;
                result.omitted += 1;
                continue;
            }
            let key = address
                .as_ref()
                .map(|a| a.retrieval_url.clone())
                .unwrap_or_else(|| {
                    format!(
                        "row:{row}:column:{column}:{}",
                        href.as_deref().unwrap_or("")
                    )
                });
            let id = sha256(
                &serde_json::to_vec(&("ddi-danoni-index-entry/1", &result.source.logical_url, key))
                    .map_err(|e| invalid(e.to_string()))?,
            );
            if let Some(a) = &address {
                seen.insert(a.retrieval_url.clone(), result.entries.len());
            }
            result.entries.push(Entry {
                id,
                kind,
                address,
                label,
                occurrences: vec![occurrence],
                reason,
            });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
