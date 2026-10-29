#!/usr/bin/env python3
"""Graft keymap + design compare from feat lib.rs onto main lib.rs."""
from pathlib import Path
import subprocess

repo = Path(__file__).resolve().parents[1]
main_path = repo / "crates/ui-egui/src/lib.rs"
feat_path = repo / "_feat_lib.rs"

if not main_path.exists() or main_path.stat().st_size < 100_000:
    main_path.write_bytes(
        subprocess.check_output(
            ["git", "-C", str(repo), "show", "origin/main:crates/ui-egui/src/lib.rs"]
        )
    )

main_lines = main_path.read_text(encoding="utf-8").splitlines()
feat_lines = feat_path.read_text(encoding="utf-8").splitlines()


def slice_lines(lines: list[str], start: int, end: int) -> list[str]:
    return lines[start - 1 : end]


def find_line(lines: list[str], needle: str, start: int = 0) -> int:
    for i in range(start, len(lines)):
        if needle in lines[i]:
            return i
    raise SystemExit(f"needle not found: {needle!r}")


def close_impl_after(lines: list[str], start: int) -> int:
    depth = 0
    for j in range(start, len(lines)):
        depth += lines[j].count("{") - lines[j].count("}")
        if j > start and depth == 0:
            return j
    raise SystemExit("impl block not closed")


compare_kind = slice_lines(feat_lines, 647, 651)
pref_keymap_fields = slice_lines(feat_lines, 660, 664)
keymap_defaults = slice_lines(feat_lines, 729, 733)
pref_keymap_impl = slice_lines(feat_lines, 731, 790)
app_extra_fields = slice_lines(feat_lines, 1061, 1079)
app_extra_inits = [
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
net_for_selection = slice_lines(feat_lines, 3120, 3131)
dispatch_shortcut = slice_lines(feat_lines, 3133, 3300)
shortcuts_fn = slice_lines(feat_lines, 3302, 3375)
compare_methods = slice_lines(feat_lines, 3686, 3830)
file_menu_compare = slice_lines(feat_lines, 3857, 3863)
shortcuts_settings = slice_lines(feat_lines, 4789, 4980)

out = main_lines[:]

if not any(l.strip() == "mod keymap;" for l in out):
    out.insert(find_line(out, "mod layers_tree;") + 1, "mod keymap;")

if not any("use std::collections::HashMap;" in l for l in out[:80]):
    out.insert(find_line(out, "use targets::TapButton;"), "use std::collections::HashMap;")

if not any("enum CompareKind" in l for l in out):
    idx = find_line(out, "struct Preferences {")
    out[idx:idx] = compare_kind + [""]

if not any("active_keymap_profile" in l for l in out):
    idx = find_line(out, "hidden_layers: Vec<String>,")
    out[idx + 1 : idx + 1] = pref_keymap_fields

if not any("fn default_keymap_profile" in l for l in out):
    idx = find_line(out, "fn default_language()")
    out[idx:idx] = keymap_defaults + [""]

if not any("fn active_keymap(" in l for l in out):
    idx = find_line(out, "impl Default for Preferences {")
    end = close_impl_after(out, idx)
    out[end + 1 : end + 1] = [""] + pref_keymap_impl + [""]

if not any("active_keymap_profile:" in l for l in out):
    idx = find_line(out, "modifiers: Default::default(),")
    out[idx:idx] = [
        "            active_keymap_profile: default_keymap_profile(),",
        "            keymap_profiles: default_keymap_profiles(),",
    ]

if not any("binding_capture:" in l for l in out):
    idx = find_line(out, "    library: library::Browser,")
    out[idx:idx] = app_extra_fields

if not any("compare_open: false," in l for l in out):
    idx = find_line(out, "            frames: 0,")
    out[idx + 1 : idx + 1] = app_extra_inits

if "normalize_keymaps()" not in "\n".join(out):
    idx = find_line(out, 'and_then(|s| eframe::get_value(s, "preferences"))')
    idx = find_line(out, ".unwrap_or_default();", idx)
    out.insert(idx + 1, "        preferences.normalize_keymaps();")

if not any("fn dispatch_shortcut" in l for l in out):
    idx = find_line(out, "    fn shortcuts(&mut self, ctx: &egui::Context) {")
    out[idx:idx] = net_for_selection + [""] + dispatch_shortcut + [""]

s_start = find_line(out, "    fn shortcuts(&mut self, ctx: &egui::Context) {")
s_end = s_start + 1
while s_end < len(out) and not (out[s_end].startswith("    fn ") and s_end > s_start + 5):
    s_end += 1
out[s_start:s_end] = shortcuts_fn + [""]

if not any("fn run_design_compare" in l for l in out):
    idx = find_line(out, "    fn settings_tab(&mut self, ui: &mut egui::Ui) {")
    out[idx:idx] = compare_methods + [""] + shortcuts_settings + [""]

if "menu.file.compare" not in "\n".join(out):
    idx = find_line(out, '.button(t!("menu.file.export"))')
    out[idx:idx] = file_menu_compare + [""]

if "shortcuts_settings_panel(ui)" not in "\n".join(out):
    idx = find_line(out, "    fn settings_tab(&mut self, ui: &mut egui::Ui) {")
    out.insert(idx + 3, "        self.shortcuts_settings_panel(ui);")
    out.insert(idx + 4, "        ui.add_space(10.0);")

if "self.compare_window(&ctx)" not in "\n".join(out):
    idx = find_line(out, "        self.shortcuts(&ctx);")
    out.insert(idx + 1, "        if self.compare_open {")
    out.insert(idx + 2, "            self.compare_window(&ctx);")
    out.insert(idx + 3, "        }")

main_path.write_text("\n".join(out) + "\n", encoding="utf-8")
print(f"wrote {len(out)} lines")
