//! The machine library: one place that answers "which machine is this id" for every writer.
//!
//! A machine can come from three places: the copy a project keeps, the person's own machine
//! folder, or the files shipped with the program. This module holds the pure rules for
//! choosing between them, comparing a project copy with its source, and making new machines
//! from old ones. Reading and writing the person's folder happens in the engine.
//!
//! # Rules
//! - Resolution order is project copy, then the person's machines, then built in.
//! - A built-in machine is read-only here; changing one means copying it first.
//! - A job file is written only for a file type with a registered writer.
//!
//! # Dependencies
//! - crates/io/src/machine_profile.rs (built-in machines)
//! - crates/doc/src/machine_profile.rs (`MachineProfile`)
//! - crates/io/src/neoden_yy1.rs (the one registered placement writer)
//!
//! # Do not
//! - Name a machine id in this file.
//! - Touch the file system.
use crate::machine_profile::builtin_machine_profiles;
use crate::neoden_yy1::{NeodenYy1Options, NeodenYy1Output, YySide, placement_neoden_yy1};
use pcbcraft_doc::{MachineBasedOn, MachineOrigin, MachineProfile, Project};

/// How a project copy stands against the library machine it came from.
///
/// # Rules
/// - `LibraryNewer` means the library machine has a higher version than the one copied.
/// - `Edited` means the copy differs from the library machine but the library has not moved.
///
/// # Dependencies
/// - crates/io/src/machine_library.rs (`copy_state`)
///
/// # Do not
/// - Treat `Edited` as an error; a person may change their copy on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyState {
    /// The copy matches the library machine.
    Same,
    /// The library machine has a newer version than the copy.
    LibraryNewer,
    /// The copy has been changed and the library has not moved.
    Edited,
    /// No library machine has the id this copy came from.
    ProjectOnly,
}

/// Find a machine by id: project copy first, then the person's machines, then built in.
///
/// # Rules
/// - The result carries the origin of the place it was found.
/// - An unknown id gives one plain sentence.
///
/// # Dependencies
/// - crates/io/src/machine_profile.rs (`builtin_machine_profiles`)
///
/// # Do not
/// - Fall through to a built-in machine when a project copy exists but is invalid.
pub fn resolve_machine_profile(
    project: &[MachineProfile],
    user: &[MachineProfile],
    id: &str,
) -> Result<MachineProfile, String> {
    if let Some(found) = project.iter().find(|m| m.id == id) {
        let mut found = found.clone();
        found.origin = MachineOrigin::Project;
        return Ok(found);
    }
    if let Some(found) = user.iter().find(|m| m.id == id) {
        let mut found = found.clone();
        found.origin = MachineOrigin::User;
        return Ok(found);
    }
    builtin_machine_profiles()
        .iter()
        .find(|m| m.id == id)
        .cloned()
        .ok_or_else(|| format!("no machine named {id} in the project or the machine library"))
}

/// Every library machine: the person's own first, then the built-in ones they do not shadow.
///
/// # Rules
/// - A person's machine with the id of a built-in one hides that built-in entry.
/// - `hidden` lists built-in ids the person chose not to see; they are left out.
///
/// # Dependencies
/// - crates/io/src/machine_profile.rs (`builtin_machine_profiles`)
///
/// # Do not
/// - Remove a built-in machine; hiding only leaves it out of this list.
pub fn library_machines(user: &[MachineProfile], hidden: &[String]) -> Vec<MachineProfile> {
    let mut out: Vec<MachineProfile> = user
        .iter()
        .map(|m| {
            let mut m = m.clone();
            m.origin = MachineOrigin::User;
            m
        })
        .collect();
    for built in builtin_machine_profiles() {
        if out.iter().any(|m| m.id == built.id) || hidden.contains(&built.id) {
            continue;
        }
        out.push(built.clone());
    }
    out
}

/// Say how a project copy stands against the library machine it came from.
///
/// # Rules
/// - The source is the library machine with the copy's own id, otherwise the `based_on` one.
/// - The version copied is the copy's own version, or the `based_on` version for a renamed copy.
/// - A copy made under a new name is not called edited for its name alone.
///
/// # Dependencies
/// - crates/io/src/machine_library.rs (`library_machines`)
///
/// # Do not
/// - Look at the copy's origin; every project copy has the same one.
pub fn copy_state(copy: &MachineProfile, library: &[MachineProfile]) -> CopyState {
    // A library machine with the copy's own id is the first choice (the person's saved
    // copy of it); otherwise the machine it was made from.
    let (source, copied, renamed) = if let Some(own) = library.iter().find(|m| m.id == copy.id) {
        (own, copy.version, false)
    } else if let Some(from) = copy.based_on.as_ref().filter(|b| !b.id.is_empty()) {
        match library.iter().find(|m| m.id == from.id) {
            Some(found) => (found, from.version, true),
            None => return CopyState::ProjectOnly,
        }
    } else {
        return CopyState::ProjectOnly;
    };
    if source.version > copied {
        return CopyState::LibraryNewer;
    }
    let mut a = copy.clone();
    let mut b = source.clone();
    for m in [&mut a, &mut b] {
        m.id = String::new();
        m.based_on = None;
        m.version = 0;
        if renamed {
            m.name = String::new();
        }
    }
    if a == b {
        CopyState::Same
    } else {
        CopyState::Edited
    }
}

/// Make a new machine from an existing one, under a new id and name.
///
/// # Rules
/// - The copy records which machine and version it came from in `based_on`.
/// - The copy starts at version 1 and is marked as a project copy.
///
/// # Dependencies
/// - crates/doc/src/machine_profile.rs (`MachineBasedOn`)
///
/// # Do not
/// - Change anything else; the copy is the same machine until a person edits it.
pub fn duplicate_machine(source: &MachineProfile, id: &str, name: &str) -> MachineProfile {
    let mut copy = source.clone();
    copy.id = id.to_owned();
    copy.name = name.to_owned();
    copy.based_on = Some(MachineBasedOn {
        id: source.id.clone(),
        version: source.version,
    });
    copy.version = 1;
    copy.origin = MachineOrigin::Project;
    copy
}

/// A new machine of one kind with nothing set but the work area and the output file type.
///
/// # Rules
/// - The kind must be one the program knows; anything else is refused.
/// - The result passes `MachineProfile::validate`.
///
/// # Dependencies
/// - crates/doc/src/machine_profile.rs (`MachineProfile::validate`)
///
/// # Do not
/// - Guess a writer: a blank pick and place machine has no file type until one is chosen.
pub fn blank_machine(kind: &str, id: &str, name: &str) -> Result<MachineProfile, String> {
    let dialect = match kind {
        "laser_galvo" | "laser_gantry" => "laser_job",
        "cnc_mill" | "pick_place" | "uv_printer" | "reflow_oven" | "other" => "",
        _ => return Err(
            "a machine kind must be pick_place, laser_galvo, laser_gantry, cnc_mill, uv_printer, reflow_oven or other"
                .into(),
        ),
    };
    let mut machine = MachineProfile {
        id: id.to_owned(),
        name: name.to_owned(),
        kind: kind.to_owned(),
        version: 1,
        ..MachineProfile::default()
    };
    machine.work_area.x = 100.0;
    machine.work_area.y = 100.0;
    machine.work_area.origin = "lower-left".into();
    machine.work_area.y_up = true;
    machine.output.units = "mm".into();
    machine.output.line_ending = "lf".into();
    machine.output.dialect = dialect.to_owned();
    machine.validate()?;
    Ok(machine)
}

/// True when a placement job file can be written for this output file type.
///
/// # Rules
/// - Gates the Write job file control and the export, so a machine with no writer cannot be asked.
///
/// # Dependencies
/// - crates/io/src/machine_library.rs (`placement_job`)
///
/// # Do not
/// - Compare against a machine id or a vendor name.
pub fn has_placement_writer(dialect: &str) -> bool {
    dialect == "neoden_yy1"
}

/// Write the placement job files for a machine, using the writer its file type names.
///
/// # Rules
/// - A machine whose file type has no writer is refused with one plain sentence.
/// - The machine must be a pick and place machine.
///
/// # Dependencies
/// - crates/io/src/neoden_yy1.rs (`placement_neoden_yy1`)
///
/// # Do not
/// - Choose a writer by machine id.
pub fn placement_job(
    p: &Project,
    side: YySide,
    profile: &MachineProfile,
    options: &NeodenYy1Options,
) -> Result<NeodenYy1Output, String> {
    if profile.kind != "pick_place" {
        return Err(format!(
            "machine \"{}\" is not a pick and place machine",
            profile.id
        ));
    }
    if !has_placement_writer(&profile.output.dialect) {
        return Err(format!(
            "The job file of {} cannot be written yet: its file type has no writer.",
            profile.name
        ));
    }
    placement_neoden_yy1(p, side, profile, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_profile::{builtin_machine_profile_ids, machine_profile_to_toml};

    fn first_of_kind(kind: &str) -> Result<MachineProfile, String> {
        builtin_machine_profiles()
            .iter()
            .find(|m| m.kind == kind)
            .cloned()
            .ok_or_else(|| format!("no built-in {kind} machine"))
    }

    #[test]
    fn writers_resolve_project_then_user_then_built_in() -> Result<(), String> {
        let built = first_of_kind("pick_place")?;
        let id = built.id.clone();
        let mut user = built.clone();
        user.name = "From my folder".into();
        let mut project = built.clone();
        project.name = "From the project".into();
        let all = resolve_machine_profile(
            std::slice::from_ref(&project),
            std::slice::from_ref(&user),
            &id,
        )?;
        assert_eq!(all.name, "From the project");
        let no_project = resolve_machine_profile(&[], std::slice::from_ref(&user), &id)?;
        assert_eq!(no_project.name, "From my folder");
        let neither = resolve_machine_profile(&[], &[], &id)?;
        assert_eq!(neither.name, built.name);
        assert!(resolve_machine_profile(&[], &[], "no-such-machine").is_err());
        Ok(())
    }

    #[test]
    fn built_in_file_and_user_file_give_identical_output() -> Result<(), String> {
        for id in builtin_machine_profile_ids() {
            let built = crate::builtin_machine_profile(&id)?;
            let text = machine_profile_to_toml(&built)?;
            let user = crate::machine_profile_from_toml(&text)?;
            let from_user = resolve_machine_profile(&[], std::slice::from_ref(&user), &id)?;
            let from_built = resolve_machine_profile(&[], &[], &id)?;
            // The user file wins over the built-in one, and the two are the same machine.
            assert!(matches!(from_user.origin, MachineOrigin::User));
            assert_eq!(from_user, from_built, "{id} changed through a file");
            assert_eq!(
                machine_profile_to_toml(&from_user)?,
                machine_profile_to_toml(&from_built)?
            );
        }
        Ok(())
    }

    #[test]
    fn machine_with_unknown_dialect_is_refused() -> Result<(), String> {
        let mut machine = first_of_kind("pick_place")?;
        machine.output.dialect = "made_up".into();
        let options = NeodenYy1Options {
            layout: crate::YyLayout::Native,
            variant: None,
            include_dnp_as_skip: false,
        };
        let refused = placement_job(&Project::default(), YySide::Top, &machine, &options)
            .err()
            .ok_or("an unknown file type must be refused")?;
        assert!(refused.contains("has no writer"), "{refused}");
        assert!(!has_placement_writer("made_up"));
        Ok(())
    }

    #[test]
    fn a_hidden_built_in_leaves_the_list_and_a_duplicate_remembers_its_source() -> Result<(), String>
    {
        let built = first_of_kind("pick_place")?;
        let all = library_machines(&[], &[]);
        assert!(all.iter().any(|m| m.id == built.id));
        let shown = library_machines(&[], std::slice::from_ref(&built.id));
        assert!(!shown.iter().any(|m| m.id == built.id));
        let copy = duplicate_machine(&built, "my-copy", "My copy");
        assert_eq!(
            copy.based_on,
            Some(MachineBasedOn {
                id: built.id.clone(),
                version: built.version
            })
        );
        copy.validate()?;
        Ok(())
    }

    #[test]
    fn a_project_copy_is_behind_when_the_library_version_is_higher() -> Result<(), String> {
        let built = first_of_kind("pick_place")?;
        let copy = built.clone();
        assert_eq!(
            copy_state(&copy, std::slice::from_ref(&built)),
            CopyState::Same
        );
        let mut newer = built.clone();
        newer.version = built.version + 1;
        assert_eq!(copy_state(&copy, &[newer]), CopyState::LibraryNewer);
        let mut edited = built.clone();
        edited.notes = "changed".into();
        assert_eq!(
            copy_state(&edited, std::slice::from_ref(&built)),
            CopyState::Edited
        );
        assert_eq!(copy_state(&copy, &[]), CopyState::ProjectOnly);
        Ok(())
    }

    #[test]
    fn blank_machines_of_each_kind_are_valid() -> Result<(), String> {
        for kind in [
            "pick_place",
            "laser_galvo",
            "laser_gantry",
            "cnc_mill",
            "uv_printer",
            "reflow_oven",
            "other",
        ] {
            blank_machine(kind, "blank", "Blank")?;
        }
        assert!(blank_machine("toaster", "blank", "Blank").is_err());
        Ok(())
    }
}
