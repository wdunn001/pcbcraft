use pcbcraft_calc::{term_by_id, CALCULATORS, TERMS};

#[test]
fn every_term_has_required_fields() {
    for t in TERMS {
        assert!(!t.id.is_empty());
        assert!(!t.term.is_empty());
        assert!(!t.category.is_empty());
        assert!(!t.short.is_empty());
        assert!(!t.long.is_empty());
        assert!(!t.search.is_empty());
        assert!(!t.source.is_empty());
    }
}

#[test]
fn related_ids_resolve() {
    for t in TERMS {
        for rid in t.related {
            assert!(
                term_by_id(rid).is_some(),
                "term {} related id {rid} missing",
                t.id
            );
        }
    }
}

#[test]
fn calculator_ids_resolve_when_present() {
    let calc_ids: std::collections::HashSet<&str> =
        CALCULATORS.iter().map(|c| c.id).collect();
    for t in TERMS {
        if !t.calculator.is_empty() {
            assert!(
                calc_ids.contains(t.calculator),
                "term {} calculator {} missing",
                t.id,
                t.calculator
            );
        }
    }
}
