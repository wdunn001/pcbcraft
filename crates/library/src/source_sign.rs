//! Request signing helpers for vendor APIs that require HMAC or digest auth.

use hmac::{Hmac, Mac};
use sha1::{Digest, Sha1};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// LCSC agent API signature: SHA-1 of `key=...&nonce=...&secret=...&timestamp=...`.
pub fn lcsc_signature(key: &str, secret: &str, nonce: &str, timestamp: &str) -> String {
    let material = format!("key={key}&nonce={nonce}&secret={secret}&timestamp={timestamp}");
    let digest = Sha1::digest(material.as_bytes());
    hex_lower(&digest)
}

/// JLCPCB Open Platform `JOP` authorization header value.
pub fn jlcpcb_authorization(
    app_id: &str,
    access_key: &str,
    secret_key: &str,
    method: &str,
    url: &str,
    body: &str,
    nonce: &str,
    timestamp: u64,
) -> String {
    let canonical = canonical_uri(url);
    let string_to_sign = format!(
        "{}\n{}\n{}\n{}\n{}\n",
        method.to_uppercase(),
        canonical,
        timestamp,
        nonce,
        body
    );
    let Ok(mut mac) = HmacSha256::new_from_slice(secret_key.as_bytes()) else {
        return String::new();
    };
    mac.update(string_to_sign.as_bytes());
    let signature = base64_encode(mac.finalize().into_bytes());
    format!(
        "JOP appid=\"{app_id}\",accesskey=\"{access_key}\",timestamp=\"{timestamp}\",nonce=\"{nonce}\",signature=\"{signature}\""
    )
}

fn canonical_uri(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let path = without_scheme
        .find('/')
        .map(|i| &without_scheme[i..])
        .unwrap_or("/");
    path.to_owned()
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(hex_char(b >> 4));
        out.push(hex_char(b & 0xf));
    }
    out
}

fn hex_char(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + nibble - 10) as char,
        _ => '0',
    }
}

fn base64_encode(bytes: Vec<u8>) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push(TABLE[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}
