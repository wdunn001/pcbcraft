//! Legal transitions for reports and takedowns. Pure functions; the caller
//! provides time as Unix seconds. No side effects.

use crate::error::PortalError;
use crate::schema::{ReportState, TakedownRow, TakedownState};

/// An event that moves a report between states.
///
/// # Rules
/// - Open -> Triaged via Triage.
/// - Open or Triaged -> Resolved via Resolve.
/// - Open or Triaged -> Dismissed via Dismiss.
/// - Resolved or Dismissed -> Open via Reopen.
/// - Any other combination is a Conflict.
///
/// # Dependencies
/// - crates/portal/src/schema.rs
/// - crates/portal/src/error.rs
///
/// # Do not
/// - Add new events without documenting their transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportEvent {
    Triage,
    Resolve,
    Dismiss,
    Reopen,
}

/// Advance a report's state machine given the current state and an event.
///
/// # Rules
/// - Open -> Triaged via Triage.
/// - Open or Triaged -> Resolved via Resolve.
/// - Open or Triaged -> Dismissed via Dismiss.
/// - Resolved or Dismissed -> Open via Reopen.
/// - Any other combination returns Conflict.
///
/// # Dependencies
/// - crates/portal/src/schema.rs
/// - crates/portal/src/error.rs
///
/// # Do not
/// - Return the same state on an illegal event; return an error instead.
pub fn advance_report(state: ReportState, event: ReportEvent) -> Result<ReportState, PortalError> {
    match (&state, event) {
        (ReportState::Open, ReportEvent::Triage) => Ok(ReportState::Triaged),
        (ReportState::Open, ReportEvent::Resolve) => Ok(ReportState::Resolved),
        (ReportState::Triaged, ReportEvent::Resolve) => Ok(ReportState::Resolved),
        (ReportState::Open, ReportEvent::Dismiss) => Ok(ReportState::Dismissed),
        (ReportState::Triaged, ReportEvent::Dismiss) => Ok(ReportState::Dismissed),
        (ReportState::Resolved, ReportEvent::Reopen) => Ok(ReportState::Open),
        (ReportState::Dismissed, ReportEvent::Reopen) => Ok(ReportState::Open),
        _ => Err(PortalError::Conflict(
            "report transition not allowed".into(),
        )),
    }
}

/// An event that moves a takedown between states.
///
/// # Rules
/// - NoticePending -> MaterialRemoved via RemoveMaterial.
/// - MaterialRemoved -> CounterNoticePending via CounterNotice.
/// - CounterNoticePending -> Restored via Restore (only after the wait).
/// - MaterialRemoved or Restored -> Closed via Close.
/// - Any other combination is a Conflict.
///
/// # Dependencies
/// - crates/portal/src/schema.rs
/// - crates/portal/src/error.rs
///
/// # Do not
/// - Add new events without documenting their transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakedownEvent {
    RemoveMaterial,
    CounterNotice,
    Restore,
    Close,
}

/// Advance a takedown's state machine given the current row, event, and now.
///
/// The restore wait is enforced: a takedown in CounterNoticePending may only be
/// restored after restore_wait_secs have elapsed since counter_notice_at.
///
/// # Rules
/// - NoticePending -> MaterialRemoved via RemoveMaterial.
/// - MaterialRemoved -> CounterNoticePending via CounterNotice.
/// - CounterNoticePending -> Restored via Restore, only after the wait.
/// - MaterialRemoved or Restored -> Closed via Close.
/// - Any other combination is a Conflict.
///
/// # Dependencies
/// - crates/portal/src/schema.rs
/// - crates/portal/src/error.rs
///
/// # Do not
/// - Read the clock inside this function; the caller passes now.
pub fn advance_takedown(
    row: &TakedownRow,
    event: TakedownEvent,
    now: u64,
    restore_wait_secs: u64,
) -> Result<TakedownRow, PortalError> {
    let mut updated = row.clone();
    match (&row.state, event) {
        (TakedownState::NoticePending, TakedownEvent::RemoveMaterial) => {
            updated.state = TakedownState::MaterialRemoved;
        }
        (TakedownState::MaterialRemoved, TakedownEvent::CounterNotice) => {
            updated.state = TakedownState::CounterNoticePending;
            updated.counter_notice_at = Some(now);
        }
        (TakedownState::CounterNoticePending, TakedownEvent::Restore) => {
            let wait_end = row
                .counter_notice_at
                .ok_or(PortalError::Conflict("no counter-notice timestamp".into()))?
                + restore_wait_secs;
            if now < wait_end {
                return Err(PortalError::Forbidden(
                    "the counter-notice wait has not ended".into(),
                ));
            }
            updated.state = TakedownState::Restored;
            updated.restored_at = Some(now);
        }
        (TakedownState::MaterialRemoved, TakedownEvent::Close)
        | (TakedownState::Restored, TakedownEvent::Close) => {
            updated.state = TakedownState::Closed;
        }
        _ => {
            return Err(PortalError::Conflict(
                "takedown transition not allowed".into(),
            ));
        }
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_row(id: &str) -> TakedownRow {
        TakedownRow {
            id: id.into(),
            publication_id: "pub-1".into(),
            state: TakedownState::NoticePending,
            notice_received_at: 1000,
            counter_notice_at: None,
            restored_at: None,
        }
    }

    #[test]
    fn a_report_walks_open_triaged_resolved() {
        assert_eq!(
            advance_report(ReportState::Open, ReportEvent::Triage).unwrap(),
            ReportState::Triaged
        );
        assert_eq!(
            advance_report(ReportState::Triaged, ReportEvent::Resolve).unwrap(),
            ReportState::Resolved
        );
    }

    #[test]
    fn a_resolved_report_cannot_be_triaged() {
        assert!(advance_report(ReportState::Resolved, ReportEvent::Triage).is_err());
    }

    #[test]
    fn a_counter_notice_restores_after_the_wait() {
        let row =
            advance_takedown(&new_row("t1"), TakedownEvent::RemoveMaterial, 1000, 864000).unwrap();
        let row = advance_takedown(&row, TakedownEvent::CounterNotice, 1000, 864000).unwrap();
        assert_eq!(row.state, TakedownState::CounterNoticePending);
        assert_eq!(row.counter_notice_at, Some(1000));
        let row = advance_takedown(&row, TakedownEvent::Restore, 865000, 864000).unwrap();
        assert_eq!(row.state, TakedownState::Restored);
        assert_eq!(row.restored_at, Some(865000));
        let too_soon = advance_takedown(&row, TakedownEvent::Restore, 864999, 864000);
        assert!(too_soon.is_err());
    }

    #[test]
    fn a_takedown_cannot_skip_removal() {
        let row = new_row("t2");
        assert!(advance_takedown(&row, TakedownEvent::CounterNotice, 1000, 864000).is_err());
    }
}
