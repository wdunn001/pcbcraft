## Gerber layers produce valid RS-274X

Add parse validation using the gerber_parser crate to ensure all exported
Gerber layers are syntactically valid RS-274X format. Two existing tests now
assert that every .gbr file in the export parses without error.

- crates/io/src/lib.rs: add gerber_parser import and assert_gerber_files_parse
  helper; call it in gerber_files_export_copper_outline_and_drills and
  inch_projects_export_imperial_fabrication_files tests
