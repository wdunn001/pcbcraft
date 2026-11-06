## feat(portal): report and takedown state machines

Pure transition functions for the moderation queue.

- `advance_report(state, event)`: Open->Triaged->Resolved/Dismissed->Open (reopen).
- `advance_takedown(row, event, now, wait)`: Notice->Removed->CounterNotice->Restored (after wait) -> Closed.
- Time is passed by the caller as Unix seconds; no clock dependency.
- Tests cover legal walks and illegal transitions.
