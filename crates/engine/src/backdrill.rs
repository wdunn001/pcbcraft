//! Back-drill commands: set or clear the back-drill of one via, or the
//! default of a net class.
//!
//! # Rules
//! - `from` is `top` or `bottom`; omitting `from` clears the setting.
//! - `must_not_cut` names a copper layer (for example `In2.Cu`, `L3`, `B.Cu`).
//! - A via setting is checked against the via's own span before anything changes; a class default is checked for sizes and layer only, and applies to a member via only where it fits.
//! - Changing a via span that would make its own back-drill invalid is refused in `via.set_span`.
//!
//! # Dependencies
//! - `crates/doc/src/backdrill.rs` `BackDrill`, `Project::validate_backdrill`.
//! - `crates/engine/src/lib.rs` `CommandSpec`, `schema`, `parse`, `id_schema`.
//!
//! # Do not
//! - Mutate before validation has passed.
use super::*;
use pcbcraft_doc::{BackDrill, BackDrillFrom};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    id: Id,
    from: Option<BackDrillFrom>,
    must_not_cut: Option<String>,
    max_stub: Option<f64>,
    diameter: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassParams {
    class: Id,
    from: Option<BackDrillFrom>,
    must_not_cut: Option<String>,
    max_stub: Option<f64>,
    diameter: Option<f64>,
}

fn props() -> Value {
    json!({"from":{"type":"string","enum":["top","bottom"]},"must_not_cut":{"type":"string"},"max_stub":{"type":"number","minimum":0,"maximum":20},"diameter":{"type":"number","exclusiveMinimum":0,"maximum":20}})
}

pub(super) fn commands() -> Vec<CommandSpec> {
    let mut via = props();
    let mut class = props();
    if let (Some(v), Some(c)) = (via.as_object_mut(), class.as_object_mut()) {
        v.insert("id".into(), id_schema());
        c.insert("class".into(), id_schema());
    }
    vec![
        CommandSpec {
            id: "via.set_backdrill",
            description: "Back-drill a via: drill from the top or bottom face up to, but not through, the must_not_cut copper layer, with a drill larger than the via hole and a longest allowed leftover stub in millimetres. Omit from to clear it. The layers the drill removes stop conducting through the via",
            input_schema: schema(via, &["id"]),
        },
        CommandSpec {
            id: "netclass.set_backdrill",
            description: "Set the back-drill that vias of a net class use when they have none of their own; applies to a via only where it fits that via's span. Omit from to clear it",
            input_schema: schema(class, &["class"]),
        },
    ]
}

fn build(
    p: &Project,
    from: Option<BackDrillFrom>,
    layer: Option<&str>,
    stub: Option<f64>,
    diameter: Option<f64>,
) -> Result<Option<BackDrill>, Error> {
    let Some(from) = from else {
        return Ok(None);
    };
    let (Some(layer), Some(max_stub), Some(diameter)) = (layer, stub, diameter) else {
        return Err(Error::Invalid(
            "a back-drill needs from, must_not_cut, max_stub and diameter".into(),
        ));
    };
    let must_not_cut = p.resolve_copper_layer(layer).map_err(Error::Invalid)?;
    let spec = BackDrill {
        from,
        must_not_cut,
        max_stub,
        diameter,
    };
    spec.validate_sizes().map_err(Error::Invalid)?;
    Ok(Some(spec))
}

pub(super) fn run(p: &mut Project, command: &str, params: &Value) -> Option<Result<Value, Error>> {
    Some(match command {
        "via.set_backdrill" => parse::<SetParams>(params).and_then(|a| {
            let spec = build(p, a.from, a.must_not_cut.as_deref(), a.max_stub, a.diameter)?;
            let via = p
                .board
                .vias
                .iter()
                .find(|v| v.id == a.id)
                .ok_or_else(missing)?;
            if let Some(s) = &spec {
                p.validate_backdrill(via, s).map_err(Error::Invalid)?;
            }
            let via = p
                .board
                .vias
                .iter_mut()
                .find(|v| v.id == a.id)
                .ok_or_else(missing)?;
            via.backdrill = spec;
            let via = via.clone();
            let stub = spec.and_then(|s| p.backdrill_stub(&via, &s));
            Ok(json!({"id":a.id,"backdrill":spec,"stub_mm":stub}))
        }),
        "netclass.set_backdrill" => parse::<ClassParams>(params).and_then(|a| {
            let spec = build(p, a.from, a.must_not_cut.as_deref(), a.max_stub, a.diameter)?;
            let class = p
                .net_classes
                .iter_mut()
                .find(|k| k.id == a.class)
                .ok_or_else(missing)?;
            class.backdrill = spec;
            Ok(json!({"class":a.class,"backdrill":spec}))
        }),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn six_layer_via() -> Result<(Session, Id), Error> {
        let mut s = Session::default();
        for n in 0..6 {
            let name = match n {
                0 => "F.Cu".to_string(),
                5 => "B.Cu".to_string(),
                k => format!("In{k}.Cu"),
            };
            s.execute(
                "board.set_layer_record",
                json!({"name":name,"kind":"copper","thickness":0.035}),
                None,
            )?;
        }
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or(Error::Invalid("net id".into()))?;
        let id = s.execute(
            "via.place",
            json!({"net":net,"position":{"x":5.0,"y":5.0},"diameter":0.6,"drill":0.3}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or(Error::Invalid("via id".into()))?;
        Ok((s, id))
    }

    #[test]
    fn a_backdrill_is_set_validated_undone_and_cleared() -> Result<(), Error> {
        let (mut s, id) = six_layer_via()?;
        let good =
            json!({"id":id,"from":"bottom","must_not_cut":"In3.Cu","max_stub":0.3,"diameter":0.5});
        s.execute("via.set_backdrill", good, None)?;
        assert!(s.project.board.vias[0].backdrill.is_some());
        let before = s.inspect();
        for bad in [
            json!({"id":id,"from":"bottom","must_not_cut":"B.Cu","max_stub":0.3,"diameter":0.5}),
            json!({"id":id,"from":"top","must_not_cut":"F.Cu","max_stub":0.3,"diameter":0.5}),
            json!({"id":id,"from":"bottom","must_not_cut":"In2.Cu","max_stub":0.3,"diameter":0.3}),
            json!({"id":id,"from":"bottom","must_not_cut":"In2.Cu","max_stub":-1.0,"diameter":0.5}),
            json!({"id":id,"from":"bottom","must_not_cut":"In2.Cu","max_stub":0.3}),
            json!({"id":id,"from":"bottom","must_not_cut":"Nope","max_stub":0.3,"diameter":0.5}),
            json!({"id":999,"from":"bottom","must_not_cut":"In2.Cu","max_stub":0.3,"diameter":0.5}),
        ] {
            assert!(s.execute("via.set_backdrill", bad, None).is_err());
        }
        assert_eq!(s.inspect(), before, "refused commands change nothing");
        let refused = s.execute(
            "via.set_span",
            json!({"id":id,"from_layer":"In1.Cu","to_layer":"In2.Cu"}),
            None,
        );
        assert!(
            refused.is_err(),
            "a span the back-drill no longer fits is refused"
        );
        s.execute("via.set_backdrill", json!({"id":id}), None)?;
        assert!(s.project.board.vias[0].backdrill.is_none());
        s.undo()?;
        assert!(
            s.project.board.vias[0].backdrill.is_some(),
            "clearing undoes"
        );
        Ok(())
    }
}
