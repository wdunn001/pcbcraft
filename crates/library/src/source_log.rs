//! Plain logging helpers for live sources (no secrets).

/// Reserved for structured logs: source id, query length, result count.
pub fn query_summary(source_id: &str, query_len: usize, result_count: usize) {
    let _ = (source_id, query_len, result_count);
}
