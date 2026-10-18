use super::*;
use crate::assets::Binding;
use robotstxt::{DefaultMatcher, RobotsParseHandler};
use serde::Serialize;
/// RFC-sized robots input; the host must enforce this while receiving bytes.
pub const ROBOTS_LIMIT: usize = 512 * 1024;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RobotsDecision {
    pub target_url: String,
    pub robots_url: String,
    pub retrieval_url: String,
    pub responses: Vec<serde_json::Value>,
    pub status: u16,
    pub user_agent: &'static str,
    pub allowed: bool,
    pub crawl_delay_ms: Option<u64>,
    pub index: Binding,
    pub body: Binding,
    pub retrieved_at: String,
}
#[derive(Default)]
struct Delay {
    specific: bool,
    global: bool,
    separator: bool,
    ever_specific: bool,
    specific_delay: Option<u64>,
    global_delay: Option<u64>,
    specific_bad: bool,
    global_bad: bool,
}
impl Delay {
    fn result(&self) -> Result<Option<u64>> {
        let (bad, delay) = if self.ever_specific {
            (self.specific_bad, self.specific_delay)
        } else {
            (self.global_bad, self.global_delay)
        };
        if bad {
            return Err(Error::new(
                "robots-unavailable",
                "Selected Crawl-delay is invalid or exceeds the host's one-hour bound",
            ));
        }
        Ok(delay)
    }
}
impl RobotsParseHandler for Delay {
    fn handle_robots_start(&mut self) {
        *self = Self::default();
    }
    fn handle_robots_end(&mut self) {}
    fn handle_user_agent(&mut self, _line: u32, agent: &str) {
        if self.separator {
            self.specific = false;
            self.global = false;
            self.separator = false;
        }
        let token = agent
            .split(|c: char| !(c.is_ascii_alphabetic() || c == '-' || c == '_'))
            .next()
            .unwrap_or("");
        if token.eq_ignore_ascii_case(USER_AGENT) {
            self.specific = true;
            self.ever_specific = true;
        }
        if agent == "*"
            || agent
                .strip_prefix('*')
                .is_some_and(|s| s.starts_with(char::is_whitespace))
        {
            self.global = true;
        }
    }
    fn handle_allow(&mut self, _line: u32, _value: &str) {
        self.separator = true;
    }
    fn handle_disallow(&mut self, _line: u32, _value: &str) {
        self.separator = true;
    }
    fn handle_sitemap(&mut self, _line: u32, _value: &str) {
        self.separator = true;
    }
    fn handle_unknown_action(&mut self, _line: u32, action: &str, value: &str) {
        self.separator = true;
        if !action.eq_ignore_ascii_case("crawl-delay") {
            return;
        }
        let seconds = value
            .parse::<f64>()
            .ok()
            .filter(|s| s.is_finite() && *s >= 0.0 && *s <= 3600.0);
        let delay = seconds.map(|s| (s * 1000.0).ceil() as u64);
        for (selected, target, bad) in [
            (
                self.specific,
                &mut self.specific_delay,
                &mut self.specific_bad,
            ),
            (self.global, &mut self.global_delay, &mut self.global_bad),
        ] {
            if selected {
                if let Some(delay) = delay {
                    *target = Some(target.unwrap_or(0).max(delay));
                } else {
                    *bad = true;
                }
            }
        }
    }
}
/// Evaluate bound historical robots evidence. Freshness, terms, DNS and live
/// transport policy remain host responsibilities.
pub fn evaluate_robots(response: &ReplayResponse, target: &str) -> Result<RobotsDecision> {
    evaluate_robots_chain(std::slice::from_ref(response), target)
}
/// The complete chain must start at the target origin's /robots.txt, have at
/// most eight redirects, and end in a bound 200/404 response. Each next request
/// must exactly match the previous hop's Location; no guessed/live fallback.
pub fn evaluate_robots_chain(responses: &[ReplayResponse], target: &str) -> Result<RobotsDecision> {
    let target = address(target)?;
    let robots_url = format!("{}/robots.txt", target.origin().ascii_serialization());
    if responses.is_empty() || responses.len() > 9 {
        return Err(Error::new(
            "robots-unavailable",
            "Need 1..9 exact robots responses",
        ));
    }
    let mut expected = robots_url.clone();
    let mut seen = std::collections::BTreeSet::new();
    for (i, response) in responses.iter().enumerate() {
        if response.request.url != expected
            || response.canonical_url != expected
            || !seen.insert(expected.clone())
            || crate::assets::sha256(&response.body) != response.body_binding.sha256
            || response.selected_response["sha256"] != response.body_binding.sha256
            || response.selected_response["byteLength"].as_u64() != Some(response.body.len() as u64)
            || response.selected_response["status"].as_u64() != Some(response.status as u64)
        {
            return Err(Error::new(
                "robots-unavailable",
                "Robots response binding/chain differs",
            ));
        }
        if response.body.len() > ROBOTS_LIMIT {
            return Err(Error::new(
                "robots-unavailable",
                "Robots response exceeds 512 KiB",
            ));
        }
        if i + 1 < responses.len() {
            if ![301, 302, 303, 307, 308].contains(&response.status) {
                return Err(Error::new(
                    "robots-unavailable",
                    "Intermediate policy response is not a redirect",
                ));
            }
            expected = response
                .redirect_to
                .clone()
                .ok_or_else(|| Error::new("robots-unavailable", "Missing robots redirect"))?;
            let location = response
                .headers
                .get("location")
                .ok_or_else(|| Error::new("robots-unavailable", "Missing robots Location"))?;
            let resolved = address(&response.response_url)?
                .join(location)
                .map_err(|e| Error::new("robots-unavailable", e.to_string()))?;
            if address(resolved.as_str())?.as_str() != expected {
                return Err(Error::new("robots-unavailable", "Robots Location differs"));
            }
        } else if response.redirect_to.is_some() || ![200, 404].contains(&response.status) {
            return Err(Error::new(
                "robots-unavailable",
                "Need a terminal robots 200/404 response",
            ));
        }
    }
    let response = responses.last().unwrap();
    let (allowed, crawl_delay_ms) = if response.status == 404 {
        (true, None)
    } else {
        let text = std::str::from_utf8(
            response
                .body
                .strip_prefix(&[0xef, 0xbb, 0xbf])
                .unwrap_or(&response.body),
        )
        .map_err(|_| Error::new("robots-unavailable", "Robots is not valid UTF-8"))?;
        if text.contains('\0') {
            return Err(Error::new("robots-unavailable", "NUL in robots input"));
        }
        let mut delay = Delay::default();
        robotstxt::parse_robotstxt(text, &mut delay);
        (
            DefaultMatcher::default().one_agent_allowed_by_robots(
                text,
                USER_AGENT,
                target.as_str(),
            ),
            delay.result()?,
        )
    };
    Ok(RobotsDecision {
        target_url: target.into(),
        robots_url,
        retrieval_url: response.response_url.clone(),
        responses: responses.iter().map(ReplayResponse::receipt).collect(),
        status: response.status,
        user_agent: USER_AGENT,
        allowed,
        crawl_delay_ms,
        index: response.index.clone(),
        body: response.body_binding.clone(),
        retrieved_at: response.selected_response["retrievedAt"]
            .as_str()
            .unwrap_or("")
            .into(),
    })
}
