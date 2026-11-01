use pcbcraft_doc::ComplianceListSnapshot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityMatchConfidence {
    Exact,
    Possible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityMatch {
    pub entry_name: String,
    pub entry_type: String,
    pub added_date: String,
    pub confidence: EntityMatchConfidence,
}

pub fn normalize_compliance_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_space = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_owned()
}

pub fn match_entity_name(
    snapshot: &ComplianceListSnapshot,
    query: &str,
    entry_type: Option<&str>,
) -> Option<EntityMatch> {
    let normalized = normalize_compliance_name(query);
    if normalized.is_empty() {
        return None;
    }
    for entry in &snapshot.entities {
        if !entry_type_matches(entry_type, &entry.entry_type) {
            continue;
        }
        if normalize_compliance_name(&entry.name) == normalized {
            return Some(entity_match(entry, EntityMatchConfidence::Exact));
        }
        for alias in &entry.aliases {
            if normalize_compliance_name(alias) == normalized {
                return Some(entity_match(entry, EntityMatchConfidence::Exact));
            }
        }
    }
    for entry in &snapshot.entities {
        if !entry_type_matches(entry_type, &entry.entry_type) {
            continue;
        }
        if near_name_match(&normalized, &normalize_compliance_name(&entry.name)) {
            return Some(entity_match(entry, EntityMatchConfidence::Possible));
        }
        for alias in &entry.aliases {
            if near_name_match(&normalized, &normalize_compliance_name(alias)) {
                return Some(entity_match(entry, EntityMatchConfidence::Possible));
            }
        }
    }
    None
}

fn entry_type_matches(filter: Option<&str>, entry_type: &str) -> bool {
    match filter {
        None | Some("") => true,
        Some(want) => entry_type.eq_ignore_ascii_case(want),
    }
}

fn entity_match(
    entry: &pcbcraft_doc::ComplianceListEntityEntry,
    confidence: EntityMatchConfidence,
) -> EntityMatch {
    EntityMatch {
        entry_name: entry.name.clone(),
        entry_type: entry.entry_type.clone(),
        added_date: entry.added_date.clone(),
        confidence,
    }
}

fn near_name_match(a: &str, b: &str) -> bool {
    if a == b || a.is_empty() || b.is_empty() {
        return false;
    }
    let max_len = a.len().max(b.len());
    if max_len < 4 {
        return false;
    }
    let distance = levenshtein(a.as_bytes(), b.as_bytes());
    distance <= 2 && distance * 4 <= max_len
}

fn levenshtein(a: &[u8], b: &[u8]) -> usize {
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0; m + 1];
    for (i, &byte_a) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &byte_b) in b.iter().enumerate() {
            let cost = if byte_a == byte_b { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}
