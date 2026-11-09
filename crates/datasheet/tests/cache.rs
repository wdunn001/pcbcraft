use pcbcraft_datasheet::{gate, ingest_pdf, lru, record::CacheRecord};

#[test]
fn ingest_rejects_non_pdf() {
    let err = ingest_pdf(b"hello".to_vec(), None, None, "2026-01-01T00:00:00Z")
        .err()
        .unwrap_or_default();
    assert!(err.contains("not a valid PDF"), "{err}");
}

#[test]
fn sha256_is_stable() {
    let a = gate::sha256_hex(b"%PDF-1.4\n");
    let b = gate::sha256_hex(b"%PDF-1.4\n");
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
}

#[test]
fn lru_eviction_drops_oldest_first() {
    let mut index = vec![
        CacheRecord::new("old".into(), None, None, 100, "2020-01-01T00:00:00Z"),
        CacheRecord::new("new".into(), None, None, 100, "2020-02-01T00:00:00Z"),
    ];
    index[0].last_used_at = "2020-01-01T00:00:00Z".into();
    index[1].last_used_at = "2020-02-01T00:00:00Z".into();
    let dropped = lru::evict_for_cap(&mut index, 150);
    assert_eq!(dropped, vec!["old"]);
}
