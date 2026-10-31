//! Text and byte scans for secrets, personal data and local identifiers.
//!
//! # Rules
//! - Every hit includes path, line and column when the input is text.
//! - Emails listed in `PrivacyOptions::allowed_emails` are ignored.
//!
//! # Dependencies
//! - crates/publication/src/scan.rs aggregates findings.
//!
//! # Do not
//! - Log or store matched secret text beyond a short rule label.

use crate::ScanOutcome;
use std::collections::BTreeSet;

/// Addresses that may appear because they identify the publication author.
#[derive(Clone, Debug, Default)]
pub struct PrivacyOptions {
    /// Lowercase emails that are allowed outside the dedicated author field.
    pub allowed_emails: BTreeSet<String>,
}

/// One privacy or safety finding with a stable rule id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivacyFinding {
    pub rule: &'static str,
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub detail: String,
}

impl PrivacyFinding {
    fn hard(self) -> ScanOutcome {
        ScanOutcome::HardFail {
            rule: self.rule,
            path: self.path,
            line: Some(self.line),
            column: Some(self.column),
            detail: self.detail,
            action: "remove_and_retry",
        }
    }
}

/// Scan UTF-8 text for privacy violations.
pub fn scan_privacy_text(path: &str, text: &str, options: &PrivacyOptions) -> Vec<ScanOutcome> {
    let mut out = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line_u = (line_no + 1) as u32;
        scan_line(path, line_u, line, options, &mut out);
    }
    scan_multiline_pem(path, text, options, &mut out);
    out
}

/// Scan binary input for embedded keys, preview metadata and token prefixes.
pub fn scan_privacy_bytes(path: &str, bytes: &[u8], options: &PrivacyOptions) -> Vec<ScanOutcome> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return scan_privacy_text(path, text, options);
    }
    let mut out = Vec::new();
    if bytes.windows(4).any(|w| w == b"Exif") || bytes.windows(4).any(|w| w == b"eXif") {
        out.push(
            PrivacyFinding {
                rule: "image_metadata",
                path: path.into(),
                line: 0,
                column: 0,
                detail: "preview or image bytes still carry Exif metadata; strip it before upload"
                    .into(),
            }
            .hard(),
        );
    }
    if contains_subsequence(bytes, b"-----BEGIN") && contains_subsequence(bytes, b"PRIVATE KEY") {
        out.push(
            PrivacyFinding {
                rule: "private_key",
                path: path.into(),
                line: 0,
                column: 0,
                detail: "embedded private key material".into(),
            }
            .hard(),
        );
    }
    for prefix in TOKEN_PREFIXES {
        if let Some(off) = find_ascii(bytes, prefix) {
            out.push(
                PrivacyFinding {
                    rule: "service_token",
                    path: path.into(),
                    line: 0,
                    column: off as u32,
                    detail: format!("token prefix `{prefix}` in binary content"),
                }
                .hard(),
            );
        }
    }
    let _ = options;
    out
}

const TOKEN_PREFIXES: &[&str] = &[
    "sk-",
    "ghp_",
    "gho_",
    "github_pat_",
    "AKIA",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "xoxs-",
    "Bearer ",
];

fn scan_multiline_pem(
    path: &str,
    text: &str,
    _options: &PrivacyOptions,
    out: &mut Vec<ScanOutcome>,
) {
    if text.contains("-----BEGIN") && text.contains("PRIVATE KEY-----") {
        out.push(
            PrivacyFinding {
                rule: "private_key",
                path: path.into(),
                line: 1,
                column: 1,
                detail: "PEM private key block".into(),
            }
            .hard(),
        );
    }
    if text.contains("-----BEGIN OPENSSH PRIVATE KEY-----") {
        out.push(
            PrivacyFinding {
                rule: "private_key",
                path: path.into(),
                line: 1,
                column: 1,
                detail: "OpenSSH private key block".into(),
            }
            .hard(),
        );
    }
}

fn scan_line(
    path: &str,
    line_no: u32,
    line: &str,
    options: &PrivacyOptions,
    out: &mut Vec<ScanOutcome>,
) {
    for (rule, needle) in LINE_NEEDLES {
        if let Some(col) = line.find(needle) {
            out.push(
                PrivacyFinding {
                    rule,
                    path: path.into(),
                    line: line_no,
                    column: (col + 1) as u32,
                    detail: format!("matched `{needle}`"),
                }
                .hard(),
            );
        }
    }
    for prefix in TOKEN_PREFIXES {
        if let Some(col) = line.find(prefix) {
            out.push(
                PrivacyFinding {
                    rule: "service_token",
                    path: path.into(),
                    line: line_no,
                    column: (col + 1) as u32,
                    detail: format!("token prefix `{prefix}`"),
                }
                .hard(),
            );
        }
    }
    if let Some(col) = find_password_in_url(line) {
        out.push(
            PrivacyFinding {
                rule: "password_in_url",
                path: path.into(),
                line: line_no,
                column: (col + 1) as u32,
                detail: "credentials embedded in a URL".into(),
            }
            .hard(),
        );
    }
    for (col, email) in find_emails(line) {
        let lower = email.to_ascii_lowercase();
        if !options.allowed_emails.contains(&lower) {
            out.push(
                PrivacyFinding {
                    rule: "email_address",
                    path: path.into(),
                    line: line_no,
                    column: (col + 1) as u32,
                    detail: "email address outside the author field".into(),
                }
                .hard(),
            );
        }
    }
    scan_home_paths(path, line_no, line, out);
    scan_private_hosts(path, line_no, line, out);
    scan_credential_json_keys(path, line_no, line, out);
}

const LINE_NEEDLES: &[(&str, &str)] = &[
    ("windows_user_path", "\\Users\\"),
    ("windows_user_path", ":\\Users\\"),
    ("unix_home_path", "/home/"),
    ("unix_home_path", "/Users/"),
];

fn scan_home_paths(path: &str, line_no: u32, line: &str, out: &mut Vec<ScanOutcome>) {
    for (rule, needle) in LINE_NEEDLES {
        if let Some(col) = line.find(needle) {
            out.push(
                PrivacyFinding {
                    rule,
                    path: path.into(),
                    line: line_no,
                    column: (col + 1) as u32,
                    detail: "absolute path with a user or home name".into(),
                }
                .hard(),
            );
        }
    }
}

fn scan_private_hosts(path: &str, line_no: u32, line: &str, out: &mut Vec<ScanOutcome>) {
    let lower = line.to_ascii_lowercase();
    if lower.contains("localhost") || lower.contains(".local") {
        if let Some(col) = lower.find("localhost").or_else(|| lower.find(".local")) {
            out.push(
                PrivacyFinding {
                    rule: "local_hostname",
                    path: path.into(),
                    line: line_no,
                    column: (col + 1) as u32,
                    detail: "local host name".into(),
                }
                .hard(),
            );
        }
    }
    for (col, _) in find_private_ipv4(line) {
        out.push(
            PrivacyFinding {
                rule: "private_ip",
                path: path.into(),
                line: line_no,
                column: (col + 1) as u32,
                detail: "private or loopback IPv4 address".into(),
            }
            .hard(),
        );
    }
}

fn scan_credential_json_keys(path: &str, line_no: u32, line: &str, out: &mut Vec<ScanOutcome>) {
    let trimmed = line.trim();
    for key in ["api_key", "apikey", "password", "secret", "token"] {
        let pattern = format!("\"{key}\"");
        if trimmed.contains(&pattern) && trimmed.contains(':') && !trimmed.contains(": \"\"") {
            if let Some(col) = trimmed.find(&pattern) {
                out.push(
                    PrivacyFinding {
                        rule: "credential_field",
                        path: path.into(),
                        line: line_no,
                        column: (col + 1) as u32,
                        detail: format!("project credential field `{key}` with a value"),
                    }
                    .hard(),
                );
            }
        }
    }
}

fn find_password_in_url(line: &str) -> Option<usize> {
    let scheme = line.find("://")?;
    let rest = &line[scheme + 3..];
    let at = rest.find('@')?;
    let creds = &rest[..at];
    creds.contains(':').then_some(scheme + 3 + creds.find(':')?)
}

fn find_emails(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    for (at, _) in line.match_indices('@') {
        let Some(start) = local_part_start(line, at) else {
            continue;
        };
        let Some(end) = domain_end(line, at) else {
            continue;
        };
        out.push((start, &line[start..end]));
    }
    out
}

fn local_part_start(line: &str, at: usize) -> Option<usize> {
    if at == 0 {
        return None;
    }
    let bytes = line.as_bytes();
    let mut i = at;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-' | b'_') {
            i -= 1;
        } else {
            break;
        }
    }
    (i < at && bytes[i].is_ascii_alphanumeric()).then_some(i)
}

fn domain_end(line: &str, at: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if at + 2 >= bytes.len() {
        return None;
    }
    let mut j = at + 1;
    while j < bytes.len() {
        let b = bytes[j];
        if b.is_ascii_alphanumeric() || b == b'.' || b == b'-' {
            j += 1;
        } else {
            break;
        }
    }
    if j <= at + 1 || !bytes[at + 1..j].contains(&b'.') {
        return None;
    }
    Some(j)
}

fn find_private_ipv4(line: &str) -> Vec<(usize, ())> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            if let Some((start, end, a, b, c, d)) = parse_ipv4(bytes, i) {
                if is_private_ipv4(a, b, c, d) {
                    out.push((start, ()));
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn parse_ipv4(bytes: &[u8], start: usize) -> Option<(usize, usize, u8, u8, u8, u8)> {
    let mut parts = [0u8; 4];
    let mut idx = 0;
    let mut i = start;
    let mut part_start = start;
    while i <= bytes.len() && idx < 4 {
        if i == bytes.len() || bytes[i] == b'.' {
            if i == part_start || idx == 4 {
                return None;
            }
            let n: u16 = std::str::from_utf8(&bytes[part_start..i]).ok()?.parse().ok()?;
            if n > 255 {
                return None;
            }
            parts[idx] = n as u8;
            idx += 1;
            if i == bytes.len() {
                break;
            }
            part_start = i + 1;
        } else if !bytes[i].is_ascii_digit() {
            return None;
        }
        i += 1;
    }
    (idx == 4).then_some((start, i, parts[0], parts[1], parts[2], parts[3]))
}

fn is_private_ipv4(a: u8, b: u8, _c: u8, _d: u8) -> bool {
    a == 10
        || a == 127
        || (a == 192 && b == 168)
        || (a == 172 && (16..=31).contains(&b))
}

fn contains_subsequence(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn find_ascii(hay: &[u8], needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    hay.windows(needle.len())
        .position(|w| w == needle)
}
