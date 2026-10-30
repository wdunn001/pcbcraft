use pcbcraft_doc::{Id, Project};
use serde_json::Value;

/// Resolve the lock scope for a command and its JSON params against the current project.
pub fn command_lock_scope(project: &Project, command: &str, params: &Value) -> String {
    let kinds = candidate_kinds(project, command, params);
    merge_kinds(kinds).encode()
}

/// Commands that only read the design and never join a room batch.
pub(super) fn room_read_only(id: &str) -> bool {
    matches!(
        id,
        "design.summary"
            | "design.inspect"
            | "board.bounds"
            | "board.materials"
            | "board.panel"
            | "board.statistics"
            | "board.select_connected"
            | "board.select_net"
            | "net.members"
            | "net.lengths"
            | "net.length_groups"
            | "net.impedance"
            | "net.chain"
            | "schematic.table"
            | "schematic.net_pages"
            | "schematic.cleanup"
            | "component.find"
            | "footprint.find"
            | "library.footprint.check"
            | "rules.preset_list"
            | "artifact.export"
            | "artifact.generate"
            | "artifact.stale"
            | "route.preview"
            | "route.settings"
            | "checks.quick"
            | "checks.run"
            | "board_mesh"
            | "padstack.report"
            | "machine.profile.list"
            | "machine.profile.export"
            | "online.library.search"
            | "online.library.browse"
            | "online.part.lookup"
            | "recents.list"
            | "recents.clear"
            | "profiles.list"
            | "profiles.export"
            | "library.link.list"
            | "library.online.status"
            | "attached.list"
            | "variant.list"
            | "output.list"
            | "bom.read"
            | "calc.run"
            | "calc.list"
            | "tuning.list"
            | "tuning.read"
            | "image.list"
            | "image.read"
            | "outline.read"
            | "selection.clipboard.read"
            | "session.info"
    ) || id.starts_with("inspect.")
        || id.ends_with(".read")
        || id.ends_with(".list")
        || id.ends_with(".status")
        || id.ends_with(".lookup")
        || id.ends_with(".search")
        || id.ends_with(".browse")
        || id.ends_with(".report")
        || id.ends_with(".preview")
        || id.ends_with(".stale")
        || id.ends_with(".generate")
        || id.starts_with("file.")
        || id.starts_with("view.")
        || matches!(
            id,
            "project.export_archive"
                | "project.import_archive"
                | "output_job.package"
                | "output_job.run"
                | "artifact.export"
                | "library.import_footprint_file"
                | "library.import_symbol_file"
                | "library.import_footprint_dir"
                | "library.import_symbol_dir"
                | "library.load"
                | "library.import_online"
                | "library.place_online"
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ScopeKind {
    Net,
    Component,
    Sheet,
    Board,
}

impl ScopeKind {
    fn encode(self, id: u64) -> String {
        match self {
            ScopeKind::Net => format!("net:{id}"),
            ScopeKind::Component => format!("component:{id}"),
            ScopeKind::Sheet => format!("sheet:{id}"),
            ScopeKind::Board => "board".into(),
        }
    }

}

struct EncodedScope {
    kind: ScopeKind,
    id: u64,
}

impl EncodedScope {
    fn board() -> Self {
        Self {
            kind: ScopeKind::Board,
            id: 0,
        }
    }

    fn encode(self) -> String {
        if self.kind == ScopeKind::Board {
            "board".into()
        } else {
            self.kind.encode(self.id)
        }
    }
}

fn merge_kinds(kinds: Vec<EncodedScope>) -> EncodedScope {
    let mut iter = kinds.into_iter();
    let Some(first) = iter.next() else {
        return EncodedScope::board();
    };
    let mut merged = first;
    for next in iter {
        if merged.kind == ScopeKind::Board || next.kind == ScopeKind::Board {
            merged = EncodedScope::board();
            continue;
        }
        if merged.kind == next.kind && merged.id == next.id {
            continue;
        }
        merged = EncodedScope::board();
    }
    merged
}

fn candidate_kinds(project: &Project, command: &str, params: &Value) -> Vec<EncodedScope> {
    if always_board(command) {
        return vec![EncodedScope::board()];
    }

    let mut out = Vec::new();

    if let Some(net) = params.get("net").and_then(Value::as_u64) {
        out.push(EncodedScope {
            kind: ScopeKind::Net,
            id: net,
        });
    }
    if let Some(component) = params.get("component").and_then(Value::as_u64) {
        out.push(EncodedScope {
            kind: ScopeKind::Component,
            id: component,
        });
    }
    if let Some(sheet) = params.get("sheet").and_then(Value::as_u64) {
        out.push(EncodedScope {
            kind: ScopeKind::Sheet,
            id: sheet,
        });
    }
    if let Some(id) = params.get("id").and_then(Value::as_u64) {
        out.push(scope_for_id(project, command, id));
    }

    for key in ["from", "to", "start", "end", "position", "anchor"] {
        if let Some(v) = params.get(key) {
            out.extend(endpoint_scopes(project, v));
        }
    }
    if let Some(pins) = params.get("pins").and_then(Value::as_array) {
        for pin in pins {
            out.extend(endpoint_scopes(project, pin));
        }
    }
    if let Some(nets) = params.get("nets").and_then(Value::as_array) {
        for net in nets {
            if let Some(name) = net.get("name").and_then(Value::as_str) {
                if let Some(id) = project.net_id_by_name(name) {
                    out.push(EncodedScope {
                        kind: ScopeKind::Net,
                        id: id,
                    });
                }
            }
            if let Some(members) = net.get("pins").and_then(Value::as_array) {
                for pin in members {
                    out.extend(endpoint_scopes(project, pin));
                }
            }
        }
    }

    if out.is_empty() {
        out.push(prefix_default(command));
    }
    out
}

fn always_board(command: &str) -> bool {
    command.starts_with("project.")
        || command.starts_with("board.")
        || command.starts_with("library.")
        || command.starts_with("parameter.")
        || command.starts_with("output_job.")
        || command.starts_with("artifact.")
        || command.starts_with("machine.")
        || command.starts_with("padstack.")
        || command.starts_with("variant.")
        || command.starts_with("attached.")
        || command.starts_with("align.")
        || command.starts_with("check.")
        || command.starts_with("rules.")
        || command.starts_with("bom.")
        || command.starts_with("calc.")
        || command.starts_with("import.")
        || command.starts_with("export.")
        || command.starts_with("netlist.")
        || command.starts_with("auto_sync.")
        || command.starts_with("selection.")
        || command.starts_with("session.")
        || command.starts_with("profiles.")
        || command.starts_with("recents.")
        || command.starts_with("online.")
        || command.starts_with("image.")
        || command.starts_with("outline.")
        || command.starts_with("tuning.")
        || matches!(
            command,
            "array"
                | "teardrop.add"
                | "teardrop.remove"
                | "component.add"
                | "component.place_library"
                | "part.place"
                | "net.create"
                | "netlist.import"
                | "netlist.import_session"
                | "schematic.new_sheet"
                | "schematic.import_sheet"
                | "schematic.cleanup"
                | "schematic.table"
                | "schematic.net_pages"
                | "board.sync"
                | "route.settings"
                | "route.preview"
        )
}

fn prefix_default(command: &str) -> EncodedScope {
    if command.starts_with("net.") {
        return EncodedScope::board();
    }
    if command.starts_with("component.") {
        return EncodedScope::board();
    }
    if command.starts_with("schematic.") {
        return EncodedScope::board();
    }
    EncodedScope::board()
}

fn scope_for_id(project: &Project, command: &str, id: Id) -> EncodedScope {
    if schematic_sheet_command(command) {
        return EncodedScope {
            kind: ScopeKind::Sheet,
            id,
        };
    }
    if command.starts_with("component.") {
        return EncodedScope {
            kind: ScopeKind::Component,
            id,
        };
    }
    if command.starts_with("net.") {
        return EncodedScope {
            kind: ScopeKind::Net,
            id,
        };
    }
    if command.starts_with("footprint.") || command.starts_with("library.footprint.adopt") {
        return footprint_scope(project, id);
    }
    if command.starts_with("library.symbol.adopt") {
        return EncodedScope {
            kind: ScopeKind::Component,
            id,
        };
    }
    if command.starts_with("track.")
        || command.starts_with("via.")
        || command.starts_with("zone.")
        || command.starts_with("barcode.")
        || command.starts_with("label.")
    {
        return copper_scope(project, id);
    }
    if command.starts_with("wire.") {
        return wire_scope(project, id);
    }
    if command.starts_with("schematic.") {
        if let Some(port) = project.ports.iter().find(|p| p.id == id) {
            return EncodedScope {
                kind: ScopeKind::Sheet,
                id: port.sheet,
            };
        }
        if project.components.iter().any(|c| c.id == id) {
            return EncodedScope {
                kind: ScopeKind::Component,
                id,
            };
        }
    }
    EncodedScope::board()
}

fn schematic_sheet_command(command: &str) -> bool {
    matches!(
        command,
        "schematic.rename_sheet"
            | "schematic.remove_sheet"
            | "schematic.set_page_setting"
            | "schematic.set_sheet_parent"
    )
}

fn footprint_scope(project: &Project, footprint: Id) -> EncodedScope {
    if let Some(fp) = project.board.footprints.iter().find(|f| f.id == footprint) {
        EncodedScope {
            kind: ScopeKind::Component,
            id: fp.component,
        }
    } else {
        EncodedScope::board()
    }
}

fn copper_scope(project: &Project, object: Id) -> EncodedScope {
    if let Some(track) = project.board.tracks.iter().find(|t| t.id == object) {
        return EncodedScope {
            kind: ScopeKind::Net,
            id: track.net,
        };
    }
    if let Some(via) = project.board.vias.iter().find(|v| v.id == object) {
        return EncodedScope {
            kind: ScopeKind::Net,
            id: via.net,
        };
    }
    if let Some(zone) = project.board.zones.iter().find(|z| z.id == object) {
        return EncodedScope {
            kind: ScopeKind::Net,
            id: zone.net,
        };
    }
    EncodedScope::board()
}

fn wire_scope(project: &Project, wire: Id) -> EncodedScope {
    if let Some(w) = project.wires.iter().find(|w| w.id == wire) {
        return EncodedScope {
            kind: ScopeKind::Net,
            id: w.net,
        };
    }
    EncodedScope::board()
}

fn endpoint_scopes(project: &Project, value: &Value) -> Vec<EncodedScope> {
    let mut out = Vec::new();
    if let Some(component) = value.get("component").and_then(Value::as_u64) {
        out.push(EncodedScope {
            kind: ScopeKind::Component,
            id: component,
        });
    }
    if let (Some(footprint), Some(_pad)) = (
        value.get("footprint").and_then(Value::as_u64),
        value.get("pad").and_then(Value::as_str),
    ) {
        out.push(footprint_scope(project, footprint));
    }
    if let Some(reference) = value.get("reference").and_then(Value::as_str) {
        if let Some(c) = project.components.iter().find(|c| c.reference == reference) {
            out.push(EncodedScope {
                kind: ScopeKind::Component,
                id: c.id,
            });
        }
    }
    out
}

trait NetLookup {
    fn net_id_by_name(&self, name: &str) -> Option<Id>;
}

impl NetLookup for Project {
    fn net_id_by_name(&self, name: &str) -> Option<Id> {
        self.nets.iter().find(|n| n.name == name).map(|n| n.id)
    }
}
