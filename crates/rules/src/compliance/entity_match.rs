use pcbcraft_doc::ComplianceListSnapshot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityMatchConfidence {
    Exact,
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
            return Some(entity_match(entry));
        }
        for alias in &entry.aliases {
            if normalize_compliance_name(alias) == normalized {
                return Some(entity_match(entry));
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

fn entity_match(entry: &pcbcraft_doc::ComplianceListEntityEntry) -> EntityMatch {
    EntityMatch {
        entry_name: entry.name.clone(),
        entry_type: entry.entry_type.clone(),
        added_date: entry.added_date.clone(),
        confidence: EntityMatchConfidence::Exact,
    }
}
