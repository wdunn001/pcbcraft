use pcbcraft_library::pin_kind_from_library_string;
use pcbcraft_library::PinKind;

#[test]
fn known_library_strings_map_to_kinds() {
    let (k, unknown) = pin_kind_from_library_string("input");
    assert_eq!(k, PinKind::Input);
    assert!(!unknown);
    let (k, unknown) = pin_kind_from_library_string("power_out");
    assert_eq!(k, PinKind::PowerOut);
    assert!(!unknown);
}

#[test]
fn unknown_string_maps_to_passive_with_warning_flag() {
    let (k, unknown) = pin_kind_from_library_string("made_up_type");
    assert_eq!(k, PinKind::Passive);
    assert!(unknown);
}
