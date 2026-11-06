---
type: feature
area: doc
issues: [1646]
user_visible: true
docs: n/a: written by the worker harness, the issue text records the behaviour
summary: Model reflow profiles for reflow oven machines.
---

Reflow oven machines can now store named reflow profiles with stages. Each profile
has stages with target temperatures and times, validated against the oven's
maximum temperature.

- Added `ReflowStage` and `ReflowProfile` structs to the machine profile model
- Added `reflow.profile_add`, `reflow.profile_remove`, and `reflow.profile_list`
  engine commands
- Machines tab shows profiles for selected reflow ovens
- Engine tests cover profile add, list, remove, and validation
- E2E test covers adding and removing a profile from the UI
