//! Pin no-connect flag toggle: mark a pin unconnected or clear the mark.
//!
//! Why: the right-click menu and the No connect tool arm must behave identically:
//! click a free pin to flag it, click again to clear the flag, get a plain
//! sentence when the pin is wired. One fn here keeps them the same.
//!
//! # Rules
//! - Never arm a tool; flag or clear the pin under the cursor now.
//! - If the engine refuses (wired pin, duplicate), set app.message from an en.txt key.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (PinRef, NoConnectFlag)
//! - crates/engine/src/lib.rs (pin.no_connect, object.delete)
//!
//! # Do not
//! - Paint here; this is logic only, called from canvas and menu paths.

use super::*;
use pcbcraft_doc::PinRef;

impl App {
    /// Flag a pin as deliberately unconnected, or clear an existing flag.
    ///
    /// If a no-connect flag already exists on the pin it is removed. Otherwise
    /// pin.no_connect is run; if the engine refuses, a plain sentence is shown
    /// instead of the engine error text.
    ///
    /// # Rules
    /// - Leaves app.tool unchanged: never arms the No connect tool.
    pub(crate) fn toggle_no_connect(&mut self, pin: PinRef) {
        if let Some(flag) = self
            .project
            .no_connects
            .iter()
            .find(|f| f.pin.component == pin.component && f.pin.pin == pin.pin)
        {
            self.run("object.delete", json!({"id": flag.id}));
            return;
        }
        let result = self
            .session
            .lock()
            .map_err(|_| t!("dialogs.run.session_unavailable"))
            .and_then(|mut s| {
                s.execute("pin.no_connect", json!({"pin": pin}), Some(self.revision))
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(_) => {
                self.message = t!("dialogs.run.command_committed", command = "pin.no_connect");
                self.refresh();
            }
            Err(e) => {
                if e == "no-connect flag conflicts with a connected pin" {
                    self.message = t!("schematic.no_connect.connected");
                } else if e == "pin already has a no-connect flag" {
                    self.message = t!("schematic.no_connect.duplicate");
                } else {
                    self.message = e;
                }
            }
        }
    }
}
