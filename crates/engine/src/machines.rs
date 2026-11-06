//! The machine library commands: list, add, install, duplicate, update, save, hide and remove.
//!
//! A machine can sit in three places: the copy a project keeps, the person's own machine
//! folder (beside the preferences), and the files shipped with the program. These commands
//! move machines between those places. The project always holds its own copy of every
//! machine it uses, so a project's output does not change when the library does.
//!
//! # Rules
//! - Project changes go through the normal command path, so each is one undo step.
//! - The person's folder is read and written through `UserConfig`; a session with no folder
//!   refuses a write with one plain sentence and reads an empty library.
//! - A built-in machine can be hidden but never deleted or changed.
//!
//! # Dependencies
//! - crates/io/src/machine_library.rs (resolution, copy state, duplicate, blank machines)
//! - crates/io/src/machine_profile.rs (TOML reading and writing)
//! - crates/engine/src/user_config.rs (the folder)
//!
//! # Do not
//! - Name a machine id here.
//! - Write a project copy back into the library without being asked (`machine.library_save`).
use crate::user_config::UserConfig;
use crate::{Error, parse, upsert_machine_profile};
use pcbcraft_doc::{MachineOrigin, MachineProfile, Project, ReflowProfile};
use pcbcraft_io::CopyState;
use serde::Deserialize;
use serde_json::{Value, json};

const FOLDER: &str = "machines";
const HIDDEN_FILE: &str = "hidden-machines.txt";

/// True for the commands that change the person's folder and never the project.
pub(crate) fn writes_folder_only(command: &str) -> bool {
    matches!(
        command,
        "machine.library_list"
            | "machine.library_save"
            | "machine.library_hide"
            | "machine.library_remove"
    )
}

/// The machines in the person's own folder, marked as theirs. Unreadable files are skipped.
///
/// # Rules
/// - A session with no folder has no machines.
/// - A file that is not a valid machine is left out, not an error.
///
/// # Do not
/// - Create the folder; only a save does.
pub(crate) fn user_machines(config: &UserConfig) -> Vec<MachineProfile> {
    let Ok(folder) = config.file(FOLDER) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|text| pcbcraft_io::machine_profile_from_toml(&text).ok())
        .map(|mut m| {
            m.origin = MachineOrigin::User;
            m
        })
        .collect()
}

/// The built-in ids the person chose to hide.
pub(crate) fn hidden_ids(config: &UserConfig) -> Vec<String> {
    let Ok(path) = config.file(HIDDEN_FILE) else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn no_folder() -> Error {
    Error::Invalid("There is no folder for your machines on this computer.".into())
}

/// Create the folder a file will be written into.
fn make_parent(path: &std::path::Path) -> Result<(), Error> {
    match path.parent() {
        Some(parent) => std::fs::create_dir_all(parent).map_err(|e| Error::Invalid(e.to_string())),
        None => Ok(()),
    }
}

fn write_hidden(config: &UserConfig, ids: &[String]) -> Result<(), Error> {
    let path = config.file(HIDDEN_FILE).map_err(|_| no_folder())?;
    make_parent(&path)?;
    let mut text = ids.join("\n");
    text.push('\n');
    pcbcraft_io::atomic_write(&path, text.as_bytes()).map_err(Error::Invalid)
}

fn save_user_machine(config: &UserConfig, machine: &MachineProfile) -> Result<(), Error> {
    let path = config
        .file(&format!("{FOLDER}/{}.toml", machine.id))
        .map_err(|_| no_folder())?;
    let text = pcbcraft_io::machine_profile_to_toml(machine).map_err(Error::Invalid)?;
    make_parent(&path)?;
    pcbcraft_io::atomic_write(&path, text.as_bytes()).map_err(Error::Invalid)
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    trimmed.chars().take(64).collect::<String>()
}

fn taken(project: &Project, user: &[MachineProfile], id: &str) -> bool {
    project.machine_profiles.iter().any(|m| m.id == id)
        || user.iter().any(|m| m.id == id)
        || pcbcraft_io::builtin_machine_profiles()
            .iter()
            .any(|m| m.id == id)
}

fn free_id(project: &Project, user: &[MachineProfile], wanted: &str) -> String {
    let base: String = wanted.chars().take(56).collect();
    let base = base.trim_matches('-').to_owned();
    if !taken(project, user, &base) {
        return base;
    }
    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        if !taken(project, user, &candidate) {
            return candidate;
        }
    }
    base
}

fn yes() -> bool {
    true
}

/// What `machine.add` and `machine.duplicate` can be asked.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddParams {
    /// A library machine to copy as it is (the older way to ask).
    #[serde(default)]
    pub(crate) preset: Option<String>,
    /// The id of the new machine.
    #[serde(default)]
    pub(crate) id: Option<String>,
    /// A whole machine to store.
    #[serde(default)]
    pub(crate) profile: Option<Value>,
    /// Start a blank machine of this kind.
    #[serde(default)]
    pub(crate) kind: Option<String>,
    /// The name of the new machine.
    #[serde(default)]
    pub(crate) name: Option<String>,
    /// A machine to start from; the new machine remembers it.
    #[serde(default)]
    pub(crate) based_on: Option<String>,
    /// Also keep the new machine in the person's own machines.
    #[serde(default)]
    pub(crate) keep_in_my_machines: bool,
    /// Put the new machine in the project; on unless turned off.
    #[serde(default = "yes")]
    pub(crate) add_to_project: bool,
}

/// The `machine.add` command: [`add`] with the size-limited profile reader.
pub(crate) fn run_add(config: &UserConfig, p: &mut Project, a: AddParams) -> Result<Value, Error> {
    add(config, p, a, crate::machine_profile_from_json)
}

/// Add a machine to the project and, when asked, to the person's machines.
///
/// # Rules
/// - Exactly one of `preset`, `profile`, `kind` or `based_on` says where the machine comes from.
/// - `preset` and `profile` keep their id; `based_on` and `kind` get a free id made from the name.
/// - Returns the id and whether an existing project machine was replaced.
///
/// # Dependencies
/// - crates/io/src/machine_library.rs (`resolve_machine_profile`, `duplicate_machine`, `blank_machine`)
///
/// # Do not
/// - Replace a project machine that has the same id; that is refused.
pub(crate) fn add(
    config: &UserConfig,
    p: &mut Project,
    a: AddParams,
    parsed_profile: impl FnOnce(&Value) -> Result<MachineProfile, Error>,
) -> Result<Value, Error> {
    let user = user_machines(config);
    let sources = [
        a.preset.is_some(),
        a.profile.is_some(),
        a.kind.is_some(),
        a.based_on.is_some(),
    ];
    if sources.iter().filter(|s| **s).count() != 1 {
        return Err(Error::Invalid(
            "machine.add needs exactly one of a preset id, a profile object, a kind or a machine to start from".into(),
        ));
    }
    let name = a.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let mut profile = if let Some(preset) = &a.preset {
        if preset.is_empty() || preset.len() > 64 {
            return Err(Error::Invalid(
                "built-in preset id must be 1 to 64 bytes".into(),
            ));
        }
        let mut found = pcbcraft_io::resolve_machine_profile(&[], &user, preset)
            .map_err(|_| Error::Invalid("unknown built-in machine profile".into()))?;
        if let Some(id) = &a.id {
            if id.is_empty() || id.len() > 64 {
                return Err(Error::Invalid(
                    "machine profile id must be 1 to 64 bytes".into(),
                ));
            }
            found.id = id.clone();
        }
        found
    } else if let Some(body) = &a.profile {
        parsed_profile(body)?
    } else if let Some(kind) = &a.kind {
        let label = name.unwrap_or("New machine");
        let id = free_id(p, &user, &a.id.clone().unwrap_or_else(|| slug(label)));
        pcbcraft_io::blank_machine(kind, &id, label).map_err(Error::Invalid)?
    } else if let Some(from) = &a.based_on {
        let source = pcbcraft_io::resolve_machine_profile(&p.machine_profiles, &user, from)
            .map_err(Error::Invalid)?;
        let label = name
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} copy", source.name));
        let id = free_id(p, &user, &a.id.clone().unwrap_or_else(|| slug(&label)));
        pcbcraft_io::duplicate_machine(&source, &id, &label)
    } else {
        return Err(Error::Invalid("machine.add needs a source".into()));
    };
    profile.origin = MachineOrigin::Project;
    profile.validate().map_err(Error::Invalid)?;
    let id = profile.id.clone();
    if a.add_to_project && p.machine_profiles.iter().any(|m| m.id == id) {
        return Err(Error::Invalid(
            "a machine profile with that id already exists".into(),
        ));
    }
    if a.keep_in_my_machines {
        save_user_machine(config, &profile)?;
    }
    let replaced = if a.add_to_project {
        upsert_machine_profile(&mut p.machine_profiles, profile)?
    } else {
        false
    };
    Ok(
        json!({"id": id, "replaced": replaced, "in_project": a.add_to_project, "in_my_machines": a.keep_in_my_machines}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdParams {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DuplicateParams {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    new_id: Option<String>,
    #[serde(default)]
    keep_in_my_machines: bool,
    #[serde(default = "yes")]
    add_to_project: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HideParams {
    id: String,
    #[serde(default = "yes")]
    hidden: bool,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ListParams {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    include_hidden: bool,
}

fn source_word(origin: MachineOrigin) -> &'static str {
    match origin {
        MachineOrigin::BuiltIn => "built_in",
        MachineOrigin::User => "mine",
        MachineOrigin::Project => "project",
    }
}

fn state_word(state: CopyState) -> &'static str {
    match state {
        CopyState::Same => "same",
        CopyState::LibraryNewer => "library_newer",
        CopyState::Edited => "edited",
        CopyState::ProjectOnly => "project_only",
    }
}

fn is_built_in(m: &MachineProfile) -> bool {
    matches!(m.origin, MachineOrigin::BuiltIn)
}

fn card(m: &MachineProfile, hidden: bool) -> Value {
    json!({
        "id": m.id,
        "name": m.name,
        "kind": m.kind,
        "vendor": m.vendor,
        "model": m.model,
        "version": m.version,
        "source": source_word(m.origin),
        "hidden": hidden,
        "work_area": {"x": m.work_area.x, "y": m.work_area.y},
        "nozzles": m.pick_place.nozzle.len(),
        "rated_power_w": m.reflow_oven.rated_power_w,
        "max_temp_c": m.reflow_oven.max_temp_c,
        "heating": m.reflow_oven.heating,
        "profile_count": m.reflow_oven.profile_count,
        "dialect": m.output.dialect,
        "has_writer": pcbcraft_io::has_placement_writer(&m.output.dialect),
    })
}

fn list(config: &UserConfig, p: &Project, a: ListParams) -> Result<Value, Error> {
    let scope = a.scope.as_deref().unwrap_or("all");
    if !matches!(scope, "all" | "built_in" | "mine" | "project") {
        return Err(Error::Invalid(
            "scope must be all, built_in, mine or project".into(),
        ));
    }
    let user = user_machines(config);
    let hidden = hidden_ids(config);
    let everything = pcbcraft_io::library_machines(&user, &[]);
    let query = a.query.as_deref().unwrap_or("").trim().to_lowercase();
    let fits = |m: &MachineProfile| {
        a.kind.as_deref().is_none_or(|k| m.kind == k)
            && (query.is_empty()
                || [&m.name, &m.vendor, &m.model, &m.id]
                    .iter()
                    .any(|f| f.to_lowercase().contains(&query)))
    };
    let mut cards: Vec<Value> = Vec::new();
    if matches!(scope, "all" | "built_in" | "mine") {
        for m in &everything {
            let is_hidden = is_built_in(m) && hidden.contains(&m.id);
            if (is_hidden && !a.include_hidden) || !fits(m) {
                continue;
            }
            if (scope == "built_in" && !is_built_in(m)) || (scope == "mine" && is_built_in(m)) {
                continue;
            }
            let mut c = card(m, is_hidden);
            let copy = p.machine_profiles.iter().find(|c| c.id == m.id);
            c["in_project"] = json!(copy.is_some());
            if let Some(copy) = copy {
                c["copy_state"] = json!(state_word(pcbcraft_io::copy_state(copy, &everything)));
            }
            cards.push(c);
        }
    }
    if matches!(scope, "all" | "project") {
        for copy in &p.machine_profiles {
            let in_library = everything.iter().any(|m| m.id == copy.id);
            if (in_library && scope == "all") || !fits(copy) {
                continue;
            }
            let mut c = card(copy, false);
            c["in_project"] = json!(true);
            c["copy_state"] = json!(state_word(pcbcraft_io::copy_state(copy, &everything)));
            cards.push(c);
        }
    }
    Ok(json!({"machines": cards, "count": cards.len()}))
}

/// Which top-level parts of two machines differ, for the "Update from library" answer.
fn differences(a: &MachineProfile, b: &MachineProfile) -> Vec<String> {
    let (Ok(Value::Object(x)), Ok(Value::Object(y))) =
        (serde_json::to_value(a), serde_json::to_value(b))
    else {
        return Vec::new();
    };
    let mut keys: Vec<String> = x.keys().chain(y.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|k| !matches!(k.as_str(), "id" | "based_on" | "version") && x.get(k) != y.get(k))
        .collect()
}

fn project_copy(p: &Project, id: &str) -> Result<MachineProfile, Error> {
    p.machine_profiles
        .iter()
        .find(|m| m.id == id)
        .cloned()
        .ok_or_else(|| Error::Invalid(format!("no machine named {id} in the project")))
}

/// Parameters for `reflow.profile_add`.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReflowProfileAddParams {
    /// The machine profile id.
    pub(crate) machine: String,
    /// The profile to add.
    pub(crate) profile: Value,
}

/// Parameters for `reflow.profile_remove`.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReflowProfileRemoveParams {
    /// The machine profile id.
    pub(crate) machine: String,
    /// The profile id to remove.
    pub(crate) profile: String,
}

/// Run one library command.
///
/// # Rules
/// - `machine.library_list` reads only; the others are one undo step or touch only the folder.
/// - Every refusal is one plain sentence.
///
/// # Dependencies
/// - crates/engine/src/lib.rs (`run_command_part_19` calls this)
///
/// # Do not
/// - Call it for a command the engine does not route here.
pub(crate) fn run(
    config: &UserConfig,
    p: &mut Project,
    command: &str,
    params: &Value,
) -> Result<Value, Error> {
    match command {
        "machine.library_list" => {
            let a: ListParams = parse(params)?;
            list(config, p, a)
        }
        "machine.install" => {
            let a: IdParams = parse(params)?;
            if p.machine_profiles.iter().any(|m| m.id == a.id) {
                return Err(Error::Invalid(
                    "That machine is already on the project. Use update from library to refresh it."
                        .into(),
                ));
            }
            let user = user_machines(config);
            let mut found =
                pcbcraft_io::resolve_machine_profile(&[], &user, &a.id).map_err(Error::Invalid)?;
            found.origin = MachineOrigin::Project;
            upsert_machine_profile(&mut p.machine_profiles, found)?;
            Ok(json!({"id": a.id}))
        }
        "machine.duplicate" => {
            let a: DuplicateParams = parse(params)?;
            add(
                config,
                p,
                AddParams {
                    based_on: Some(a.id),
                    name: a.name,
                    id: a.new_id,
                    keep_in_my_machines: a.keep_in_my_machines,
                    add_to_project: a.add_to_project,
                    ..AddParams::default()
                },
                |_| Err(Error::Invalid("no profile expected".into())),
            )
        }
        "machine.update_from_library" => {
            let a: IdParams = parse(params)?;
            let copy = project_copy(p, &a.id)?;
            let library = pcbcraft_io::library_machines(&user_machines(config), &[]);
            // The library machine with the copy's own id comes first, then the one it was made from.
            let source = library
                .iter()
                .find(|m| m.id == copy.id)
                .or_else(|| {
                    copy.based_on
                        .as_ref()
                        .filter(|b| !b.id.is_empty())
                        .and_then(|b| library.iter().find(|m| m.id == b.id))
                })
                .cloned()
                .ok_or_else(|| {
                    Error::Invalid("The library has no machine this copy came from.".into())
                })?;
            let changed = differences(&copy, &source);
            let mut updated = source.clone();
            updated.id = copy.id.clone();
            updated.origin = MachineOrigin::Project;
            if copy.id != source.id {
                updated.name = copy.name.clone();
                updated.based_on = Some(pcbcraft_doc::MachineBasedOn {
                    id: source.id.clone(),
                    version: source.version,
                });
            }
            upsert_machine_profile(&mut p.machine_profiles, updated)?;
            Ok(json!({"id": a.id, "changed": changed}))
        }
        "machine.library_save" => {
            let a: IdParams = parse(params)?;
            let copy = project_copy(p, &a.id)?;
            if pcbcraft_io::builtin_machine_profiles()
                .iter()
                .any(|m| m.id == copy.id)
            {
                return Err(Error::Invalid(
                    "A built-in machine keeps its own id. Duplicate it first, then save the copy."
                        .into(),
                ));
            }
            save_user_machine(config, &copy)?;
            Ok(json!({"id": a.id}))
        }
        "machine.library_hide" => {
            let a: HideParams = parse(params)?;
            if !pcbcraft_io::builtin_machine_profiles()
                .iter()
                .any(|m| m.id == a.id)
            {
                return Err(Error::Invalid(
                    "Only a built-in machine can be hidden.".into(),
                ));
            }
            let mut ids = hidden_ids(config);
            ids.retain(|i| *i != a.id);
            if a.hidden {
                ids.push(a.id.clone());
            }
            write_hidden(config, &ids)?;
            Ok(json!({"id": a.id, "hidden": a.hidden}))
        }
        "machine.library_remove" => {
            let a: IdParams = parse(params)?;
            let path = config
                .file(&format!("{FOLDER}/{}.toml", a.id))
                .map_err(|_| no_folder())?;
            if !path.is_file() {
                let built_in = pcbcraft_io::builtin_machine_profiles()
                    .iter()
                    .any(|m| m.id == a.id);
                return Err(Error::Invalid(
                    if built_in {
                        "A built-in machine can be hidden but not deleted."
                    } else {
                        "That machine is not in your machines."
                    }
                    .into(),
                ));
            }
            std::fs::remove_file(path).map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"removed": a.id}))
        }
        "reflow.profile_add" => {
            let a: ReflowProfileAddParams = parse(params)?;
            let profile: ReflowProfile = parse(&a.profile)?;
            let machine = p
                .machine_profiles
                .iter_mut()
                .find(|m| m.id == a.machine)
                .ok_or_else(|| Error::Invalid(format!("no machine named {}", a.machine)))?;
            if machine.kind != "reflow_oven" {
                return Err(Error::Invalid("machine is not a reflow oven".into()));
            }
            machine.reflow_oven.profiles.push(profile);
            machine.reflow_oven.profile_count = machine.reflow_oven.profiles.len() as u32;
            machine.validate().map_err(Error::Invalid)?;
            Ok(json!({"id": a.machine, "profile_count": machine.reflow_oven.profile_count}))
        }
        "reflow.profile_remove" => {
            let a: ReflowProfileRemoveParams = parse(params)?;
            let machine = p
                .machine_profiles
                .iter_mut()
                .find(|m| m.id == a.machine)
                .ok_or_else(|| Error::Invalid(format!("no machine named {}", a.machine)))?;
            if machine.kind != "reflow_oven" {
                return Err(Error::Invalid("machine is not a reflow oven".into()));
            }
            let before = machine.reflow_oven.profiles.len();
            machine.reflow_oven.profiles.retain(|p| p.id != a.profile);
            if machine.reflow_oven.profiles.len() == before {
                return Err(Error::Invalid(format!("no profile named {}", a.profile)));
            }
            machine.reflow_oven.profile_count = machine.reflow_oven.profiles.len() as u32;
            machine.validate().map_err(Error::Invalid)?;
            Ok(json!({"id": a.machine, "profile_count": machine.reflow_oven.profile_count}))
        }
        "reflow.profile_list" => {
            let a: IdParams = parse(params)?;
            let machine = p
                .machine_profiles
                .iter()
                .find(|m| m.id == a.id)
                .ok_or_else(|| Error::Invalid(format!("no machine named {}", a.id)))?;
            if machine.kind != "reflow_oven" {
                return Err(Error::Invalid("machine is not a reflow oven".into()));
            }
            let profiles: Vec<Value> = machine
                .reflow_oven
                .profiles
                .iter()
                .map(|p| {
                    json!({
                        "id": p.id,
                        "name": p.name,
                        "stages": p.stages.iter().map(|s| json!({
                            "name": s.name,
                            "target_c": s.target_c,
                            "time_s": s.time_s
                        })).collect::<Vec<Value>>()
                    })
                })
                .collect();
            Ok(json!({"profiles": profiles, "count": profiles.len()}))
        }
        _ => Err(Error::Invalid(format!("unknown machine command {command}"))),
    }
}
