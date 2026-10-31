use pcbcraft_library::listing::{Unit, parse_tolerance, parse_value};
use pcbcraft_library::part_params::{
    ParamConfidence, ParamKey, ParamPayload, load_param_aliases, merge_part_params,
    params_from_attributes, parse_temperature_coefficient,
};
use pcbcraft_library::sources::{FixtureSource, PartResult, PartSource, SourceError};

const SAMPLE: &str = include_str!("fixtures/sources/sample.json");
const ATTRIBUTES: &str = include_str!("fixtures/sources/sample-attributes.json");
const ALIASES: &str = include_str!("fixtures/sources/sample-param-aliases.json");

fn source() -> Result<FixtureSource, SourceError> {
    FixtureSource::from_json(SAMPLE)?.with_attributes(ATTRIBUTES)
}

fn aliases() -> Result<pcbcraft_library::part_params::ParamAliasFile, String> {
    load_param_aliases(ALIASES)
}

fn params_for(
    src: &FixtureSource,
    part: &PartResult,
) -> Result<pcbcraft_library::part_params::PartParams, SourceError> {
    let attrs = src.attributes(&part.mpn);
    let cat = attrs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Category"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    Ok(params_from_attributes(
        &part.source,
        cat,
        &part.kind,
        &attrs,
        &aliases().map_err(|e| SourceError::Invalid(e))?,
        part.retrieved_at,
    ))
}

fn alt_result(mpn: &str, kind: &str) -> PartResult {
    PartResult {
        source: "Sample parts".into(),
        mpn: mpn.into(),
        manufacturer: "Example Components".into(),
        description: String::new(),
        kind: kind.into(),
        distributor: String::new(),
        sku: String::new(),
        stock: 0,
        price_breaks: vec![],
        lifecycle: String::new(),
        datasheet_url: String::new(),
        product_url: String::new(),
        retrieved_at: 1_790_000_000,
    }
}

fn number(params: &pcbcraft_library::part_params::PartParams, key: ParamKey) -> Option<f64> {
    params.params.iter().find(|p| p.key == key).and_then(|p| {
        if let ParamPayload::Number { value } = p.payload {
            Some(value)
        } else {
            None
        }
    })
}

#[test]
fn resistor_from_two_sources_agrees() -> Result<(), SourceError> {
    let src = source()?;
    let a = params_for(&src, &src.fetch("SAMPLE-R-10K")?)?;
    let b = params_for(&src, &alt_result("ALT-R-10K", "resistor"))?;
    let merged = merge_part_params(a, b);
    assert_eq!(number(&merged, ParamKey::Resistance), Some(10_000.0));
    assert!(merged.params.iter().all(|p| {
        p.confidence == ParamConfidence::Agreed || p.confidence == ParamConfidence::Single
    }));
    assert!(
        merged
            .params
            .iter()
            .any(|p| p.key == ParamKey::Resistance && p.confidence == ParamConfidence::Agreed)
    );
    Ok(())
}

#[test]
fn capacitor_from_two_sources_agrees() -> Result<(), SourceError> {
    let src = source()?;
    let a = params_for(&src, &src.fetch("SAMPLE-C-100N")?)?;
    let b = params_for(&src, &alt_result("ALT-C-100N", "capacitor"))?;
    let merged = merge_part_params(a, b);
    assert_eq!(number(&merged, ParamKey::Capacitance), Some(1.0e-7));
    assert!(
        merged
            .params
            .iter()
            .any(|p| p.key == ParamKey::Capacitance && p.confidence == ParamConfidence::Agreed)
    );
    Ok(())
}

#[test]
fn inductor_from_two_sources_agrees() -> Result<(), SourceError> {
    let src = source()?;
    let a = params_for(&src, &src.fetch("SAMPLE-L-10U")?)?;
    let b = params_for(&src, &alt_result("ALT-L-10U", "inductor"))?;
    let merged = merge_part_params(a, b);
    assert_eq!(number(&merged, ParamKey::Inductance), Some(1.0e-5));
    assert_eq!(number(&merged, ParamKey::RatedCurrent), Some(1.2));
    Ok(())
}

#[test]
fn deliberate_resistance_mismatch_is_conflict() -> Result<(), SourceError> {
    let src = source()?;
    let a = params_for(&src, &src.fetch("SAMPLE-R-10K")?)?;
    let b = params_for(&src, &alt_result("MISMATCH-R-10K", "resistor"))?;
    let merged = merge_part_params(a, b);
    let resistance = merged.params.iter().find(|p| p.key == ParamKey::Resistance);
    assert_eq!(
        resistance.map(|p| p.confidence),
        Some(ParamConfidence::Conflict)
    );
    assert_eq!(resistance.map(|p| p.conflicts.len()), Some(1));
    assert_eq!(number(&merged, ParamKey::Resistance), Some(10_000.0));
    Ok(())
}

#[test]
fn unknown_attributes_stay_as_extras() -> Result<(), SourceError> {
    let src = source()?;
    let p = params_for(&src, &src.fetch("SAMPLE-R-10K")?)?;
    assert!(p.extras.iter().any(|e| e.name == "RoHS"));
    Ok(())
}

#[test]
fn value_and_tolerance_parsers_by_table() {
    assert_eq!(parse_value("10 kohms", None), Some((10_000.0, Unit::Ohm)));
    assert_eq!(parse_tolerance("+80%, -20%"), Some((20.0, 80.0)));
    assert_eq!(parse_temperature_coefficient("±30 ppm/°C"), Some(30.0));
}

#[test]
fn bare_category_leaves_params_empty() -> Result<(), SourceError> {
    let src = source()?;
    let attrs = src.attributes("BARE-PART");
    let out = params_from_attributes(
        "Sample parts",
        "Mystery parts",
        "",
        &attrs,
        &aliases().map_err(|e| SourceError::Invalid(e))?,
        0,
    );
    assert!(out.params.is_empty());
    Ok(())
}
