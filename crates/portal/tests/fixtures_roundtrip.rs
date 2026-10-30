use pcbcraft_portal::api::{
    CapabilitiesDocument, PublicationListResponse, StaticFeedResponse, UploadInitResponse,
};
use pcbcraft_portal::moderation::QueueListResponse;

fn parse_fixture<T: serde::de::DeserializeOwned>(name: &str) -> Result<T, String> {
    let path = format!("tests/fixtures/{name}");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read {path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {path}: {e}"))
}

#[test]
fn api_fixtures_round_trip() -> Result<(), String> {
    let caps: CapabilitiesDocument = parse_fixture("capabilities.json")?;
    assert_eq!(caps.api_version, 1);
    assert!(caps.anonymous_browse);

    let upload: UploadInitResponse = parse_fixture("upload-init-response.json")?;
    assert!(!upload.upload_id.is_empty());

    let list: PublicationListResponse = parse_fixture("publication-list.json")?;
    assert_eq!(list.items.len(), 1);

    let feed: StaticFeedResponse = parse_fixture("static-feed.json")?;
    assert_eq!(feed.feed.schema_version, 1);

    let queue: QueueListResponse = parse_fixture("moderation-queue.json")?;
    assert_eq!(queue.items.len(), 1);

    let caps_json = serde_json::to_string(&caps).map_err(|e| e.to_string())?;
    let caps_again: CapabilitiesDocument =
        serde_json::from_str(&caps_json).map_err(|e| e.to_string())?;
    assert_eq!(caps, caps_again);
    Ok(())
}
