//! The reviewed worker configuration is data, never a command line or script.
use super::*;
use serde::Deserialize;
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Config {
    pub movie: String,
    pub source_url: String,
    pub page_url: Option<String>,
    #[serde(default)]
    pub flash_vars: BTreeMap<String, String>,
    pub capture_frames: Vec<u64>,
    #[serde(default)]
    pub render: bool,
    #[serde(default)]
    pub mix_audio: bool,
    #[serde(default)]
    pub inspect_globals: bool,
    pub render_scale: Option<u64>,
    pub inspection_scale: Option<u64>,
    pub avm_execution_ms: Option<u64>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub resources: Vec<Resource>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Resource {
    pub url: String,
    pub path: String,
    pub sha256: String,
    pub http_charset: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Event {
    Click {
        frame: u64,
        x: f64,
        y: f64,
    },
    Enter {
        frame: u64,
        #[serde(rename = "holdFrames")]
        hold_frames: Option<u64>,
    },
    Key {
        frame: u64,
        code: String,
        #[serde(rename = "holdFrames")]
        hold_frames: Option<u64>,
    },
    Text {
        frame: u64,
        value: String,
    },
}
fn address(raw: &str) -> Result<String> {
    if raw.len() > 8192 {
        return Err(invalid("URL limit"));
    }
    let u = url::Url::parse(raw).map_err(|e| invalid(e.to_string()))?;
    if !matches!(u.scheme(), "http" | "https")
        || !u.has_host()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
    {
        return Err(invalid(
            "Need anonymous HTTP(S) resource identity without a fragment",
        ));
    }
    Ok(u.to_string())
}
impl Config {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        let c: Self = serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
        relative_path(&c.movie)?;
        address(&c.source_url)?;
        if let Some(page) = &c.page_url {
            address(page)?;
        }
        if c.capture_frames.is_empty()
            || c.capture_frames.len() > 16
            || c.capture_frames.iter().any(|f| !(1..=1200).contains(f))
            || c.capture_frames.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(invalid(
                "Need 1..16 strictly increasing capture frames in 1..1200",
            ));
        }
        if !(1..=4).contains(&c.render_scale.unwrap_or(1))
            || (!c.render && c.render_scale.unwrap_or(1) != 1)
            || !(1..=4).contains(&c.inspection_scale.unwrap_or(1))
            || !(50..=10000).contains(&c.avm_execution_ms.unwrap_or(500))
        {
            return Err(invalid("Unreviewed render/inspection/execution bounds"));
        }
        if c.flash_vars.len() > 128
            || c.flash_vars
                .iter()
                .any(|(k, v)| k.len() > 8192 || v.len() > 8192)
            || c.events.len() > 64
            || c.resources.len() > 64
        {
            return Err(invalid("Worker input inventory limit"));
        }
        for e in &c.events {
            let (frame, held) = match e {
                Event::Click { frame, x, y } => {
                    if !(0.0..=320.0).contains(x) || !(0.0..=240.0).contains(y) {
                        return Err(invalid("Click outside logical viewport"));
                    }
                    (*frame, 1)
                }
                Event::Enter { frame, hold_frames } => (*frame, hold_frames.unwrap_or(1)),
                Event::Key {
                    frame,
                    code,
                    hold_frames,
                } => {
                    if ![
                        "Enter",
                        "ArrowLeft",
                        "ArrowRight",
                        "ArrowUp",
                        "ArrowDown",
                        "Space",
                    ]
                    .contains(&code.as_str())
                    {
                        return Err(invalid("Unsupported native key"));
                    }
                    (*frame, hold_frames.unwrap_or(1))
                }
                Event::Text { frame, value } => {
                    if value.is_empty()
                        || value.chars().count() > 32
                        || value.chars().any(char::is_control)
                    {
                        return Err(invalid("Text requires 1..32 printable characters"));
                    }
                    (*frame, 1)
                }
            };
            if !(1..=1200).contains(&frame) || !(1..=120).contains(&held) || frame + held > 1201 {
                return Err(invalid("Event/held-key frame bounds"));
            }
        }
        let mut urls = BTreeSet::new();
        for r in &c.resources {
            relative_path(&r.path)?;
            if !valid_hash(&r.sha256) || !urls.insert(address(&r.url)?) {
                return Err(invalid("Invalid hash or duplicate canonical resource URL"));
            }
            if let Some(label) = &r.http_charset
                && encoding_rs::Encoding::for_label(label.as_bytes()).is_none()
            {
                return Err(invalid("Unknown HTTP charset"));
            }
        }
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> Value {
        json!({"movie":"blobs/movie.swf","sourceUrl":"https://example.test/song.swf?name=num301","captureFrames":[1,2],"events":[],"resources":[]})
    }
    fn parse(v: &Value) -> Result<Config> {
        Config::parse(&serde_json::to_vec(v).unwrap())
    }
    #[test]
    fn configuration_keeps_url_queries_and_rejects_unsafe_or_unknown_inputs() {
        let v = base();
        let c = parse(&v).unwrap();
        assert!(c.source_url.ends_with("?name=num301"));
        for (key, value) in [
            ("movie", json!("../escape.swf")),
            ("sourceUrl", json!("https://user:password@example.test/a")),
            ("sourceUrl", json!("file:///tmp/movie")),
            ("captureFrames", json!([1, 1])),
            ("captureFrames", json!([1201])),
            ("renderScale", json!(4)),
            ("inspectionScale", json!(5)),
            ("avmExecutionMs", json!(10001)),
            ("shell", json!("arbitrary")),
        ] {
            let mut bad = v.clone();
            bad[key] = value;
            assert!(parse(&bad).is_err(), "{key}");
        }
        let mut bad = v.clone();
        bad["events"] = json!([{"frame":1200,"type":"key","code":"Enter","holdFrames":120}]);
        assert!(parse(&bad).is_err());
        bad = v.clone();
        bad["events"] = json!([{"frame":1,"type":"text","value":"\n"}]);
        assert!(parse(&bad).is_err());
        bad = v;
        bad["resources"] = json!([{"url":"https://EXAMPLE.test:443/a","path":"a","sha256":"0".repeat(64)},{"url":"https://example.test/a","path":"b","sha256":"0".repeat(64)}]);
        assert!(parse(&bad).is_err());
    }
    #[test]
    fn budgets_cannot_disable_confinement_or_raise_reviewed_limits() {
        let d = Limits::default();
        d.validate().unwrap();
        for bad in [
            Limits {
                wall_ms: 0,
                ..d.clone()
            },
            Limits {
                wall_ms: 60001,
                ..d.clone()
            },
            Limits {
                resident_mib: 0,
                ..d.clone()
            },
            Limits {
                resident_mib: 2049,
                ..d.clone()
            },
            Limits {
                cpu_seconds: 21,
                ..d.clone()
            },
            Limits {
                file_bytes: 40 * 1024 * 1024 + 1,
                ..d.clone()
            },
            Limits {
                output_bytes: 256 * 1024 * 1024 + 1,
                ..d
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }
}
