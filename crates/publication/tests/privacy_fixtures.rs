use base64::Engine as _;
use pcbcraft_publication::{
    scan, scan_privacy_text, BundleFile, PrivacyOptions, PublicationScanInput, ScanOutcome,
};
use std::fs;

fn fixture(name: &str) -> String {
    fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("read fixture {name}: {e}"))
}

#[test]
fn clean_fixture_has_no_privacy_hits() {
    let text = fixture("clean/project-notes.txt");
    let hits = scan_privacy_text("project-notes.txt", &text, &PrivacyOptions::default());
    assert!(hits.is_empty(), "unexpected hits: {hits:?}");
}

#[test]
fn planted_secrets_are_all_caught() {
    let rules = [
        "private_key",
        "service_token",
        "password_in_url",
        "email_address",
        "windows_user_path",
        "private_ip",
        "local_hostname",
        "credential_field",
        "refused_file_type",
        "image_metadata",
    ];
    let mut input = PublicationScanInput {
        design_licence: "MIT".into(),
        ..Default::default()
    };
    input.files.push(BundleFile {
        path: "planted/secrets.txt".into(),
        role: "text".into(),
        text: Some(fixture("planted/secrets.txt")),
        bytes_base64: None,
    });
    input.files.push(BundleFile {
        path: "planted/preview.jpg".into(),
        role: "preview".into(),
        text: None,
        bytes_base64: Some(base64::engine::general_purpose::STANDARD.encode(
            b"\xff\xd8\xff Exif\0\0metadata",
        )),
    });
    input.files.push(BundleFile {
        path: "scripts/run.sh".into(),
        role: "text".into(),
        text: Some("#!/bin/sh\necho hi".into()),
        bytes_base64: None,
    });
    let result = scan(&input);
    assert!(!result.ok_to_upload());
    for rule in rules {
        assert!(
            result.hard_fails.iter().any(|o| match o {
                ScanOutcome::HardFail { rule: r, .. } => *r == rule,
                _ => false,
            }),
            "missing rule {rule}: {:?}",
            result.hard_fails
        );
    }
}

#[test]
fn author_email_is_allowed_when_listed() {
    let text = "Contact the author at author@example.com for questions.\n";
    let mut opts = PrivacyOptions::default();
    opts.allowed_emails.insert("author@example.com".into());
    let hits = scan_privacy_text("notes.txt", text, &opts);
    assert!(hits.is_empty());
    let other = scan_privacy_text("notes.txt", "Reach reviewer@example.com\n", &opts);
    assert!(!other.is_empty());
}
