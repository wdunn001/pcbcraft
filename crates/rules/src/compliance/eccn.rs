//! ECCN ordering for "highest declared" summaries (informational only).

/// Higher means more export control concern for summary ordering.
pub fn control_rank(eccn: &str) -> Option<u32> {
    let t = eccn.trim();
    if t.is_empty() {
        return None;
    }
    let upper = t.to_ascii_uppercase();
    if upper == "UNKNOWN" || upper == "N/A" {
        return None;
    }
    if upper == "EAR99" {
        return Some(0);
    }
    let mut rank = 0u32;
    let mut chars = upper.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let Some(d) = c.to_digit(10) else {
                return None;
            };
            rank = rank.saturating_mul(10).saturating_add(d);
        } else if c.is_ascii_alphabetic() {
            rank = rank.saturating_mul(100).saturating_add(u32::from(c as u8));
            while chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                let next = chars.next();
                let Some(ch) = next else {
                    break;
                };
                let Some(d) = ch.to_digit(10) else {
                    return None;
                };
                rank = rank.saturating_mul(10).saturating_add(d);
            }
            break;
        } else if c == '.' || c == '-' {
            continue;
        } else {
            break;
        }
    }
    if rank == 0 {
        return Some(1);
    }
    Some(rank.saturating_add(10))
}

pub fn highest_eccn<'a>(codes: impl IntoIterator<Item = &'a str>) -> (Option<String>, usize) {
    let mut best: Option<(u32, String)> = None;
    let mut unknown = 0usize;
    for code in codes {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            unknown += 1;
            continue;
        }
        let Some(rank) = control_rank(trimmed) else {
            unknown += 1;
            continue;
        };
        let replace = best.as_ref().is_none_or(|(r, _)| rank > *r);
        if replace {
            best = Some((rank, trimmed.to_string()));
        }
    }
    (best.map(|(_, s)| s), unknown)
}
