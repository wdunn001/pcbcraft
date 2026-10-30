//! Connector manufacturer part-number build and parse from embedded series tables.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const SEP: char = '-';

/// Bounds checked when reading a part number string or field token.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesLimits {
    pub max_input_len: usize,
    pub min_positions_per_row: u32,
    pub max_positions_per_row: u32,
}

/// One selectable code within an enum field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldValue {
    pub code: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaning: Option<toml::Value>,
}

/// How one hyphen-separated segment is encoded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldKind {
    Literal {
        text: String,
    },
    PitchPositions,
    Enum {
        #[serde(default)]
        values: Vec<FieldValue>,
    },
    OptionalEnum {
        #[serde(default)]
        values: Vec<FieldValue>,
    },
}

/// Ordered segment of a series part number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    pub id: String,
    pub label: String,
    #[serde(flatten)]
    pub kind: FieldKind,
}

/// Cross-field rule applied after segments are chosen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Constraint {
    EvenTotalPositions {
        row_field: String,
        positions_field: String,
    },
}

/// Data file describing one connector series part-number layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesSpec {
    pub id: String,
    pub prefix: String,
    pub manufacturer: String,
    pub description: String,
    pub limits: SeriesLimits,
    pub fields: Vec<FieldSpec>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
}

/// Canonical part number string.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PartNumber(pub String);

/// Field values extracted from a part number or chosen for a build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParsedPart {
    pub series_id: String,
    pub values: BTreeMap<String, String>,
    pub text: String,
    pub complete: bool,
}

/// Options keyed by [`FieldSpec::id`] for [`build`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BuildOptions(pub BTreeMap<String, String>);

/// Validation failure for one field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
    pub valid_values: Vec<String>,
}

impl FieldError {
    fn unknown_field(field: &str, valid: Vec<String>) -> Self {
        Self {
            field: field.into(),
            message: format!("unknown value for {field}"),
            valid_values: valid,
        }
    }
}

/// Embedded first-slice series tables (TSW, SSW, BTH).
pub fn first_slice_specs() -> Vec<SeriesSpec> {
    vec![
        load_embedded_series(include_str!("../data/connectors/tsw.toml")),
        load_embedded_series(include_str!("../data/connectors/ssw.toml")),
        load_embedded_series(include_str!("../data/connectors/bth.toml")),
    ]
}

pub fn load_embedded_series(text: &str) -> SeriesSpec {
    toml::from_str(text).unwrap_or_else(|e| panic!("embedded series table: {e}"))
}

/// Build a part number from a series spec and per-field option codes.
pub fn build(spec: &SeriesSpec, options: &BuildOptions) -> Result<PartNumber, Vec<FieldError>> {
    let mut values = BTreeMap::new();
    let mut errors = Vec::new();
    for field in &spec.fields {
        match &field.kind {
            FieldKind::Literal { text } => {
                values.insert(field.id.clone(), text.clone());
            }
            FieldKind::PitchPositions | FieldKind::Enum { .. } => {
                let Some(raw) = options.0.get(&field.id) else {
                    errors.push(FieldError {
                        field: field.id.clone(),
                        message: format!("missing value for {}", field.label),
                        valid_values: valid_codes(&field.kind),
                    });
                    continue;
                };
                if let Err(e) = validate_field_token(spec, field, raw) {
                    errors.push(e);
                } else {
                    values.insert(field.id.clone(), raw.clone());
                }
            }
            FieldKind::OptionalEnum { .. } => {
                if let Some(raw) = options.0.get(&field.id) {
                    if let Err(e) = validate_field_token(spec, field, raw) {
                        errors.push(e);
                    } else {
                        values.insert(field.id.clone(), raw.clone());
                    }
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if let Err(e) = check_constraints(spec, &values) {
        errors.push(e);
        return Err(errors);
    }
    Ok(PartNumber(assemble(spec, &values)))
}

/// Parse a part number against a set of series specs.
pub fn parse(spec_set: &[SeriesSpec], text: &str) -> Result<ParsedPart, Vec<FieldError>> {
    let (part, errors) = parse_partial(spec_set, text);
    if errors.is_empty() {
        Ok(part)
    } else {
        Err(errors)
    }
}

/// Parse through the first failing segment, returning fields consumed so far.
pub fn parse_partial(spec_set: &[SeriesSpec], text: &str) -> (ParsedPart, Vec<FieldError>) {
    let trimmed = text.trim();
    let max_len = spec_set
        .iter()
        .map(|s| s.limits.max_input_len)
        .max()
        .unwrap_or(64);
    if trimmed.len() > max_len {
        let field = "part_number".into();
        return (
            ParsedPart {
                series_id: String::new(),
                values: BTreeMap::new(),
                text: trimmed.to_owned(),
                complete: false,
            },
            vec![FieldError {
                field,
                message: "part number is too long".into(),
                valid_values: Vec::new(),
            }],
        );
    }
    let Some(spec) = match_series(spec_set, trimmed) else {
        return (
            ParsedPart {
                series_id: String::new(),
                values: BTreeMap::new(),
                text: trimmed.to_owned(),
                complete: false,
            },
            vec![FieldError {
                field: "series".into(),
                message: "unknown connector series prefix".into(),
                valid_values: spec_set.iter().map(|s| s.prefix.clone()).collect(),
            }],
        );
    };
    let mut rest = trimmed;
    let mut values = BTreeMap::new();
    let mut errors = Vec::new();
    for field in &spec.fields {
        if rest.is_empty() {
            errors.push(FieldError {
                field: field.id.clone(),
                message: format!("expected {}", field.label),
                valid_values: valid_codes(&field.kind),
            });
            break;
        }
        match &field.kind {
            FieldKind::Literal { text } => {
                if !rest.starts_with(text) {
                    errors.push(FieldError::unknown_field(
                        &field.id,
                        vec![text.clone()],
                    ));
                    break;
                }
                values.insert(field.id.clone(), text.clone());
                rest = strip_prefix_or_empty(rest, text);
                rest = strip_sep(rest);
            }
            FieldKind::PitchPositions => {
                let (token, next) = match next_token(rest) {
                    Some(v) => v,
                    None => {
                        errors.push(FieldError::unknown_field(
                            &field.id,
                            vec!["NNN".into()],
                        ));
                        break;
                    }
                };
                if let Err(e) = validate_pitch_positions(spec, &token) {
                    errors.push(e);
                    break;
                }
                values.insert(field.id.clone(), token);
                rest = next;
            }
            FieldKind::Enum { .. } => {
                let (token, next) = match next_token(rest) {
                    Some(v) => v,
                    None => {
                        errors.push(FieldError::unknown_field(
                            &field.id,
                            valid_codes(&field.kind),
                        ));
                        break;
                    }
                };
                if let Err(e) = validate_field_token(spec, field, &token) {
                    errors.push(e);
                    break;
                }
                values.insert(field.id.clone(), token);
                rest = next;
            }
            FieldKind::OptionalEnum { .. } => {
                if let Some((token, next)) = next_token(rest) {
                    if validate_field_token(spec, field, &token).is_ok() {
                        values.insert(field.id.clone(), token);
                        rest = next;
                        continue;
                    }
                }
            }
        }
    }
    if errors.is_empty() {
        if !rest.is_empty() {
            errors.push(FieldError {
                field: "suffix".into(),
                message: "unexpected trailing text".into(),
                valid_values: Vec::new(),
            });
        } else if let Err(e) = check_constraints(spec, &values) {
            errors.push(e);
        }
    }
    let complete = errors.is_empty();
    (
        ParsedPart {
            series_id: spec.id.clone(),
            values,
            text: trimmed.to_owned(),
            complete,
        },
        errors,
    )
}

fn match_series<'a>(spec_set: &'a [SeriesSpec], text: &str) -> Option<&'a SeriesSpec> {
    spec_set
        .iter()
        .filter(|s| text.starts_with(&s.prefix))
        .max_by_key(|s| s.prefix.len())
}

fn assemble(spec: &SeriesSpec, values: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for field in &spec.fields {
        let Some(token) = values.get(&field.id) else {
            continue;
        };
        if !out.is_empty() {
            out.push(SEP);
        }
        out.push_str(token);
    }
    out
}

fn valid_codes(kind: &FieldKind) -> Vec<String> {
    match kind {
        FieldKind::Literal { text } => vec![text.clone()],
        FieldKind::PitchPositions => vec!["1NN".into(), "2NN".into()],
        FieldKind::Enum { values } | FieldKind::OptionalEnum { values } => {
            values.iter().map(|v| v.code.clone()).collect()
        }
    }
}

fn validate_field_token(
    spec: &SeriesSpec,
    field: &FieldSpec,
    raw: &str,
) -> Result<(), FieldError> {
    if raw.is_empty() || raw.len() > 8 {
        return Err(FieldError::unknown_field(&field.id, valid_codes(&field.kind)));
    }
    if raw.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '_')) {
        return Err(FieldError::unknown_field(&field.id, valid_codes(&field.kind)));
    }
    match &field.kind {
        FieldKind::Literal { text } => {
            if raw == text {
                Ok(())
            } else {
                Err(FieldError::unknown_field(&field.id, vec![text.clone()]))
            }
        }
        FieldKind::PitchPositions => validate_pitch_positions(spec, raw),
        FieldKind::Enum { values } | FieldKind::OptionalEnum { values } => {
            if values.iter().any(|v| v.code == raw) {
                Ok(())
            } else {
                Err(FieldError::unknown_field(
                    &field.id,
                    values.iter().map(|v| v.code.clone()).collect(),
                ))
            }
        }
    }
}

fn validate_pitch_positions(spec: &SeriesSpec, raw: &str) -> Result<(), FieldError> {
    if raw.len() != 3 || !raw.chars().all(|c| c.is_ascii_digit()) {
        return Err(FieldError::unknown_field(
            "pitch_positions",
            vec!["three digits".into()],
        ));
    }
    let pitch = raw.as_bytes()[0] - b'0';
    if pitch != 1 && pitch != 2 {
        return Err(FieldError::unknown_field(
            "pitch_positions",
            vec!["1NN".into(), "2NN".into()],
        ));
    }
    let positions: u32 = raw[1..].parse().unwrap_or(0);
    if positions < spec.limits.min_positions_per_row
        || positions > spec.limits.max_positions_per_row
    {
        return Err(FieldError::unknown_field(
            "pitch_positions",
            vec![format!(
                "{}..={}",
                spec.limits.min_positions_per_row, spec.limits.max_positions_per_row
            )],
        ));
    }
    Ok(())
}

fn check_constraints(
    spec: &SeriesSpec,
    values: &BTreeMap<String, String>,
) -> Result<(), FieldError> {
    for constraint in &spec.constraints {
        if let Constraint::EvenTotalPositions {
            row_field,
            positions_field,
        } = constraint
        {
            let rows = row_count(values.get(row_field.as_str()));
            let per_row = positions_per_row(values.get(positions_field.as_str()));
            if rows > 1 && (rows * per_row) % 2 != 0 {
                return Err(FieldError {
                    field: positions_field.clone(),
                    message: "total positions must be even for multi-row parts".into(),
                    valid_values: Vec::new(),
                });
            }
        }
    }
    Ok(())
}

fn row_count(code: Option<&String>) -> u32 {
    let Some(code) = code else {
        return 1;
    };
    match code.as_str() {
        "D" | "Q" => 2,
        "T" => 3,
        _ => 1,
    }
}

fn positions_per_row(code: Option<&String>) -> u32 {
    let Some(code) = code else {
        return 0;
    };
    if code.len() == 3 && code.chars().all(|c| c.is_ascii_digit()) {
        code[1..].parse().unwrap_or(0)
    } else {
        code.parse().unwrap_or(0)
    }
}

fn next_token(input: &str) -> Option<(String, &str)> {
    let mut start = 0usize;
    if let Some(stripped) = input.strip_prefix(SEP) {
        start = input.len() - stripped.len();
    }
    let tail = &input[start..];
    if tail.is_empty() {
        return None;
    }
    let end = tail
        .find(SEP)
        .map(|i| start + i)
        .unwrap_or(input.len());
    let token = input[start..end].trim();
    if token.is_empty() {
        return None;
    }
    let next = if end < input.len() {
        &input[end + 1..]
    } else {
        ""
    };
    Some((token.to_owned(), next))
}

fn strip_prefix_or_empty<'a>(input: &'a str, prefix: &str) -> &'a str {
    input.strip_prefix(prefix).unwrap_or("")
}

fn strip_sep(input: &str) -> &str {
    input.strip_prefix(SEP).unwrap_or(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
    }

    fn specs() -> Vec<SeriesSpec> {
        first_slice_specs()
    }

    fn spec(id: &str) -> SeriesSpec {
        specs()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("missing series {id}"))
    }

    #[test]
    fn manufacturer_page_examples_round_trip() {
        let examples = [
            ("tsw", "TSW-108-07-L-S"),
            ("tsw", "TSW-103-07-L-S"),
            ("tsw", "TSW-110-07-G-S"),
            ("tsw", "TSW-101-14-F-S"),
            ("tsw", "TSW-101-14-L-S"),
            ("ssw", "SSW-108-01-L-D"),
            ("ssw", "SSW-108-01-F-D"),
            ("bth", "BTH-060-01-F-D-A"),
            ("bth", "BTH-060-01-L-D-A-K-TR"),
            ("bth", "BTH-030-01-F-D-A"),
            ("bth", "BTH-120-01-F-D-A"),
            ("bth", "BTH-030-01-F-D-RA-WT-K"),
        ];
        let set = specs();
        for (series_id, text) in examples {
            let parsed = parse(&set, text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
            assert_eq!(parsed.series_id, series_id);
            let built = build(&spec(series_id), &BuildOptions(parsed.values)).unwrap();
            assert_eq!(built.0, text, "{text}");
        }
    }

    #[test]
    fn invalid_lead_plating_and_positions_are_rejected() {
        let set = specs();
        let tsw = spec("tsw");
        let bad_lead = build(
            &tsw,
            &BuildOptions(BTreeMap::from([
                ("pitch_positions".into(), "108".into()),
                ("lead".into(), "99".into()),
                ("plating".into(), "L".into()),
                ("row".into(), "S".into()),
            ])),
        )
        .unwrap_err();
        assert_eq!(bad_lead[0].field, "lead");
        assert!(!bad_lead[0].valid_values.is_empty());

        let bad_plating = build(
            &tsw,
            &BuildOptions(BTreeMap::from([
                ("pitch_positions".into(), "108".into()),
                ("lead".into(), "07".into()),
                ("plating".into(), "Z".into()),
                ("row".into(), "S".into()),
            ])),
        )
        .unwrap_err();
        assert_eq!(bad_plating[0].field, "plating");

        let bad_positions = build(
            &tsw,
            &BuildOptions(BTreeMap::from([
                ("pitch_positions".into(), "199".into()),
                ("lead".into(), "07".into()),
                ("plating".into(), "L".into()),
                ("row".into(), "S".into()),
            ])),
        )
        .unwrap_err();
        assert_eq!(bad_positions[0].field, "pitch_positions");
    }

    #[test]
    fn parse_then_build_is_identity_for_random_valid_configs() {
        let set = specs();
        let mut rng = Lcg(779);
        for _ in 0..1000 {
            let series_idx = (rng.next() % 3) as usize;
            let spec = &set[series_idx];
            let options = random_valid_options(spec, &mut rng);
            let built = build(spec, &options).expect("random build");
            let parsed = parse(&set, &built.0).expect("random parse");
            assert_eq!(parsed.values, options.0);
            let rebuilt = build(spec, &BuildOptions(parsed.values)).expect("rebuild");
            assert_eq!(rebuilt.0, built.0);
        }
    }

    #[test]
    fn partial_string_reports_first_failing_field() {
        let set = specs();
        let (part, errors) = parse_partial(&set, "TSW-108-99");
        assert!(!part.values.contains_key("plating"));
        assert_eq!(errors[0].field, "lead");
        assert!(!errors[0].valid_values.is_empty());
    }

    #[test]
    fn a_newcomer_enters_a_wrong_option_and_is_told_which_field_is_invalid() {
        let tsw = spec("tsw");
        let errors = build(
            &tsw,
            &BuildOptions(BTreeMap::from([
                ("pitch_positions".into(), "108".into()),
                ("lead".into(), "07".into()),
                ("plating".into(), "Z".into()),
                ("row".into(), "S".into()),
            ])),
        )
        .unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "plating");
        assert!(errors[0]
            .valid_values
            .iter()
            .any(|v| v == "L" || v == "F" || v == "G"));
    }

    fn random_valid_options(spec: &SeriesSpec, rng: &mut Lcg) -> BuildOptions {
        let mut map = BTreeMap::new();
        for field in &spec.fields {
            match &field.kind {
                FieldKind::Literal { .. } => {}
                FieldKind::PitchPositions => {
                    let positions = (rng.next() % spec.limits.max_positions_per_row) + 1;
                    let pitch = if rng.next() % 2 == 0 { 1 } else { 2 };
                    map.insert(
                        field.id.clone(),
                        format!("{pitch}{positions:02}"),
                    );
                }
                FieldKind::Enum { values } => {
                    let idx = (rng.next() as usize) % values.len();
                    map.insert(field.id.clone(), values[idx].code.clone());
                }
                FieldKind::OptionalEnum { values } => {
                    if rng.next() % 3 == 0 {
                        let idx = (rng.next() as usize) % values.len();
                        map.insert(field.id.clone(), values[idx].code.clone());
                    }
                }
            }
        }
        // Retry until constraints pass.
        for _ in 0..32 {
            if build(spec, &BuildOptions(map.clone())).is_ok() {
                return BuildOptions(map);
            }
            if let Some(row) = map.get_mut("row") {
                *row = if *row == "S" { "D".into() } else { "S".into() };
            }
        }
        BuildOptions(map)
    }
}
