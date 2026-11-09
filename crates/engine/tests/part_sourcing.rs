//! Typed sourcing commands: undo, bounds, a property test on supplier edits, loading
//! old files, and the bill of materials columns (#227, #804).
use pcbcraft_engine::{Error, Session};
use serde_json::{Value, json};

fn missing() -> Error {
    Error::Invalid("missing field".into())
}

fn session_with_part() -> Result<(Session, u64), Error> {
    let mut s = Session::default();
    let id = s.execute(
        "component.add",
        json!({"kind":"resistor","reference":"R1","value":"10k","position":{"x":10.0,"y":10.0}}),
        None,
    )?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    Ok((s, id))
}

fn sourcing_of(s: &Session, id: u64) -> Result<pcbcraft_doc::Sourcing, Error> {
    s.project
        .component(id)
        .map(|c| c.sourcing.clone())
        .ok_or_else(missing)
}

#[test]
fn set_sourcing_sets_clears_and_undoes_in_one_step() -> Result<(), Error> {
    let (mut s, id) = session_with_part()?;
    s.execute(
        "component.set_sourcing",
        json!({"id":id,"manufacturer":"Yageo","mpn":"RC0603","description":"10 k",
               "lifecycle":"last_time_buy","moisture_level":3,"rohs":true,"reach":false}),
        None,
    )?;
    let set = sourcing_of(&s, id)?;
    assert_eq!(set.manufacturer, "Yageo");
    assert_eq!(set.moisture_level, Some(3));
    assert_eq!(set.rohs, Some(true));
    assert_eq!(set.reach, Some(false));
    assert_eq!(set.halogen_free, None);
    s.execute(
        "component.set_sourcing",
        json!({"id":id,"mpn":null,"rohs":null}),
        None,
    )?;
    let cleared = sourcing_of(&s, id)?;
    assert_eq!(cleared.mpn, "");
    assert_eq!(cleared.rohs, None);
    assert_eq!(cleared.manufacturer, "Yageo", "a key left out stays");
    s.undo()?;
    assert_eq!(sourcing_of(&s, id)?, set);
    s.undo()?;
    assert!(sourcing_of(&s, id)?.is_empty());
    Ok(())
}

#[test]
fn bad_sourcing_is_refused_and_changes_nothing() -> Result<(), Error> {
    let (mut s, id) = session_with_part()?;
    s.execute("component.set_sourcing", json!({"id":id,"mpn":"A"}), None)?;
    let before = sourcing_of(&s, id)?;
    for bad in [
        json!({"id":id,"moisture_level":7}),
        json!({"id":id,"moisture_level":0}),
        json!({"id":id,"manufacturer":"x".repeat(257)}),
        json!({"id":id,"description":"line\nbreak"}),
        json!({"id":id,"lifecycle":"mostly alive"}),
        json!({"id":id,"colour":"red"}),
    ] {
        assert!(
            s.execute("component.set_sourcing", bad.clone(), None)
                .is_err(),
            "{bad}"
        );
    }
    assert_eq!(sourcing_of(&s, id)?, before);
    Ok(())
}

#[test]
fn suppliers_replace_by_name_ignoring_case_and_remove() -> Result<(), Error> {
    let (mut s, id) = session_with_part()?;
    let set = |s: &mut Session, name: &str, pn: Value| {
        s.execute(
            "component.set_supplier",
            json!({"id":id,"supplier":name,"part_number":pn,"url":"https://example.test/a"}),
            None,
        )
    };
    set(&mut s, "Digi-Key", json!("A1"))?;
    set(&mut s, "Mouser", json!("B2"))?;
    set(&mut s, "DIGI-KEY", json!("A9"))?;
    let list = sourcing_of(&s, id)?.suppliers;
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].supplier, "Digi-Key", "the first spelling stays");
    assert_eq!(list[0].part_number, "A9");
    set(&mut s, "mouser", Value::Null)?;
    assert_eq!(sourcing_of(&s, id)?.suppliers.len(), 1);
    assert!(set(&mut s, "Nobody", Value::Null).is_err());
    assert!(set(&mut s, "", json!("1")).is_err());
    assert!(set(&mut s, "X", json!("")).is_err());
    Ok(())
}

/// Small seeded generator so the property run is reproducible.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

#[test]
fn property_random_supplier_edits_match_a_plain_model_and_undo_one_by_one() -> Result<(), Error> {
    let names = [
        "Digi-Key", "digi-key", "Mouser", "LCSC", "House", "Farnell", "RS", "TME",
    ];
    let (mut s, id) = session_with_part()?;
    let mut g = Lcg(227);
    // Model: (lowercase name, number) in insertion order, replaced in place, at most 16.
    let mut model: Vec<(String, String)> = vec![];
    let mut history: Vec<Vec<(String, String)>> = vec![model.clone()];
    for step in 0..400u64 {
        let name = names[(g.next() % names.len() as u64) as usize];
        let remove = g.next().is_multiple_of(3);
        let number = format!("PN{step}");
        let params = if remove {
            json!({"id":id,"supplier":name,"part_number":null})
        } else {
            json!({"id":id,"supplier":name,"part_number":number})
        };
        let key = name.to_ascii_lowercase();
        let at = model.iter().position(|(n, _)| *n == key);
        let expect_ok = match (remove, at) {
            (true, None) => false,
            (true, Some(_)) | (false, Some(_)) => true,
            (false, None) => model.len() < 16,
        };
        let outcome = s.execute("component.set_supplier", params, None);
        assert_eq!(
            outcome.is_ok(),
            expect_ok,
            "step {step} {name} remove={remove}"
        );
        if expect_ok {
            match (remove, at) {
                (true, Some(i)) => {
                    model.remove(i);
                }
                (false, Some(i)) => model[i].1 = number,
                (false, None) => model.push((key, number)),
                (true, None) => {}
            }
            history.push(model.clone());
        }
        let now: Vec<(String, String)> = sourcing_of(&s, id)?
            .suppliers
            .iter()
            .map(|p| (p.supplier.to_ascii_lowercase(), p.part_number.clone()))
            .collect();
        assert_eq!(now, model, "step {step}");
        let sourcing = sourcing_of(&s, id)?;
        assert!(sourcing.validate().is_ok());
    }
    // Undo walks the history back one accepted edit at a time, as far as the session
    // keeps steps (32).
    for _ in 0..32 {
        history.pop();
        s.undo()?;
        let now: Vec<(String, String)> = sourcing_of(&s, id)?
            .suppliers
            .iter()
            .map(|p| (p.supplier.to_ascii_lowercase(), p.part_number.clone()))
            .collect();
        assert_eq!(Some(&now), history.last());
    }
    Ok(())
}

#[test]
fn the_seventeenth_supplier_is_refused() -> Result<(), Error> {
    let (mut s, id) = session_with_part()?;
    for i in 0..16 {
        s.execute(
            "component.set_supplier",
            json!({"id":id,"supplier":format!("S{i}"),"part_number":"1"}),
            None,
        )?;
    }
    assert!(
        s.execute(
            "component.set_supplier",
            json!({"id":id,"supplier":"S16","part_number":"1"}),
            None
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn an_old_file_loads_with_typed_fields_adopted_and_free_form_kept() -> Result<(), String> {
    let (mut s, id) = session_with_part().map_err(|e| e.to_string())?;
    for (k, v) in [
        ("MPN", "RC0603"),
        ("Manufacturer", "Yageo"),
        ("Digi-Key PN", "311-ND"),
    ] {
        s.execute("component.field", json!({"id":id,"key":k,"value":v}), None)
            .map_err(|e| e.to_string())?;
    }
    assert!(
        s.project
            .component(id)
            .is_some_and(|c| c.sourcing.is_empty())
    );
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = dir.path().join("old.pcbcraft");
    // A file saved before typed fields has no sourcing key anywhere.
    let mut text: Value = serde_json::to_value(&s.project).map_err(|e| e.to_string())?;
    if let Some(list) = text["components"].as_array_mut() {
        for c in list {
            if let Some(o) = c.as_object_mut() {
                o.remove("sourcing");
            }
        }
    }
    std::fs::write(&path, text.to_string()).map_err(|e| e.to_string())?;
    let loaded = pcbcraft_io::load(&path)?;
    let c = loaded.component(id).ok_or("part missing")?;
    assert_eq!(c.sourcing.mpn, "RC0603");
    assert_eq!(c.sourcing.manufacturer, "Yageo");
    assert_eq!(c.sourcing.supplier_number("Digi-Key"), Some("311-ND"));
    assert_eq!(
        c.field("MPN"),
        Some("RC0603"),
        "the original stays readable"
    );
    assert_eq!(c.fields.len(), 3);
    Ok(())
}

#[test]
fn bom_export_reads_typed_fields_and_supplier_columns() -> Result<(), Error> {
    let (mut s, id) = session_with_part()?;
    s.execute(
        "component.set_sourcing",
        json!({"id":id,"manufacturer":"Yageo","mpn":"RC0603","description":"10 k 1%"}),
        None,
    )?;
    s.execute(
        "component.set_supplier",
        json!({"id":id,"supplier":"Digi-Key","part_number":"311-ND"}),
        None,
    )?;
    let out = s.execute(
        "bom.export",
        json!({"fields":["MPN","Manufacturer","Description","Suppliers","Supplier:digi-key"]}),
        None,
    )?;
    let row = &out["result"]["items"][0]["fields"];
    assert_eq!(row["MPN"], "RC0603");
    assert_eq!(row["Description"], "10 k 1%");
    assert_eq!(row["Suppliers"], "Digi-Key: 311-ND");
    assert_eq!(row["Supplier:digi-key"], "311-ND");
    Ok(())
}

#[test]
fn attaching_a_source_part_fills_the_typed_record() -> Result<(), Error> {
    let (mut s, id) = session_with_part()?;
    s.execute(
        "part.attach_source",
        json!({"id":id,"manufacturer":"Yageo","mpn":"RC0603","datasheet":"","distributor":"Mouser",
               "sku":"603-RC0603","description":"10 k 1% 0603","lifecycle":"Active",
               "product_url":"https://example.test/m"}),
        None,
    )?;
    let sourcing = sourcing_of(&s, id)?;
    assert_eq!(sourcing.mpn, "RC0603");
    assert_eq!(sourcing.description, "10 k 1% 0603");
    assert_eq!(sourcing.supplier_number("Mouser"), Some("603-RC0603"));
    assert_eq!(
        sourcing.suppliers[0].url.as_deref(),
        Some("https://example.test/m")
    );
    assert_eq!(
        sourcing.lifecycle,
        Some(pcbcraft_library::listing::Lifecycle::Active)
    );
    Ok(())
}
