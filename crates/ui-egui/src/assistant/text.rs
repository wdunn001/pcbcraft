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
