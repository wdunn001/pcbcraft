//! Tests for the vendor integrations configuration.

use pcbcraft_portal::config::PortalConfig;

#[test]
fn vendor_integrations_off_by_default() {
    let config_json = r#"{"instance_id":"test","title":"Test","listen":"127.0.0.1:8080","database_url":"sqlite://test.db","object_store":{"driver":"local-folder","root":"/tmp/objstore"}}"#;
    let config: PortalConfig = serde_json::from_str(config_json).unwrap();
    assert!(config.vendor_integrations.is_none());
}

#[test]
fn vendor_integrations_quotes_enabled() {
    let config_json = r#"{"instance_id":"test","title":"Test","listen":"127.0.0.1:8080","database_url":"sqlite://test.db","object_store":{"driver":"local-folder","root":"/tmp/objstore"},"vendor_integrations":{"quotes_enabled":true,"marketplace_enabled":false}}"#;
    let config: PortalConfig = serde_json::from_str(config_json).unwrap();
    let vi = config.vendor_integrations.unwrap();
    assert!(vi.quotes_enabled);
    assert!(!vi.marketplace_enabled);
}

#[test]
fn vendor_integrations_marketplace_enabled() {
    let config_json = r#"{"instance_id":"test","title":"Test","listen":"127.0.0.1:8080","database_url":"sqlite://test.db","object_store":{"driver":"local-folder","root":"/tmp/objstore"},"vendor_integrations":{"quotes_enabled":false,"marketplace_enabled":true}}"#;
    let config: PortalConfig = serde_json::from_str(config_json).unwrap();
    let vi = config.vendor_integrations.unwrap();
    assert!(!vi.quotes_enabled);
    assert!(vi.marketplace_enabled);
}
