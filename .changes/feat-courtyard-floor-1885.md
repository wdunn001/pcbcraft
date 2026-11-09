---
type: fix
area: library
issues: [1885]
user_visible: true
docs: n/a: the command description carries the new lower bound, and no documentation file states the courtyard margin range
summary: generate_courtyard now refuses a margin smaller than the ring the courtyard check demands, so a generated courtyard cannot fail that check on sight.
---
