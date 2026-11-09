//! Content addressing helpers for publication bundles.

use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Bundle root hash: SHA-256 over sorted `path:file_sha256` lines.
pub fn bundle_root_hash(files: &[(String, String)]) -> String {
    let mut lines: Vec<String> = files
        .iter()
        .map(|(path, hash)| format!("{path}:{hash}"))
        .collect();
    lines.sort();
    let body = lines.join("\n");
    sha256_hex(body.as_bytes())
}
