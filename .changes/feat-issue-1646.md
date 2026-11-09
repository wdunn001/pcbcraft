---
type: feature
area: doc
issues: [1646]
user_visible: false
docs: n/a: written by the worker harness, the issue text records the behaviour
summary: Reflow oven machines can hold named reflow profiles, with commands to add, remove and list them and limits on stage temperature and time.
---

Reflow oven machines can now store named reflow profiles with stages. Each profile
has stages with target temperatures and times, validated against the oven's
maximum temperature.

- Added `ReflowStage` and `ReflowProfile` structs to the machine profile model
- Added `reflow.profile_add`, `reflow.profile_remove`, and `reflow.profile_list`
  engine commands
- Engine tests cover profile add, list, remove, and validation
