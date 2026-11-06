//! Legal transitions for reports and takedowns. Pure functions; the caller
//! provides time as Unix seconds. No side effects.

use crate::api::UploadQueueState;
use crate::error::PortalError;
use crate::schema::{
    DecisionLogRow, DecisionSubject, ModerationAction, ReportState, TakedownRow, TakedownState,
    TrustTier,
};

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
    /// Someone has looked at the report and will decide.
    Triage,
    /// The report was acted on and is finished.
    Resolve,
    /// The report was reviewed and needs no action.
    Dismiss,
    /// A finished report needs another look.
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
    /// The material is taken down.
    RemoveMaterial,
    /// The owner of the material disputes the takedown.
    CounterNotice,
    /// The material is put back.
    Restore,
    /// The takedown is finished.
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

/// Outcome of applying a moderation decision to an upload in the queue.
///
/// # Rules
/// - queue reflects the upload state after the decision.
/// - publisher_tier reflects any tier change from approve-and-trust.
/// - log is the decision row to write to the decision log.
///
/// # Dependencies
/// - crates/portal/src/api.rs
/// - crates/portal/src/schema.rs
///
/// # Do not
/// - Mutate the queue or tier in place; return new values.
pub struct ModerationOutcome {
    /// The state of the upload after the decision.
    pub queue: UploadQueueState,
    /// The publisher's trust tier after the decision.
    pub publisher_tier: TrustTier,
    /// The row to write to the decision log.
    pub log: DecisionLogRow,
}

/// What a moderator supplies with a decision.
///
/// # Rules
/// - `moderator_account_id` and `log_id` are 1..=64 bytes.
/// - `note` is at most 2000 bytes and is required for a request for changes.
/// - `now` is Unix seconds from the caller.
///
/// # Dependencies
/// - crates/portal/src/schema.rs
///
/// # Do not
/// - Read the clock here; the caller passes `now`.
pub struct ModerationDecision<'a> {
    /// The moderator's account id.
    pub moderator_account_id: &'a str,
    /// The moderator's note.
    pub note: &'a str,
    /// The decision time, in Unix seconds.
    pub now: u64,
    /// The id of the decision log row to write.
    pub log_id: &'a str,
}

/// Apply a moderator's decision to an upload in the queue and return the
/// updated queue state, publisher tier and the decision log row.
///
/// # Rules
/// - Only a PendingReview upload may be decided; otherwise Conflict.
/// - Approve moves to Published and leaves the tier unchanged.
/// - ApproveAndTrust moves to Published and raises New to Standard or
///   Standard to Trusted (Trusted is a ceiling).
/// - Reject moves to Rejected and leaves the tier unchanged.
/// - RequestChanges leaves the upload in PendingReview and requires a note.
/// - The note must be at most 2000 bytes.
/// - upload_id, moderator_account_id and log_id must be 1..=64 bytes.
/// - The log row always records the action, upload subject, note and now.
///
/// # Dependencies
/// - crates/portal/src/api.rs
/// - crates/portal/src/schema.rs
/// - crates/portal/src/error.rs
///
/// # Do not
/// - Read the clock; the caller passes now as Unix seconds.
pub fn apply_moderation(
    upload_id: &str,
    current: UploadQueueState,
    tier: TrustTier,
    action: ModerationAction,
    decision: ModerationDecision<'_>,
) -> Result<ModerationOutcome, PortalError> {
    let ModerationDecision {
        moderator_account_id,
        note,
        now,
        log_id,
    } = decision;
    if upload_id.is_empty() || upload_id.len() > 64 {
        return Err(PortalError::Invalid(
            "upload_id must be 1..=64 bytes".into(),
        ));
    }
    if moderator_account_id.is_empty() || moderator_account_id.len() > 64 {
        return Err(PortalError::Invalid(
            "moderator_account_id must be 1..=64 bytes".into(),
        ));
    }
    if log_id.is_empty() || log_id.len() > 64 {
        return Err(PortalError::Invalid("log_id must be 1..=64 bytes".into()));
    }
    if note.len() > 2000 {
        return Err(PortalError::Invalid(
            "note must be at most 2000 bytes".into(),
        ));
    }
    if current != UploadQueueState::PendingReview {
        return Err(PortalError::Conflict(
            "only a pending review upload may be decided".into(),
        ));
    }
    let (new_queue, new_tier) = match action {
        ModerationAction::Approve => (UploadQueueState::Published, tier),
        ModerationAction::ApproveAndTrust => {
            let promoted = match tier {
                TrustTier::New => TrustTier::Standard,
                TrustTier::Standard => TrustTier::Trusted,
                TrustTier::Trusted => TrustTier::Trusted,
            };
            (UploadQueueState::Published, promoted)
        }
        ModerationAction::Reject => (UploadQueueState::Rejected, tier),
        ModerationAction::RequestChanges => {
            if note.is_empty() {
                return Err(PortalError::Invalid(
                    "request changes requires a note".into(),
                ));
            }
            (UploadQueueState::PendingReview, tier)
        }
    };
    let log = DecisionLogRow {
        id: log_id.to_string(),
        subject: DecisionSubject::Upload {
            upload_id: upload_id.to_string(),
        },
        action,
        moderator_account_id: moderator_account_id.to_string(),
        note: note.to_string(),
        created_at: now,
    };
    Ok(ModerationOutcome {
        queue: new_queue,
        publisher_tier: new_tier,
        log,
    })
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
    fn a_report_walks_open_triaged_resolved() -> Result<(), PortalError> {
        assert_eq!(
            advance_report(ReportState::Open, ReportEvent::Triage)?,
            ReportState::Triaged
        );
        assert_eq!(
            advance_report(ReportState::Triaged, ReportEvent::Resolve)?,
            ReportState::Resolved
        );
        Ok(())
    }

    #[test]
    fn a_resolved_report_cannot_be_triaged() {
        assert!(advance_report(ReportState::Resolved, ReportEvent::Triage).is_err());
    }

    #[test]
    fn a_counter_notice_restores_after_the_wait() -> Result<(), PortalError> {
        let row = advance_takedown(&new_row("t1"), TakedownEvent::RemoveMaterial, 1000, 864000)?;
        let row = advance_takedown(&row, TakedownEvent::CounterNotice, 1000, 864000)?;
        assert_eq!(row.state, TakedownState::CounterNoticePending);
        assert_eq!(row.counter_notice_at, Some(1000));
        let row = advance_takedown(&row, TakedownEvent::Restore, 865000, 864000)?;
        assert_eq!(row.state, TakedownState::Restored);
        assert_eq!(row.restored_at, Some(865000));
        let too_soon = advance_takedown(&row, TakedownEvent::Restore, 864999, 864000);
        assert!(too_soon.is_err());
        Ok(())
    }

    #[test]
    fn a_takedown_cannot_skip_removal() {
        let row = new_row("t2");
        assert!(advance_takedown(&row, TakedownEvent::CounterNotice, 1000, 864000).is_err());
    }

    #[test]
    fn approve_and_trust_raises_the_tier_and_logs_it() -> Result<(), PortalError> {
        let outcome = apply_moderation(
            "upload-1",
            UploadQueueState::PendingReview,
            TrustTier::New,
            ModerationAction::ApproveAndTrust,
            ModerationDecision {
                moderator_account_id: "mod-1",
                note: "looks good",
                now: 1000,
                log_id: "log-1",
            },
        )?;
        assert_eq!(outcome.queue, UploadQueueState::Published);
        assert_eq!(outcome.publisher_tier, TrustTier::Standard);
        assert_eq!(outcome.log.action, ModerationAction::ApproveAndTrust);
        assert_eq!(outcome.log.created_at, 1000);
        match &outcome.log.subject {
            DecisionSubject::Upload { upload_id } => {
                assert_eq!(upload_id, "upload-1");
            }
            _ => {
                return Err(PortalError::Invalid("wrong subject".into()));
            }
        }
        Ok(())
    }

    #[test]
    fn a_published_upload_cannot_be_decided_again() {
        let result = apply_moderation(
            "upload-1",
            UploadQueueState::Published,
            TrustTier::New,
            ModerationAction::Approve,
            ModerationDecision {
                moderator_account_id: "mod-1",
                note: "again",
                now: 1000,
                log_id: "log-1",
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn request_changes_needs_a_note() {
        let result = apply_moderation(
            "upload-1",
            UploadQueueState::PendingReview,
            TrustTier::New,
            ModerationAction::RequestChanges,
            ModerationDecision {
                moderator_account_id: "mod-1",
                note: "",
                now: 1000,
                log_id: "log-1",
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn reject_keeps_the_tier() -> Result<(), PortalError> {
        let outcome = apply_moderation(
            "upload-1",
            UploadQueueState::PendingReview,
            TrustTier::Trusted,
            ModerationAction::Reject,
            ModerationDecision {
                moderator_account_id: "mod-1",
                note: "not ready",
                now: 1000,
                log_id: "log-1",
            },
        )?;
        assert_eq!(outcome.queue, UploadQueueState::Rejected);
        assert_eq!(outcome.publisher_tier, TrustTier::Trusted);
        Ok(())
    }
}
