use pcbcraft_calc::dispatch::dispatch;
use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rel_close(got: f64, expect: f64, rel_tol: f64) -> Result<(), String> {
    if !got.is_finite() || !expect.is_finite() {
        return Err(format!("non-finite got {got} expect {expect}"));
    }
    let denom = expect.abs().max(1e-12);
    let rel = (got - expect).abs() / denom;
    if rel > rel_tol {
        Err(format!("got {got} expect {expect} rel {rel} > {rel_tol}"))
    } else {
        Ok(())
    }
}

#[test]
fn golden_vectors_match_ee_thesaurus() -> Result<(), String> {
    let path = manifest_dir().join("data/calculators.toml");
    let raw = fs::read_to_string(&path).map_err(|e| format!("read toml: {e}"))?;
    let doc: toml::Value = raw.parse().map_err(|e| format!("parse toml: {e}"))?;
    let calcs = doc
        .get("calculator")
        .and_then(toml::Value::as_array)
        .ok_or("missing calculator array")?;
    let mut total = 0usize;
    for calc in calcs {
        let id = calc
            .get("id")
            .and_then(toml::Value::as_str)
            .ok_or("calculator id")?;
        let vectors = calc
            .get("vector")
            .and_then(toml::Value::as_array)
            .ok_or("vector array")?;
        for vec in vectors {
            total += 1;
            let rel_tol = vec
                .get("rel_tol")
                .and_then(toml::Value::as_float)
                .unwrap_or(1e-6);
            let inputs = vec
                .get("inputs")
                .and_then(toml::Value::as_table)
                .ok_or("vector inputs")?;
            let expect = vec
                .get("expect")
                .and_then(toml::Value::as_table)
                .ok_or("vector expect")?;
            let out = dispatch(id, inputs).map_err(|e| format!("{id}: {e}"))?;
            for (key, val) in expect {
                if let Some(num) = val.as_float() {
                    let got = *out
                        .numbers
                        .get(key)
                        .ok_or_else(|| format!("{id}: missing output {key}"))?;
                    rel_close(got, num, rel_tol)
                        .map_err(|e| format!("{id} key {key}: {e}"))?;
                } else if let Some(text) = val.as_str() {
                    let got = out
                        .strings
                        .get(key)
                        .ok_or_else(|| format!("{id}: missing string {key}"))?;
                    if got != text {
                        return Err(format!("{id} key {key}: got {got} expect {text}"));
                    }
                } else {
                    return Err(format!("{id}: unsupported expect type for {key}"));
                }
            }
        }
    }
    if total != 119 {
        return Err(format!("expected 119 vectors, ran {total}"));
    }
    Ok(())
}
