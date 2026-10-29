#!/usr/bin/env python3
import subprocess
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
main = (
    subprocess.check_output(
        ["git", "-C", str(repo), "show", "origin/main:crates/ui-egui/src/lib.rs"]
    )
    .decode("utf-8")
    .splitlines()
)
feat = Path(repo / "_feat_lib.rs").read_text(encoding="utf-8").splitlines()
out = main[:]

out.insert(out.index("mod layers_tree;") + 1, "mod keymap;")
out.insert(out.index("use targets::TapButton;"), "use std::collections::HashMap;")

i = next(x for x, l in enumerate(out) if l.startswith("struct Preferences {"))
out[i:i] = feat[646:651] + [""]

for line in feat[659:664]:
    out.insert(out.index("    hidden_layers: Vec<String>,") + 1, line)

i = next(x for x, l in enumerate(out) if l.startswith("fn default_language()"))
for line in reversed(feat[728:733]):
    out.insert(i, line)
out.insert(i, "")

mod_idx = next(x for x, l in enumerate(out) if "modifiers: Default::default()," in l)
out[mod_idx:mod_idx] = [
    "            active_keymap_profile: default_keymap_profile(),",
    "            keymap_profiles: default_keymap_profiles(),",
]

pair_rows = next(x for x, l in enumerate(out) if l.startswith("/// Pair name, coupling gap"))
out[pair_rows:pair_rows] = [""] + feat[729:777] + [""]

lib_idx = out.index("    library: library::Browser,")
for line in reversed(feat[1060:1079]):
    out.insert(lib_idx, line)

frames_idx = out.index("            frames: 0,")
inits = [
    "            binding_capture: None,",
    "            profile_name_draft: String::new(),",
    "            profile_import_text: String::new(),",
    "            keymap_notice: String::new(),",
    "            connected_cycle: 0,",
    "            last_repeat: None,",
    "            needs_zoom_selection: false,",
    "            tree_search_focus: false,",
    "            shortcuts_settings_open: false,",
    "            compare_open: false,",
    "            compare_baseline_path: String::new(),",
    "            compare_report_open: false,",
    "            compare_diff: None,",
    "            compare_tint: std::collections::BTreeMap::new(),",
]
for line in reversed(inits):
    out.insert(frames_idx + 1, line)

pref_load = next(
    x for x, l in enumerate(out) if 'get_value(s, "preferences")' in l
)
pref_load = next(
    x for x, l in enumerate(out[pref_load:], pref_load) if ".unwrap_or_default();" in l
)
out.insert(pref_load + 1, "        preferences.normalize_keymaps();")

s_start = next(
    x for x, l in enumerate(out) if l.startswith("    fn shortcuts(&mut self, ctx: &egui::Context) {")
)
s_end = s_start + 1
while s_end < len(out) and not (out[s_end].startswith("    fn ") and s_end > s_start + 5):
    s_end += 1
out[s_start:s_end] = feat[3120:3375] + [""]

settings = next(
    x for x, l in enumerate(out) if l.startswith("    fn settings_tab(&mut self, ui: &mut egui::Ui) {")
)
out[settings:settings] = feat[3686:3830] + [""] + feat[4789:4980] + [""]
out.insert(settings + 3, "        self.shortcuts_settings_panel(ui);")
out.insert(settings + 4, "        ui.add_space(10.0);")

export_btn = next(x for x, l in enumerate(out) if '.button(t!("menu.file.export"))' in l)
out[export_btn:export_btn] = feat[3857:3863] + [""]

show_shortcuts = next(x for x, l in enumerate(out) if "        self.shortcuts(&ctx);" in l)
out.insert(show_shortcuts + 1, "        if self.compare_open {")
out.insert(show_shortcuts + 2, "            self.compare_window(&ctx);")
out.insert(show_shortcuts + 3, "        }")

Path(repo / "crates/ui-egui/src/lib.rs").write_text("\n".join(out) + "\n", encoding="utf-8")
t = "\n".join(out)
print("lines", len(out), "brace", t.count("{") - t.count("}"))
