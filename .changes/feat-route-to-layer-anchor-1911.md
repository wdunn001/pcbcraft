---
type: fix
area: layout
issues: [1911]
user_visible: true
docs: n/a: the behaviour change is inside route.auto and no documentation file describes how it picks an endpoint
summary: route.auto now finishes a route on the via beside a one-sided pad instead of giving up, so a net escaped to the other layer can actually be routed there.
---
