# Bundled fonts

| File | Family | Licence |
|---|---|---|
| IBMPlexSans-Regular.ttf, IBMPlexSans-Medium.ttf, IBMPlexSans-SemiBold.ttf | IBM Plex Sans, weights 400, 500 and 600, static instances cut from the upstream variable font | SIL Open Font License 1.1, see OFL-IBMPlex.txt |
| JetBrainsMono-Regular.ttf | JetBrains Mono Regular | SIL Open Font License 1.1, see OFL-JetBrainsMono.txt |

egui cannot select a weight from a variable font, so each weight is a separate file.
The fonts are embedded with `include_bytes!` in `crates/ui-egui/src/fonts.rs`.
