//! Named-parameter validation and binding oracle tests.
use pcbcraft_doc::{
    Binding, Field, Hole, Parameter, Project, Via, valid_parameter_name, valid_parameter_note,
    valid_parameter_value, MAX_PARAMETER_NOTE, MAX_PARAMETER_VALUE,
};
use pcbcraft_geom::Point;

type T = Result<(), String>;

#[test]
fn valid_parameter_name_accepts_identifiers_and_rejects_garbage() {
    assert!(valid_parameter_name("via_drill"));
    assert!(valid_parameter_name("_pad"));
    assert!(!valid_parameter_name(""));
    assert!(!valid_parameter_name("9starts_with_digit"));
    assert!(!valid_parameter_name(&"a".repeat(33)));
}

#[test]
fn valid_parameter_value_and_note_bound_the_table() {
    assert!(valid_parameter_value(0.0));
    assert!(valid_parameter_value(MAX_PARAMETER_VALUE));
    assert!(!valid_parameter_value(MAX_PARAMETER_VALUE + 1.0));
    assert!(!valid_parameter_value(f64::INFINITY));
    assert!(valid_parameter_note("fits"));
    assert!(!valid_parameter_note(&"n".repeat(MAX_PARAMETER_NOTE + 1)));
}

#[test]
fn field_is_hole_and_is_via_partition_hole_fields() {
    assert!(Field::HoleDiameter.is_hole());
    assert!(!Field::HoleDiameter.is_via());
    assert!(Field::ViaDrill.is_via());
    assert!(!Field::ViaDrill.is_hole());
}

#[test]
fn project_parameter_helpers_round_trip_bindings() -> T {
    let mut p = Project::default();
    p.parameters.insert(
        "via_drill".into(),
        Parameter {
            value: 0.35,
            note: String::new(),
        },
    );
    p.board.vias.push(Via {
        id: 10,
        net: 0,
        position: Point::new(1.0, 2.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    p.bindings.push(Binding {
        object: 10,
        field: Field::ViaDrill,
        parameter: "via_drill".into(),
    });
    assert_eq!(
        p.parameter("via_drill").map(|par| par.value),
        Some(0.35)
    );
    assert_eq!(p.parameter_bindings("via_drill").count(), 1);
    assert!((p.field_value(10, Field::ViaDrill).unwrap_or(0.0) - 0.3).abs() < 1e-12);
    assert!(p.carries_field(10, Field::ViaDrill));
    assert!(!p.carries_field(10, Field::HoleDiameter));
    p.validate_parameters()?;
    p.board.vias.clear();
    p.prune_bindings();
    assert!(p.bindings.is_empty());
    Ok(())
}

#[test]
fn validate_parameters_rejects_a_missing_parameter_or_duplicate_binding() -> T {
    let mut p = Project::default();
    p.board.holes.push(Hole {
        id: 7,
        position: Point::default(),
        diameter: 1.0,
        end: None,
    });
    p.bindings.push(Binding {
        object: 7,
        field: Field::HoleDiameter,
        parameter: "ghost".into(),
    });
    assert!(p.validate_parameters().is_err());
    p.parameters.insert(
        "d".into(),
        Parameter {
            value: 1.0,
            note: String::new(),
        },
    );
    p.bindings[0].parameter = "d".into();
    p.bindings.push(Binding {
        object: 7,
        field: Field::HoleDiameter,
        parameter: "d".into(),
    });
    assert!(p.validate_parameters().is_err());
    Ok(())
}
