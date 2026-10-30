//! Glossary search over compiled [`TERMS`](crate::TERMS).

mod terms_generated {
    include!(concat!(env!("OUT_DIR"), "/terms.rs"));
}

pub use terms_generated::{Term, TERMS};

/// One glossary row ranked for a query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlossaryHit {
    pub term: &'static Term,
    /// Lower is better: exact term, exact synonym, prefix, then token tiers.
    pub rank: u8,
    /// Which field satisfied the match: term, synonym, prefix, search, short, long.
    pub matched_field: &'static str,
}

/// Lookup one term by stable id.
pub fn term_by_id(id: &str) -> Option<&'static Term> {
    TERMS.iter().find(|t| t.id == id)
}

fn tokens(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
        .collect()
}

fn all_tokens_in(haystack: &str, toks: &[String]) -> bool {
    let h = haystack.to_ascii_lowercase();
    toks.iter().all(|t| h.contains(t.as_str()))
}

fn rank_term(query: &str, term: &Term) -> Option<GlossaryHit> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let ql = q.to_ascii_lowercase();
    let tl = term.term.to_ascii_lowercase();
    if tl == ql {
        return Some(GlossaryHit {
            term,
            rank: 0,
            matched_field: "term",
        });
    }
    for syn in term.synonyms {
        if syn.to_ascii_lowercase() == ql {
            return Some(GlossaryHit {
                term,
                rank: 1,
                matched_field: "synonym",
            });
        }
    }
    if tl.starts_with(ql.as_str()) {
        return Some(GlossaryHit {
            term,
            rank: 2,
            matched_field: "prefix",
        });
    }
    for syn in term.synonyms {
        if syn.to_ascii_lowercase().starts_with(ql.as_str()) {
            return Some(GlossaryHit {
                term,
                rank: 3,
                matched_field: "prefix",
            });
        }
    }
    let toks = tokens(q);
    if toks.is_empty() {
        return None;
    }
    if all_tokens_in(term.search, &toks) {
        return Some(GlossaryHit {
            term,
            rank: 4,
            matched_field: "search",
        });
    }
    if all_tokens_in(term.short, &toks) {
        return Some(GlossaryHit {
            term,
            rank: 5,
            matched_field: "short",
        });
    }
    if all_tokens_in(term.long, &toks) {
        return Some(GlossaryHit {
            term,
            rank: 6,
            matched_field: "long",
        });
    }
    None
}

/// Search glossary entries; returns at most `limit` hits sorted by rank then term name.
pub fn search_glossary(query: &str, limit: usize) -> Vec<GlossaryHit> {
    if limit == 0 {
        return Vec::new();
    }
    let mut hits: Vec<GlossaryHit> = TERMS
        .iter()
        .filter_map(|t| rank_term(query, t))
        .collect();
    hits.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.term.term.cmp(b.term.term))
    });
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glossary_ranks_exact_term_before_prefix() {
        let hits = search_glossary("Voltage", 20);
        let first = hits.first().map(|h| h.term.id);
        assert_eq!(first, Some("voltage"));
        if hits.len() >= 2 {
            assert!(hits[0].rank <= hits[1].rank);
        }
    }

    #[test]
    fn glossary_synonym_matches_term() {
        let hits = search_glossary("trace width", 20);
        assert!(hits.iter().any(|h| h.term.id == "track_width"));
    }

    #[test]
    fn glossary_limit_truncates_results() {
        let all = search_glossary("a", 100);
        let few = search_glossary("a", 3);
        assert!(few.len() <= 3);
        if all.len() > 3 {
            assert_eq!(few.len(), 3);
        }
    }
}
