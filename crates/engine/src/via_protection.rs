//! Via protection commands: how each face of a via is protected under the
//! solder mask, for one via or for a selection of vias.
//!
//! # Rules
//! - `side` picks the face: `both` (the default) sets the whole via, `top` or `bottom` sets one face and leaves the other as it reads now.
//! - Omitting `covering` on `both` or `top` makes the via follow the board default again; omitting it on `bottom` makes the bottom follow the top.
//! - A face other than open needs a drilled hole.
//! - A command that names an unknown via, or more than 1000 vias, changes nothing.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` `Via::covering`, `Via::covering_bottom`, `ViaCovering`.
//! - `crates/engine/src/lib.rs` `CommandSpec`, `schema`, `parse`.
//!
//! # Do not
//! - Mutate before every named via has been checked.
use super::*;
use pcbcraft_doc::ViaCovering;

/// Most vias one command may change.
const MAX_VIAS: usize = 1000;

const COVERINGS: [&str; 8] = [
    "open",
    "tented",
    "tented_covered",
    "plugged",
    "plugged_covered",
    "filled",
    "filled_covered",
    "filled_capped",
];

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Face {
    #[default]
    Both,
    Top,
    Bottom,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct One {
    id: Id,
    #[serde(default)]
    covering: Option<ViaCovering>,
    #[serde(default)]
    side: Face,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Many {
    ids: Vec<Id>,
    #[serde(default)]
    covering: Option<ViaCovering>,
    #[serde(default)]
    side: Face,
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "via.set_covering",
            description: "Set how one via is protected under the solder mask (open, tented, tented_covered, plugged, plugged_covered, filled, filled_covered, filled_capped, named after the via protection types); side is both (default), top or bottom, so a via can be tented on one face and open on the other; omit covering to follow the board default. Anything but open needs a drilled hole",
            input_schema: schema(
                json!({"id":id_schema(),"covering":{"type":"string","enum":COVERINGS},"side":{"type":"string","enum":["both","top","bottom"]}}),
                &["id"],
            ),
        },
        CommandSpec {
            id: "via.set_covering_many",
            description: "Set the protection of up to 1000 vias at once, the same way via.set_covering sets one; if any via is unknown or cannot take the setting, none changes",
            input_schema: schema(
                json!({"ids":{"type":"array","minItems":1,"maxItems":MAX_VIAS,"items":id_schema()},"covering":{"type":"string","enum":COVERINGS},"side":{"type":"string","enum":["both","top","bottom"]}}),
                &["ids"],
            ),
        },
    ]
}

pub(super) fn run(p: &mut Project, command: &str, params: &Value) -> Option<Result<Value, Error>> {
    Some(match command {
        "via.set_covering" => parse::<One>(params).and_then(|a| {
            apply(p, &[a.id], a.covering, a.side)?;
            let via = p
                .board
                .vias
                .iter()
                .find(|v| v.id == a.id)
                .ok_or_else(missing)?;
            Ok(json!({"id":a.id,"covering":via.covering,"covering_bottom":via.covering_bottom}))
        }),
        "via.set_covering_many" => parse::<Many>(params).and_then(|a| {
            apply(p, &a.ids, a.covering, a.side)?;
            Ok(json!({"changed":a.ids.len()}))
        }),
        _ => return None,
    })
}

/// Check every named via, then change them all.
fn apply(
    p: &mut Project,
    ids: &[Id],
    covering: Option<ViaCovering>,
    side: Face,
) -> Result<(), Error> {
    if ids.is_empty() || ids.len() > MAX_VIAS {
        return Err(Error::Invalid(format!(
            "name between 1 and {MAX_VIAS} vias"
        )));
    }
    let closes = covering.is_some_and(|c| c != ViaCovering::Open);
    for id in ids {
        let via = p
            .board
            .vias
            .iter()
            .find(|v| v.id == *id)
            .ok_or_else(missing)?;
        if via.drill <= 0.0 && closes {
            return Err(Error::Invalid(
                "a via without a drilled hole cannot be tented, plugged or filled".into(),
            ));
        }
    }
    let default = p.board.via_covering_default;
    for id in ids {
        let Some(via) = p.board.vias.iter_mut().find(|v| v.id == *id) else {
            continue;
        };
        match side {
            Face::Both => {
                via.covering = covering;
                via.covering_bottom = None;
            }
            Face::Top => {
                // The bottom keeps what it reads now, so pin it first.
                if via.covering_bottom.is_none() {
                    via.covering_bottom = Some(via.covering.unwrap_or(default));
                }
                via.covering = covering;
            }
            Face::Bottom => {
                via.covering_bottom = covering;
            }
        }
        // A bottom that now equals the top says nothing extra.
        if via.covering_bottom == Some(via.covering.unwrap_or(default)) && via.covering.is_some() {
            via.covering_bottom = None;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_vias() -> Result<(Session, Id, Id), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or(Error::Invalid("net id".into()))?;
        let mut ids = vec![];
        for x in [5.0, 9.0] {
            ids.push(
                s.execute(
                    "via.place",
                    json!({"net":net,"position":{"x":x,"y":5.0},"diameter":0.6,"drill":0.3}),
                    None,
                )?["result"]["id"]
                    .as_u64()
                    .ok_or(Error::Invalid("via id".into()))?,
            );
        }
        Ok((s, ids[0], ids[1]))
    }

    #[test]
    fn one_face_changes_and_the_other_keeps_what_it_read() -> Result<(), Error> {
        let (mut s, a, _) = two_vias()?;
        s.execute(
            "board.set_via_covering_default",
            json!({"covering":"plugged"}),
            None,
        )?;
        s.execute(
            "via.set_covering",
            json!({"id":a,"covering":"tented","side":"top"}),
            None,
        )?;
        let via = &s.project.board.vias[0];
        assert_eq!(via.covering, Some(ViaCovering::Tented));
        assert_eq!(
            via.effective_covering_bottom(&s.project.board),
            ViaCovering::Plugged,
            "the bottom still reads the board default it read before"
        );
        s.execute(
            "via.set_covering",
            json!({"id":a,"covering":"open","side":"bottom"}),
            None,
        )?;
        assert_eq!(
            s.project.board.vias[0].covering_bottom,
            Some(ViaCovering::Open)
        );
        s.execute(
            "via.set_covering",
            json!({"id":a,"covering":"tented"}),
            None,
        )?;
        assert_eq!(
            s.project.board.vias[0].covering_bottom, None,
            "both sides again clears the split"
        );
        s.undo()?;
        assert_eq!(
            s.project.board.vias[0].covering_bottom,
            Some(ViaCovering::Open)
        );
        Ok(())
    }

    #[test]
    fn many_vias_change_together_or_not_at_all() -> Result<(), Error> {
        let (mut s, a, b) = two_vias()?;
        s.execute(
            "via.set_covering_many",
            json!({"ids":[a,b],"covering":"filled_capped","side":"bottom"}),
            None,
        )?;
        for v in &s.project.board.vias {
            assert_eq!(v.covering_bottom, Some(ViaCovering::FilledCapped));
        }
        let before = s.inspect();
        for params in [
            json!({"ids":[a,9999],"covering":"tented"}),
            json!({"ids":[],"covering":"tented"}),
            json!({"ids":[a],"covering":"glued"}),
            json!({"ids":[a],"covering":"tented","side":"left"}),
            json!({"ids":(0..1001).collect::<Vec<u64>>(),"covering":"tented"}),
        ] {
            assert!(
                s.execute("via.set_covering_many", params, None).is_err(),
                "must refuse"
            );
        }
        assert_eq!(
            s.inspect(),
            before,
            "refused commands leave the design untouched"
        );
        Ok(())
    }

    #[test]
    fn a_via_without_a_hole_refuses_a_closed_face() -> Result<(), Error> {
        let (mut s, a, _) = two_vias()?;
        s.execute(
            "via.set_size",
            json!({"id":a,"diameter":0.6,"drill":0.0}),
            None,
        )?;
        assert!(
            s.execute(
                "via.set_covering",
                json!({"id":a,"covering":"plugged","side":"bottom"}),
                None
            )
            .is_err()
        );
        s.execute(
            "via.set_covering",
            json!({"id":a,"covering":"open","side":"bottom"}),
            None,
        )?;
        Ok(())
    }
}
