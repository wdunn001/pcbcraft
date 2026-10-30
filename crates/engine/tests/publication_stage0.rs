//! Ensures the engine crate links Stage 0 checks for app and CI callers.

#[test]
fn engine_exposes_publication_stage0() {
    let report = pcbcraft_engine::publication::stage0_from_zip(b"");
    assert!(!report.passes());
    assert!(report
        .failure_reasons()
        .iter()
        .any(|line| line.contains("archive")));
}
