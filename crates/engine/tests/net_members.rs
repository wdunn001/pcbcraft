//! Engine tests for the read-only `net.members` command.
use pcbcraft_engine::{Error, Session};
use serde_json::json;

#[test]
fn net_members_command_rejects_an_unknown_net() -> Result<(), Error> {
    let mut s = Session::sample()?;
    let before = serde_json::to_value(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
    assert!(
        s.execute("net.members", json!({"net": 987654}), None)
            .is_err()
    );
    assert!(s.execute("net.members", json!({}), None).is_err());
    let net = s
        .project
        .nets
        .first()
        .ok_or(Error::Invalid("sample has no net".into()))?
        .id;
    let got = s.execute("net.members", json!({"net": net}), None)?;
    let direct = s
        .project
        .net_members(net)
        .ok_or(Error::Invalid("no members".into()))?;
    assert_eq!(
        got["result"],
        serde_json::to_value(direct).map_err(|e| Error::Invalid(e.to_string()))?
    );
    assert_eq!(
        serde_json::to_value(&s.project).map_err(|e| Error::Invalid(e.to_string()))?,
        before,
        "a read must not change the design"
    );
    Ok(())
}
