//! Every user-visible string of the assistant pane, as named constants.
//!
//! Context: one place to read and translate the pane's wording. When the shared text catalog
//! lands on main these constants move into it unchanged.
//! Rules: plain short sentences, no tool names of other programs, no em-dashes.

pub(super) const TAB: &str = "Assistant";
pub(super) const TAB_TIP: &str =
    "Ask a model to change the design. Every change waits for your answer.";
pub(super) const NEW_CHAT: &str = "New chat";
pub(super) const NEW_CHAT_TIP: &str = "Forget this conversation and start a fresh one.";
pub(super) const CLEAR: &str = "Clear";
pub(super) const CLEAR_TIP: &str = "Remove the messages shown here. The design is not touched.";
pub(super) const SEND: &str = "Send";
pub(super) const SEND_TIP: &str =
    "Send this message (Ctrl+Enter). While the model works it is queued.";
pub(super) const STOP: &str = "Stop";
pub(super) const STOP_TIP: &str = "Stop the model. Questions waiting for you are declined.";
pub(super) const ALLOW: &str = "Allow";
pub(super) const ALLOW_TIP: &str = "Run this change now. You can undo it.";
pub(super) const ALWAYS_ALLOW: &str = "Always allow";
pub(super) const ALWAYS_ALLOW_TIP: &str =
    "Run this kind of change without asking again in this session.";
pub(super) const DENY: &str = "Deny";
pub(super) const DENY_TIP: &str = "Do not run this change. The model is told so.";
pub(super) const UNDO: &str = "Undo";
pub(super) const UNDO_TIP: &str = "Take back the last change the model made.";
pub(super) const INPUT_HINT: &str = "Ask for a change, for example: add a 10k resistor";
pub(super) const INPUT_TIP: &str = "Type what you want changed. Ctrl+Enter sends.";
pub(super) const NO_MODEL_TITLE: &str = "No model is connected";
pub(super) const NO_MODEL_BODY: &str = "Nothing leaves this computer until you connect one. \
Open Settings, then Assistant, and enter the address and the model name.";
pub(super) const STATE_OFF: &str = "Not connected";
pub(super) const STATE_STARTING: &str = "Starting";
pub(super) const STATE_READY: &str = "Ready";
pub(super) const STATE_WORKING: &str = "Working";
pub(super) const STATE_WAITING: &str = "Waiting for you";
pub(super) const STATE_TIP: &str = "What the assistant is doing right now.";
pub(super) const QUEUED_TIP: &str = "Waiting for the model to finish its current answer.";
pub(super) const DENIED_NOTE: &str = "The user declined this change.";
pub(super) const NEW_DESIGN_NOTE: &str =
    "A different design was opened. Earlier changes belong to the old one.";

/// The tab label, with the count of questions waiting for an answer.
pub(super) fn tab_label(waiting: usize) -> String {
    if waiting == 0 {
        TAB.to_string()
    } else {
        format!("{TAB} ({waiting})")
    }
}

pub(super) const NO_HTTP_BODY: &str = "This version of the app cannot send requests to a model. \
Use the desktop app to talk to one.";
pub(super) const SETTINGS_HEADING: &str = "ASSISTANT";
pub(super) const SHOW: &str = "Show assistant";
pub(super) const SHOW_TIP: &str = "Hide every assistant control and start nothing.";
pub(super) const CONNECTIONS: &str = "Connections";
pub(super) const NO_CONNECTIONS: &str =
    "No connection yet. Nothing is sent anywhere until you add one.";
pub(super) const ADD_CONNECTION: &str = "Add connection";
pub(super) const ADD_CONNECTION_TIP: &str = "Add the address of a model server.";
pub(super) const TEST: &str = "Test";
pub(super) const TEST_TIP: &str = "Ask the server which models it offers.";
pub(super) const MAKE_DEFAULT: &str = "Use";
pub(super) const MAKE_DEFAULT_TIP: &str = "Use this connection for the chat.";
pub(super) const IN_USE: &str = "In use";
pub(super) const IN_USE_TIP: &str = "The chat uses this connection.";
pub(super) const REMOVE: &str = "Remove";
pub(super) const REMOVE_TIP: &str = "Forget this connection and delete its stored key.";
pub(super) const KEY_STORED: &str = "key stored";
pub(super) const NO_KEY: &str = "no key";
pub(super) const PERMISSIONS: &str = "What it may do without asking";
pub(super) const MODE_ASK_TIP: &str = "Every change waits for your answer.";
pub(super) const MODE_EDITS_TIP: &str =
    "Changes to the design run at once and can be undone. Files and sessions still ask.";
pub(super) const MODE_ALL_TIP: &str =
    "Everything runs without asking, including saving and opening files.";
pub(super) const MODE_ALL_WARNING: &str =
    "Allow everything lets the model save over files and replace the design. Use it with care.";
pub(super) const SAME_AS_DEFAULT: &str = "Same as above";
pub(super) const MODE_OVERRIDE_TIP: &str = "Use a different permission for this connection.";
pub(super) const LIMIT: &str = "Changes per message";
pub(super) const LIMIT_TIP: &str = "After this many changes the model is told to stop and report.";
pub(super) const RESET_ALWAYS: &str = "Forget Always allow";
pub(super) const RESET_ALWAYS_TIP: &str =
    "Ask again for every kind of change you allowed for good.";
pub(super) const DIALOG_TITLE: &str = "Add connection";
pub(super) const PRESET: &str = "Start from";
pub(super) const PRESET_TIP: &str = "Fill in the address of a common kind of server.";
pub(super) const NAME: &str = "Name";
pub(super) const NAME_TIP: &str = "How this connection is listed.";
pub(super) const URL: &str = "Address";
pub(super) const URL_TIP: &str =
    "The address of the model server, for example http://localhost:11434/v1.";
pub(super) const MODEL: &str = "Model";
pub(super) const MODEL_TIP: &str =
    "The model name to ask for. Test fills the list when the server offers one.";
pub(super) const KEY: &str = "Key";
pub(super) const KEY_TIP: &str =
    "Kept in the system keychain, never in the design or the settings file.";
pub(super) const KEY_REFUSED: &str = "A key is only sent over https, or to this computer, so the key field is hidden for this address.";
pub(super) const KEY_NEEDS_DESKTOP: &str = "Keys need the desktop app.";
pub(super) const ADD: &str = "Add";
pub(super) const ADD_TIP: &str = "Save this connection.";
pub(super) const CANCEL: &str = "Cancel";
pub(super) const CANCEL_TIP: &str = "Close without saving.";
pub(super) const TESTING: &str = "Testing";

/// The result line after a successful list of models.
pub(super) fn models_found(n: usize) -> String {
    if n == 1 {
        "Connected. 1 model.".to_string()
    } else {
        format!("Connected. {n} models.")
    }
}
pub(super) const SWITCH_TIP: &str = "Choose which connection the chat uses.";
pub(super) const TAKES_PICTURES: &str = "Model takes pictures";
pub(super) const TAKES_PICTURES_TIP: &str =
    "Turn on only if this model can read images. Attach view then sends one.";
pub(super) const ATTACH: &str = "Attach view";
pub(super) const ATTACH_TIP: &str = "Send a picture of the current view with this message.";
pub(super) const SHOW_SENT: &str = "Show what was sent";
pub(super) const SHOW_SENT_TIP: &str =
    "The design notes that went to the model with your last message.";
