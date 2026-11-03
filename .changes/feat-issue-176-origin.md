---
type: fix
area: io
issues: [176]
user_visible: true
docs: [crates/io/src/gerber_origin.rs]
summary: Every Gerber layer file now uses the board's drill origin like the drill files do, and the fabrication bundle writes mask and paste from one writer.
---
