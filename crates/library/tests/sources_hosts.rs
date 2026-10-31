#![cfg(not(target_arch = "wasm32"))]

use pcbcraft_library::element14;
use pcbcraft_library::jlcpcb;
use pcbcraft_library::lcsc;
use pcbcraft_library::source_hosts::{self, DOCUMENTED_HOSTS};

#[test]
fn only_documented_hosts_are_configured() {
    let configured = [
        "https://api.digikey.com/v1/oauth2/token",
        "https://api.digikey.com/products/v4/search/keyword",
        "https://api.mouser.com/api/v1/search/partnumber",
        element14::CATALOG_URL,
        lcsc::KEYWORD_SEARCH_URL,
        jlcpcb::COMPONENT_DETAIL_URL,
    ];
    for url in configured {
        assert!(
            source_hosts::url_uses_documented_host(url),
            "url {url} must use a documented host"
        );
        let host = url
            .strip_prefix("https://")
            .and_then(|r| r.split('/').next())
            .unwrap_or("");
        assert!(
            DOCUMENTED_HOSTS.contains(&host),
            "host {host} missing from DOCUMENTED_HOSTS"
        );
    }
}
