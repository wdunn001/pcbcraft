//! Documented API hosts for opt-in part sources (library engine lane).

/// Hostnames allowed in live source URL constants (documented vendor APIs only).
pub const DOCUMENTED_HOSTS: &[&str] = &[
    "api.digikey.com",
    "api.mouser.com",
    "api.element14.com",
    "www.lcsc.com",
    "open.jlcpcb.com",
];

/// Returns true when `url` uses an HTTPS origin whose host is in [`DOCUMENTED_HOSTS`].
pub fn url_uses_documented_host(url: &str) -> bool {
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    DOCUMENTED_HOSTS.contains(&host)
}
