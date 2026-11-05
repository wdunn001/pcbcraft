---
type: fix
area: library
issues: [946]
user_visible: true
docs: n/a: the part search takes the same words as before and the change only keeps single digits in them
summary: Part search keeps single digits in the need, so a search for a 5 V regulator finds the 5 V part and not the 3.3 V one, and a test set of stated needs checks it.
---
