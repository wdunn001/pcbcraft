#[test]
fn empty_distributor_yields_no_auth_headers() -> Result<(), String> {
    let headers = pcbcraft_io::auth_headers_for_distributor("")?;
    assert!(headers.is_empty());
    Ok(())
}

#[test]
fn invalid_distributor_name_is_rejected_on_store() {
    let err = pcbcraft_io::set_distributor_credential("bad/name", "token");
    assert!(err.is_err());
}
