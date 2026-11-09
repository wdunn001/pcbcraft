# Bundled fonts

| File | Family | Licence |
|---|---|---|
| IBMPlexSans-Regular.ttf, IBMPlexSans-Medium.ttf, IBMPlexSans-SemiBold.ttf | IBM Plex Sans, weights 400, 500 and 600, static instances cut from the upstream variable font | SIL Open Font License 1.1, see OFL-IBMPlex.txt |
| JetBrainsMono-Regular.ttf | JetBrains Mono Regular | SIL Open Font License 1.1, see OFL-JetBrainsMono.txt |

egui cannot select a weight from a variable font, so each weight is a separate file.
The fonts are embedded with `include_bytes!` in `crates/ui-egui/src/fonts.rs`.

## On-demand CJK fonts (not embedded)

| File | Family | Licence |
|---|---|---|
| NotoSansJP-Regular.otf, NotoSansSC-Regular.otf | Noto Sans CJK, Japanese and Simplified Chinese, Regular, unmodified subset OpenType files | SIL Open Font License 1.1, see OFL-NotoSansCJK.txt |

These are not compiled into the program. Installers copy them to a `fonts/` folder next to the program
(or `../share/pcbcraft/fonts`), and the browser build serves them as `fonts/<file>`. They load only when
the language is Japanese or Chinese, as fallback fonts behind IBM Plex. See `fonts::font_file_for`.
