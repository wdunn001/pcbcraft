#!/usr/bin/env python3
"""Graft keymap + design compare from _feat_lib.rs onto origin/main ui-egui lib.rs."""
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

out.insert(out.index("mod layers_tree;"), "mod keymap;")
out.insert(out.index("use targets::TapButton;"), "use std::collections::HashMap;")

pref_struct = next(x for x, l in enumerate(out) if l.startswith("struct Preferences {"))
out[pref_struct:pref_struct] = feat[646:650] + [""]

mod_comment = next(
    x for x, l in enumerate(out) if l.strip().startswith("/// Remapped tool modifiers")
)
for line in reversed(feat[659:663]):
    out.insert(mod_comment, line)

lang_fn = next(x for x, l in enumerate(out) if l.startswith("fn default_language()"))
for line in reversed(feat[708:716]):
    out.insert(lang_fn, line)

default_body = next(x for x, l in enumerate(out) if "modifiers: Default::default()," in l)
out[default_body:default_body] = [
    "            active_keymap_profile: default_keymap_profile(),",
    "            keymap_profiles: default_keymap_profiles(),",
]

pair_rows = next(x for x, l in enumerate(out) if l.startswith("/// Pair name, coupling gap"))
out[pair_rows:pair_rows] = feat[729:777] + [""]

shortcuts_open = next(x for x, l in enumerate(out) if l.strip() == "shortcuts_open: bool,")
out[shortcuts_open + 1 : shortcuts_open + 1] = feat[1059:1068]

library_field = next(x for x, l in enumerate(out) if l.strip() == "library: library::Browser,")
out[library_field + 1 : library_field + 1] = feat[1074:1078]

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

pref_load = next(x for x, l in enumerate(out) if 'get_value(s, "preferences")' in l)
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
out[s_start:s_end] = feat[3301:3375] + [""]

settings = next(
    x for x, l in enumerate(out) if l.startswith("    fn settings_tab(&mut self, ui: &mut egui::Ui) {")
)
out[settings:settings] = feat[3685:3832] + [""] + feat[4788:4982] + [""]
settings_body = next(
    x
    for x, l in enumerate(out)
    if l.startswith("    fn settings_tab(&mut self, ui: &mut egui::Ui) {")
)
panel_anchor = next(
    x
    for x, l in enumerate(out[settings_body + 1 :], settings_body + 1)
    if l.strip().startswith("ui.")
)
out.insert(panel_anchor, "        self.shortcuts_settings_panel(ui);")
out.insert(panel_anchor + 1, "        ui.add_space(10.0);")

export_if = next(
    x
    for x, l in enumerate(out)
    if l.strip() == "if ui" and x + 1 < len(out) and 'menu.file.export' in out[x + 2]
)
out[export_if:export_if] = [
    "            if ui",
    '                .button(t!("menu.file.compare"))',
    '                .on_hover_text(t!("menu.file.compare.tip"))',
    "                .clicked()",
    "            {",
    "                self.compare_open = true;",
    "            }",
    "",
]

show_shortcuts = next(x for x, l in enumerate(out) if "        self.shortcuts(&ctx);" in l)
out.insert(show_shortcuts + 1, "        if self.compare_open {")
out.insert(show_shortcuts + 2, "            self.compare_window(&ctx);")
out.insert(show_shortcuts + 3, "        }")

Path(repo / "crates/ui-egui/src/lib.rs").write_text("\n".join(out) + "\n", encoding="utf-8")
text = "\n".join(out)
print("lines", len(out), "brace", text.count("{") - text.count("}"))
