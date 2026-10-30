//! LRU eviction for the per-user datasheet cache index.

use crate::record::CacheRecord;

/// Drop least-recently-used records until the total byte size is at most `cap_bytes`.
///
/// Returns the SHA-256 keys whose bytes should be removed from storage.
pub fn evict_for_cap(records: &mut Vec<CacheRecord>, cap_bytes: u64) -> Vec<String> {
    let mut total: u64 = records.iter().map(|r| r.byte_len).sum();
    if total <= cap_bytes {
        return Vec::new();
    }
    records.sort_by(|a, b| a.last_used_at.cmp(&b.last_used_at));
    let mut removed = Vec::new();
    while total > cap_bytes {
        let Some(oldest) = records.first() else {
            break;
        };
        total = total.saturating_sub(oldest.byte_len);
        removed.push(oldest.sha256.clone());
        records.remove(0);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::CacheRecord;

    fn rec(key: &str, bytes: u64, used: &str) -> CacheRecord {
        CacheRecord {
            sha256: key.into(),
            source_url: None,
            fetched_at: "2020-01-01T00:00:00Z".into(),
            etag: None,
            byte_len: bytes,
            last_used_at: used.into(),
        }
    }

    #[test]
    fn eviction_keeps_the_total_under_the_cap() {
        let mut index = vec![
            rec("a", 300, "2020-01-01T00:00:00Z"),
            rec("b", 300, "2020-01-02T00:00:00Z"),
            rec("c", 300, "2020-01-03T00:00:00Z"),
        ];
        let dropped = evict_for_cap(&mut index, 500);
        assert_eq!(dropped, vec!["a"]);
        assert_eq!(index.len(), 2);
        assert!(index.iter().all(|r| r.sha256 == "b" || r.sha256 == "c"));
    }
}
