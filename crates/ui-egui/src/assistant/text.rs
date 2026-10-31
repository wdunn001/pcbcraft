//! Every user-visible string of the assistant pane, as named constants.
//!
//! Context: one place to read and translate the pane's wording. When the shared text catalog
//! lands on main these constants move into it unchanged.
//! Rules: plain short sentences, no tool names of other programs, no em-dashes.

use pcbcraft_text::t;

pub(super) fn tab() -> String {
    t!("assistant.tab")
}

pub(super) fn tab_tip() -> String {
    t!("assistant.tab.tip")
}

pub(super) fn new_chat() -> String {
    t!("assistant.new_chat")
}

pub(super) fn new_chat_tip() -> String {
    t!("assistant.new_chat.tip")
}

pub(super) fn clear() -> String {
    t!("assistant.clear")
}

pub(super) fn clear_tip() -> String {
    t!("assistant.clear.tip")
}

pub(super) fn send() -> String {
    t!("assistant.send")
}

pub(super) fn send_tip() -> String {
    t!("assistant.send.tip")
}

pub(super) fn stop() -> String {
    t!("assistant.stop")
}

pub(super) fn stop_tip() -> String {
    t!("assistant.stop.tip")
}

pub(super) fn allow() -> String {
    t!("assistant.allow")
}

pub(super) fn allow_tip() -> String {
    t!("assistant.allow.tip")
}

pub(super) fn always_allow() -> String {
    t!("assistant.always_allow")
}

pub(super) fn always_allow_tip() -> String {
    t!("assistant.always_allow.tip")
}

pub(super) fn deny() -> String {
    t!("assistant.deny")
}

pub(super) fn deny_tip() -> String {
    t!("assistant.deny.tip")
}

pub(super) fn undo() -> String {
    t!("assistant.undo")
}

pub(super) fn undo_tip() -> String {
    t!("assistant.undo.tip")
}

pub(super) fn input_hint() -> String {
    t!("assistant.input_hint")
}

pub(super) fn input_tip() -> String {
    t!("assistant.input.tip")
}

pub(super) fn no_model_title() -> String {
    t!("assistant.no_model_title")
}

pub(super) fn no_model_body() -> String {
    t!("assistant.no_model_body")
}

pub(super) fn state_off() -> String {
    t!("assistant.state_off")
}

pub(super) fn state_starting() -> String {
    t!("assistant.state_starting")
}

pub(super) fn state_ready() -> String {
    t!("assistant.state_ready")
}

pub(super) fn state_working() -> String {
    t!("assistant.state_working")
}

pub(super) fn state_waiting() -> String {
    t!("assistant.state_waiting")
}

pub(super) fn state_tip() -> String {
    t!("assistant.state.tip")
}

pub(super) fn queued_tip() -> String {
    t!("assistant.queued.tip")
}

pub(super) fn denied_note() -> String {
    t!("assistant.denied_note")
}

pub(super) fn new_design_note() -> String {
    t!("assistant.new_design_note")
}

pub(super) fn no_http_body() -> String {
    t!("assistant.no_http_body")
}

pub(super) fn settings_heading() -> String {
    t!("assistant.settings_heading")
}

pub(super) fn show() -> String {
    t!("assistant.show")
}

pub(super) fn show_tip() -> String {
    t!("assistant.show.tip")
}

pub(super) fn connections() -> String {
    t!("assistant.connections")
}

pub(super) fn no_connections() -> String {
    t!("assistant.no_connections")
}

pub(super) fn add_connection() -> String {
    t!("assistant.add_connection")
}

pub(super) fn add_connection_tip() -> String {
    t!("assistant.add_connection.tip")
}

pub(super) fn test() -> String {
    t!("assistant.test")
}

pub(super) fn test_tip() -> String {
    t!("assistant.test.tip")
}

pub(super) fn make_default() -> String {
    t!("assistant.make_default")
}

pub(super) fn make_default_tip() -> String {
    t!("assistant.make_default.tip")
}

pub(super) fn in_use() -> String {
    t!("assistant.in_use")
}

pub(super) fn in_use_tip() -> String {
    t!("assistant.in_use.tip")
}

pub(super) fn remove() -> String {
    t!("assistant.remove")
}

pub(super) fn remove_tip() -> String {
    t!("assistant.remove.tip")
}

pub(super) fn key_stored() -> String {
    t!("assistant.key_stored")
}

pub(super) fn no_key() -> String {
    t!("assistant.no_key")
}

pub(super) fn permissions() -> String {
    t!("assistant.permissions")
}

pub(super) fn mode_ask_tip() -> String {
    t!("assistant.mode_ask.tip")
}

pub(super) fn mode_edits_tip() -> String {
    t!("assistant.mode_edits.tip")
}

pub(super) fn mode_all_tip() -> String {
    t!("assistant.mode_all.tip")
}

pub(super) fn mode_all_warning() -> String {
    t!("assistant.mode_all_warning")
}

pub(super) fn same_as_default() -> String {
    t!("assistant.same_as_default")
}

pub(super) fn mode_override_tip() -> String {
    t!("assistant.mode_override.tip")
}

pub(super) fn limit() -> String {
    t!("assistant.limit")
}

pub(super) fn limit_tip() -> String {
    t!("assistant.limit.tip")
}

pub(super) fn reset_always() -> String {
    t!("assistant.reset_always")
}

pub(super) fn reset_always_tip() -> String {
    t!("assistant.reset_always.tip")
}

pub(super) fn dialog_title() -> String {
    t!("assistant.dialog_title")
}

pub(super) fn preset() -> String {
    t!("assistant.preset")
}

pub(super) fn preset_tip() -> String {
    t!("assistant.preset.tip")
}

pub(super) fn name() -> String {
    t!("assistant.name")
}

pub(super) fn name_tip() -> String {
    t!("assistant.name.tip")
}

pub(super) fn url() -> String {
    t!("assistant.url")
}

pub(super) fn url_tip() -> String {
    t!("assistant.url.tip")
}

pub(super) fn model() -> String {
    t!("assistant.model")
}

pub(super) fn model_tip() -> String {
    t!("assistant.model.tip")
}

pub(super) fn key() -> String {
    t!("assistant.key")
}

pub(super) fn key_tip() -> String {
    t!("assistant.key.tip")
}

pub(super) fn key_refused() -> String {
    t!("assistant.key_refused")
}

pub(super) fn key_needs_desktop() -> String {
    t!("assistant.key_needs_desktop")
}

pub(super) fn add() -> String {
    t!("assistant.add")
}

pub(super) fn add_tip() -> String {
    t!("assistant.add.tip")
}

pub(super) fn cancel() -> String {
    t!("assistant.cancel")
}

pub(super) fn cancel_tip() -> String {
    t!("assistant.cancel.tip")
}

pub(super) fn testing() -> String {
    t!("assistant.testing")
}

pub(super) fn switch_tip() -> String {
    t!("assistant.switch.tip")
}

pub(super) fn takes_pictures() -> String {
    t!("assistant.takes_pictures")
}

pub(super) fn takes_pictures_tip() -> String {
    t!("assistant.takes_pictures.tip")
}

pub(super) fn attach() -> String {
    t!("assistant.attach")
}

pub(super) fn attach_tip() -> String {
    t!("assistant.attach.tip")
}

pub(super) fn show_sent() -> String {
    t!("assistant.show_sent")
}

pub(super) fn show_sent_tip() -> String {
    t!("assistant.show_sent.tip")
}

/// The tab label, with the count of questions waiting for an answer.
pub(super) fn tab_label(waiting: usize) -> String {
    if waiting == 0 {
        tab()
    } else {
        t!("assistant.tab.waiting", count = waiting)
    }
}

/// The result line after a successful list of models.
pub(super) fn models_found(n: usize) -> String {
    if n == 1 {
        t!("assistant.models.one")
    } else {
        t!("assistant.models.other", count = n)
    }
}

/// The activity line after the conversation was written.
pub(super) fn transcript_saved(items: usize) -> String {
    t!("assistant.transcript.saved", count = items)
}

/// Where the exported conversation went.
pub(super) fn exported(path: &str) -> String {
    t!("assistant.export.done", path = path)
}
pub(super) fn export() -> String {
    t!("assistant.export")
}
pub(super) fn export_tip() -> String {
    t!("assistant.export.tip")
}
pub(super) fn forget_question() -> String {
    t!("assistant.forget_question")
}
pub(super) fn forget() -> String {
    t!("assistant.forget")
}
pub(super) fn forget_tip() -> String {
    t!("assistant.forget.tip")
}
pub(super) fn keep() -> String {
    t!("assistant.keep")
}
pub(super) fn keep_tip() -> String {
    t!("assistant.keep.tip")
}
pub(super) fn open_settings() -> String {
    t!("assistant.open_settings")
}
pub(super) fn open_settings_tip() -> String {
    t!("assistant.open_settings.tip")
}
pub(super) fn state_stopped() -> String {
    t!("assistant.state_stopped")
}
pub(super) fn retry() -> String {
    t!("assistant.retry")
}
pub(super) fn retry_tip() -> String {
    t!("assistant.retry.tip")
}
pub(super) fn disabled_tip() -> String {
    t!("assistant.disabled.tip")
}
pub(super) fn switched_off_body() -> String {
    t!("assistant.switched_off_body")
}
pub(super) fn settings_heading_tip() -> String {
    t!("assistant.settings_heading.tip")
}

// Steps, review card, undo card and next-step cards.
pub(super) fn step_tip() -> String {
    t!("assistant.step.tip")
}

pub(super) fn step_state_waiting() -> String {
    t!("assistant.step.state_waiting")
}

pub(super) fn step_state_running() -> String {
    t!("assistant.step.state_running")
}

pub(super) fn step_state_done() -> String {
    t!("assistant.step.state_done")
}

pub(super) fn step_state_failed() -> String {
    t!("assistant.step.state_failed")
}

pub(super) fn step_quick() -> String {
    t!("assistant.step.quick")
}

pub(super) fn step_commands() -> String {
    t!("assistant.step.commands")
}

pub(super) fn step_commands_tip() -> String {
    t!("assistant.step.commands.tip")
}

pub(super) fn run_working() -> String {
    t!("assistant.run.working")
}

pub(super) fn run_working_tip() -> String {
    t!("assistant.run.working.tip")
}

pub(super) fn run_tip() -> String {
    t!("assistant.run.tip")
}

pub(super) fn digest_done() -> String {
    t!("assistant.digest.done")
}

pub(super) fn digest_nothing() -> String {
    t!("assistant.digest.nothing")
}

pub(super) fn digest_changed() -> String {
    t!("assistant.digest.changed")
}

pub(super) fn digest_removed() -> String {
    t!("assistant.digest.removed")
}

pub(super) fn digest_declined() -> String {
    t!("assistant.digest.declined")
}

pub(super) fn review_caption() -> String {
    t!("assistant.review.caption")
}

pub(super) fn review_title() -> String {
    t!("assistant.review.title")
}

pub(super) fn review_applied() -> String {
    t!("assistant.review.applied")
}

pub(super) fn review_discarded() -> String {
    t!("assistant.review.discarded")
}

pub(super) fn review_row_parts() -> String {
    t!("assistant.review.row_parts")
}

pub(super) fn review_row_nets() -> String {
    t!("assistant.review.row_nets")
}

pub(super) fn review_row_tracks() -> String {
    t!("assistant.review.row_tracks")
}

pub(super) fn review_row_vias() -> String {
    t!("assistant.review.row_vias")
}

pub(super) fn review_row_untouched() -> String {
    t!("assistant.review.row_untouched")
}

pub(super) fn review_row_size() -> String {
    t!("assistant.review.row_size")
}

pub(super) fn review_row_parts_tip() -> String {
    t!("assistant.review.row_parts.tip")
}

pub(super) fn review_row_nets_tip() -> String {
    t!("assistant.review.row_nets.tip")
}

pub(super) fn review_row_tracks_tip() -> String {
    t!("assistant.review.row_tracks.tip")
}

pub(super) fn review_row_vias_tip() -> String {
    t!("assistant.review.row_vias.tip")
}

pub(super) fn review_row_untouched_tip() -> String {
    t!("assistant.review.row_untouched.tip")
}

pub(super) fn review_row_size_tip() -> String {
    t!("assistant.review.row_size.tip")
}

pub(super) fn review_list() -> String {
    t!("assistant.review.list")
}

pub(super) fn review_list_tip() -> String {
    t!("assistant.review.list.tip")
}

pub(super) fn review_step_hint() -> String {
    t!("assistant.review.step_hint")
}

pub(super) fn apply_each() -> String {
    t!("assistant.apply_each")
}

pub(super) fn apply_each_tip() -> String {
    t!("assistant.apply_each.tip")
}

pub(super) fn undo_caption() -> String {
    t!("assistant.undo.caption")
}

pub(super) fn undo_body() -> String {
    t!("assistant.undo.body")
}

pub(super) fn undo_key() -> String {
    t!("assistant.undo.key")
}

pub(super) fn undo_done() -> String {
    t!("assistant.undo.done")
}

pub(super) fn undo_title_one() -> String {
    t!("assistant.undo.title_one")
}

pub(super) fn next_caption() -> String {
    t!("assistant.next.caption")
}

pub(super) fn next_recommended() -> String {
    t!("assistant.next.recommended")
}

pub(super) fn next_tip() -> String {
    t!("assistant.next.tip")
}

pub(super) fn next_recommended_tip() -> String {
    t!("assistant.next.recommended.tip")
}

pub(super) fn input_hint_next() -> String {
    t!("assistant.input_hint_next")
}

/// The plain name of a step kind, from the catalog key `assistant.step.<id>`.
pub(super) fn step_name(kind: pcbcraft_assistant::StepKind) -> String {
    match kind {
        pcbcraft_assistant::StepKind::Parts => t!("assistant.step.parts"),
        pcbcraft_assistant::StepKind::Placing => t!("assistant.step.placing"),
        pcbcraft_assistant::StepKind::Connecting => t!("assistant.step.connecting"),
        pcbcraft_assistant::StepKind::Routing => t!("assistant.step.routing"),
        pcbcraft_assistant::StepKind::Board => t!("assistant.step.board"),
        pcbcraft_assistant::StepKind::Checking => t!("assistant.step.checking"),
        pcbcraft_assistant::StepKind::Looking => t!("assistant.step.looking"),
        pcbcraft_assistant::StepKind::Picture => t!("assistant.step.picture"),
        pcbcraft_assistant::StepKind::Files => t!("assistant.step.files"),
        pcbcraft_assistant::StepKind::Other => t!("assistant.step.other"),
    }
}

pub(super) fn step_secs(n: u64) -> String {
    t!("assistant.step.secs", n = n)
}

pub(super) fn run_summary(n: usize, secs: &str) -> String {
    if n == 1 {
        t!("assistant.run.one", secs = secs)
    } else {
        t!("assistant.run.many", n = n, secs = secs)
    }
}

/// The sentence for a counted result, from the catalog key `assistant.digest.<id>`.
pub(super) fn digest(d: &pcbcraft_assistant::Digest) -> String {
    let (a, b) = (d.numbers[0], d.numbers[1]);
    match d.id {
        "found" => t!("assistant.digest.found", a = a),
        "checks_clean" => t!("assistant.digest.checks_clean"),
        "checks_unrouted" => t!("assistant.digest.checks_unrouted", a = a),
        _ => t!("assistant.digest.checks_found", a = a, b = b),
    }
}

pub(super) fn digest_added(list: &str) -> String {
    t!("assistant.digest.added", list = list)
}

pub(super) fn digest_failed(reason: &str) -> String {
    t!("assistant.digest.failed", reason = reason)
}

pub(super) fn count_parts(n: usize) -> String {
    if n == 1 {
        t!("assistant.digest.part_one")
    } else {
        t!("assistant.digest.part_many", n = n)
    }
}

pub(super) fn count_tracks(n: usize) -> String {
    if n == 1 {
        t!("assistant.digest.track_one")
    } else {
        t!("assistant.digest.track_many", n = n)
    }
}

pub(super) fn count_vias(n: usize) -> String {
    if n == 1 {
        t!("assistant.digest.via_one")
    } else {
        t!("assistant.digest.via_many", n = n)
    }
}

pub(super) fn count_other(n: usize) -> String {
    if n == 1 {
        t!("assistant.digest.other_one")
    } else {
        t!("assistant.digest.other_many", n = n)
    }
}

pub(super) fn review_of(a: usize, b: usize) -> String {
    t!("assistant.review.of", a = a, b = b)
}

pub(super) fn review_kb(a: &str, b: &str) -> String {
    t!("assistant.review.kb", a = a, b = b)
}

pub(super) fn review_share(n: usize) -> String {
    t!("assistant.review.share", n = n)
}

pub(super) fn review_more(n: usize) -> String {
    t!("assistant.review.more", n = n)
}

pub(super) fn review_line(
    parts: usize,
    nets: usize,
    tracks: usize,
    kept: usize,
    total: usize,
) -> String {
    t!(
        "assistant.review.line",
        parts = parts,
        nets = nets,
        tracks = tracks,
        kept = kept,
        total = total
    )
}

pub(super) fn step_n(n: usize, total: usize) -> String {
    t!("assistant.step_n", n = n, total = total)
}

pub(super) fn undo_title(n: u32) -> String {
    t!("assistant.undo.title", n = n)
}
