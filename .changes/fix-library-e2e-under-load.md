---
type: test
area: library
issues: []
user_visible: false
docs: n/a: test-only change to how stories wait for the library worker thread; no behaviour or documented feature changes
summary: Library, part search and 3D model stories now wait for the worker thread's answer instead of counting frames, so they stop failing on busy machines.
---
