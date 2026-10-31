use pcbcraft_publication::{stage0_from_zip, PublicationManifest};

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn bytes_from_seed(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let word = xorshift64(&mut state);
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.truncate(len);
    out
}

#[test]
fn stage0_never_panics_on_random_zip_bytes() {
    for seed in 0..512u64 {
        let input = bytes_from_seed(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), 256 + (seed as usize % 4096));
        let _ = stage0_from_zip(&input);
    }
}

#[test]
fn manifest_parse_never_panics_on_random_json() {
    for seed in 0..512u64 {
        let input = bytes_from_seed(seed.wrapping_mul(0xD1B5_4A32_D192_ED03), 128 + (seed as usize % 2048));
        if let Ok(text) = std::str::from_utf8(&input) {
            let _ = PublicationManifest::from_json(text);
        }
    }
}
