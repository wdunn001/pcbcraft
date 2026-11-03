---
type: fix
area: io
issues: [176]
user_visible: true
docs: [crates/io/src/gerber.rs, crates/io/src/gerber_preset.rs]
summary: Gerber files keep the design coordinates on every layer, so bottom copper, mask and paste line up with the drill files, and boards with inner layers export each inner layer once, with its pours.
---
