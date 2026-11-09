//! A small JSON Schema checker for `site/src/data/status.schema.json`.
//!
//! Context: `cargo xtask status-data` validates its own output before writing it, and
//! `--check` validates a file already on disk. xtask takes no schema crate, so this
//! file implements only the keywords the status schema uses.
//!
//! # Rules
//! - Supported keywords: `type` (name or list), `enum`, `const`, `required`,
//!   `properties`, `additionalProperties: false`, `items`, `minimum`, `oneOf`, `$ref`
//!   (local `#/$defs/name` only) and `$defs`.
//! - A keyword outside that list in the schema is an error, so the checker never
//!   passes a document on a rule it did not read.
//!
//! # Dependencies
//! - site/src/data/status.schema.json
//! - xtask/src/status_data.rs
//!
//! # Do not
//! - Grow this into a general validator. Add a keyword only when the schema needs it.
use serde_json::Value;

/// Keywords the checker understands. Anything else in the schema is rejected.
const KNOWN: [&str; 15] = [
    "type",
    "enum",
    "const",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "minimum",
    "oneOf",
    "$ref",
    "$defs",
    "$schema",
    "$id",
    "title",
    "description",
];

/// Checks `doc` against `schema` and returns every problem as `path: message`.
///
/// # Rules
/// - An empty list means the document is valid.
/// - Paths look like `$.features[3].state`.
pub fn validate(schema: &Value, doc: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    check(schema, schema, doc, "$", &mut problems);
    problems
}

/// Resolves a local `#/$defs/name` reference.
fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let name = reference.strip_prefix("#/$defs/")?;
    root.get("$defs")?.get(name)
}

/// True when the value has the named JSON type.
fn has_type(value: &Value, name: &str) -> bool {
    match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        _ => false,
    }
}

/// Recursive worker behind [`validate`].
fn check(root: &Value, schema: &Value, doc: &Value, path: &str, out: &mut Vec<String>) {
    let Some(map) = schema.as_object() else {
        out.push(format!("{path}: schema node is not an object"));
        return;
    };
    for key in map.keys() {
        if !KNOWN.contains(&key.as_str()) {
            out.push(format!("{path}: schema keyword `{key}` is not supported"));
        }
    }
    if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
        match resolve(root, reference) {
            Some(target) => check(root, target, doc, path, out),
            None => out.push(format!("{path}: unresolved reference {reference}")),
        }
    }
    if let Some(expected) = map.get("type") {
        let names: Vec<&str> = match expected {
            Value::String(s) => vec![s.as_str()],
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !names.iter().any(|n| has_type(doc, n)) {
            out.push(format!("{path}: expected {}", names.join(" or ")));
            return;
        }
    }
    if let Some(list) = map.get("enum").and_then(Value::as_array)
        && !list.contains(doc)
    {
        out.push(format!("{path}: value {doc} is not allowed"));
    }
    if let Some(expected) = map.get("const")
        && expected != doc
    {
        out.push(format!("{path}: expected the constant {expected}"));
    }
    if let (Some(min), Some(n)) = (map.get("minimum").and_then(Value::as_f64), doc.as_f64())
        && n < min
    {
        out.push(format!("{path}: {n} is below {min}"));
    }
    if let Some(options) = map.get("oneOf").and_then(Value::as_array) {
        let matching = options
            .iter()
            .filter(|option| {
                let mut scratch = Vec::new();
                check(root, option, doc, path, &mut scratch);
                scratch.is_empty()
            })
            .count();
        if matching != 1 {
            out.push(format!("{path}: matches {matching} of the oneOf options"));
        }
    }
    if let Some(object) = doc.as_object() {
        if let Some(required) = map.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    out.push(format!("{path}: missing `{name}`"));
                }
            }
        }
        let properties = map.get("properties").and_then(Value::as_object);
        for (key, value) in object {
            let child = format!("{path}.{key}");
            match properties.and_then(|p| p.get(key)) {
                Some(sub) => check(root, sub, value, &child, out),
                None if map.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    out.push(format!("{child}: property is not in the schema"));
                }
                None => {}
            }
        }
    }
    if let (Some(items), Some(list)) = (map.get("items"), doc.as_array()) {
        for (i, item) in list.iter().enumerate() {
            check(root, items, item, &format!("{path}[{i}]"), out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_missing_required_property_and_a_stray_one_are_both_reported() {
        let schema = json!({"type":"object","required":["a"],"additionalProperties":false,"properties":{"a":{"type":"integer"}}});
        let problems = validate(&schema, &json!({"b": 1}));
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn refs_enums_and_one_of_are_followed() {
        let schema = json!({"$defs":{"s":{"enum":["x","y"]}},"type":"array","items":{"oneOf":[{"type":"null"},{"$ref":"#/$defs/s"}]}});
        assert!(validate(&schema, &json!([null, "x"])).is_empty());
        assert_eq!(validate(&schema, &json!(["z"])).len(), 1);
    }

    #[test]
    fn an_unsupported_keyword_is_an_error_not_a_pass() {
        let schema = json!({"type":"string","pattern":"^a"});
        assert!(!validate(&schema, &json!("b")).is_empty());
    }
}
