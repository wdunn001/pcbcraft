//! A newcomer checks their design for privacy and licence issues before publishing.
use pcbcraft_engine::{Error, Session};
use serde_json::json;

#[test]
fn a_newcomer_checks_their_design_before_publishing() -> Result<(), Error> {
    let mut s = Session::default();
    // The newcomer wants to publish their design under an MIT licence.
    // They first check for any privacy or licence issues.
    let r = s.execute(
        "publication.scan",
        json!({
            "design_licence":"MIT",
            "author_emails":["author@example.com"],
            "parts":[
                {"id":"resistor","licence":"MIT","source":"public-catalog.example"},
                {"id":"opamp","licence":"CC-BY-4.0","source":"public-catalog.example"}
            ],
            "files":[
                {"path":"schematic.txt","text":"Board design notes.\nReference: https://example.com/docs"}
            ]
        }),
        None,
    )?["result"].clone();
    assert_eq!(r["ok_to_upload"], true, "a clean design passes the scan");
    // They then try again with a file that accidentally contains a secret.
    let r2 = s.execute(
        "publication.scan",
        json!({
            "design_licence":"MIT",
            "author_emails":["author@example.com"],
            "files":[
                {"path":"notes.txt","text":"token sk-live-abc123\n"}
            ]
        }),
        None,
    )?["result"]
        .clone();
    assert_eq!(
        r2["ok_to_upload"], false,
        "a design with secrets fails the scan"
    );
    assert!(
        r2["hard_fails"].as_array().is_some_and(|a| !a.is_empty()),
        "the scan reports the privacy hit"
    );
    Ok(())
}
