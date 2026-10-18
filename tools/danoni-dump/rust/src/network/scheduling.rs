use super::*;
use std::net::{IpAddr, SocketAddr};
fn public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && (b == 168 || (b == 0 && (c <= 2)) || (b == 88 && c == 99)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            (0x2000..=0x3fff).contains(&s[0])
                && s[0] != 0x2002
                && !(s[0] == 0x2001
                    && (s[1] == 0 || s[1] == 0x0db8 || s[1] == 2 || (0x10..=0x2f).contains(&s[1])))
                && !(s[0] == 0x3fff && (s[1] & 0xf000) == 0)
        }
    }
}
/// Turn one resolver result set into exact dial addresses. Check every returned
/// IP, reject mixed public/private answers and never authorize a second lookup.
/// The eventual transport must connect only to these addresses with normal TLS
/// hostname verification, no proxy/redirect/retry resolver fallback.
pub fn pin_public_addresses(url: &str, ips: &[IpAddr]) -> Result<Vec<SocketAddr>> {
    let url = address(url)?;
    if ips.is_empty() || ips.len() > 64 {
        return Err(Error::new("dns-empty", "Need 1..64 bounded DNS answers"));
    }
    let host = url.host_str().unwrap().trim_matches(['[', ']']);
    if let Ok(literal) = host.parse::<IpAddr>()
        && ips.iter().any(|i| *i != literal)
    {
        return Err(Error::new("dns-identity", "Literal URL and dial IP differ"));
    }
    if ips.iter().any(|i| !public(i)) {
        return Err(Error::new(
            "private-network",
            "Refusing private, reserved or unsupported address class",
        ));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| Error::new("unsupported-url", "Missing HTTP port"))?;
    let mut pinned = vec![];
    for ip in ips {
        let a = SocketAddr::new(*ip, port);
        if !pinned.contains(&a) {
            pinned.push(a);
        }
    }
    Ok(pinned)
}
/// Monotonic-clock policy only; neither waits nor marks a request started on
/// reservation. An unstarted cancelled request therefore consumes no slot.
pub struct Pacer {
    minimum_ms: u64,
    started: BTreeMap<String, u64>,
}
impl Default for Pacer {
    fn default() -> Self {
        Self {
            minimum_ms: 1000,
            started: BTreeMap::new(),
        }
    }
}
impl Pacer {
    pub fn new(minimum_ms: u64) -> Result<Self> {
        if !(1000..=3_600_000).contains(&minimum_ms) {
            return Err(Error::new(
                "pacing-limit",
                "Minimum interval must be 1 s..1 h",
            ));
        }
        Ok(Self {
            minimum_ms,
            started: BTreeMap::new(),
        })
    }
    pub fn ready_at(&self, url: &str, now_ms: u64, crawl_delay_ms: Option<u64>) -> Result<u64> {
        let origin = address(url)?.origin().ascii_serialization();
        let delay = crawl_delay_ms.unwrap_or(0).max(self.minimum_ms);
        if delay > 3_600_000 {
            return Err(Error::new(
                "pacing-limit",
                "Cannot honor crawl delay above one hour",
            ));
        }
        match self.started.get(&origin) {
            Some(prior) if now_ms < *prior => Err(Error::new(
                "pacing-clock",
                "Monotonic clock moved backwards",
            )),
            Some(prior) => prior
                .checked_add(delay)
                .map(|n| n.max(now_ms))
                .ok_or_else(|| Error::new("pacing-clock", "Clock overflow")),
            None => Ok(now_ms),
        }
    }
    pub fn mark_started(
        &mut self,
        url: &str,
        now_ms: u64,
        crawl_delay_ms: Option<u64>,
    ) -> Result<()> {
        if self.ready_at(url, now_ms, crawl_delay_ms)? > now_ms {
            return Err(Error::new(
                "pacing-early",
                "Request started before its reserved time",
            ));
        }
        let origin = address(url)?.origin().ascii_serialization();
        if self.started.len() >= 256 && !self.started.contains_key(&origin) {
            return Err(Error::new("pacing-limit", "Origin budget exceeded"));
        }
        self.started.insert(origin, now_ms);
        Ok(())
    }
}
