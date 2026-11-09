//! Reduced motion: the Settings choice and the operating system hint.
//!
//! Context: WCAG 2.2 asks that motion the user did not start can be turned off.
//! The app has no timed animation of its own; the motion is egui's fade and
//! collapse time (`animation_time`) and smoothed wheel scrolling, which makes
//! a zoom glide over several frames. Dragging is essential motion and stays.
//! Local rules: read the setting from `Preferences::reduced_motion` only.
use egui::Ui;

/// Whether the computer asks for less motion; false when it cannot be told.
///
/// Why: a first-time user who set the system to reduce motion should not have
/// to find the same switch again here.
///
/// # Rules
/// - Unknown is false. Only Linux with the GNOME setting is read today; the
///   reading is skipped in tests so they never depend on the machine.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` `Preferences::default` calls it once for a new profile.
///
/// # Do not
/// - Call it every frame; it starts a process.
pub(crate) fn os_prefers_reduced_motion() -> bool {
    #[cfg(all(target_os = "linux", not(test)))]
    {
        if let Ok(out) = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "enable-animations"])
            .output()
        {
            return String::from_utf8_lossy(&out.stdout).trim() == "false";
        }
    }
    false
}

/// Points one wheel line is worth, egui's own default for `line_scroll_speed`.
const LINE_SCROLL_POINTS: f32 = 40.0;

/// The vertical wheel amount to zoom by this frame.
///
/// Why: egui spreads a wheel notch over several frames; with reduced motion the
/// whole notch lands in one frame, so a zoom step reaches its target at once.
///
/// # Rules
/// - Same units and sign as `smooth_scroll_delta`.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas.rs` board zoom.
/// - `crates/ui-egui/src/authoring.rs` library editor zoom.
///
/// # Do not
/// - Use it for dragging or panning; those follow the pointer anyway.
pub(crate) fn wheel_zoom_delta(ui: &Ui, reduced: bool) -> f32 {
    ui.input(|i| {
        if reduced {
            let page = i.content_rect().height();
            i.raw
                .events
                .iter()
                .map(|event| match event {
                    egui::Event::MouseWheel { unit, delta, .. } => match unit {
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Line => delta.y * LINE_SCROLL_POINTS,
                        egui::MouseWheelUnit::Page => delta.y * page,
                    },
                    _ => 0.0,
                })
                .sum()
        } else {
            i.smooth_scroll_delta.y
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::Preferences;

    #[test]
    fn reduced_motion_survives_a_preferences_round_trip() -> Result<(), String> {
        let mut prefs = Preferences::default();
        assert!(
            !prefs.reduced_motion,
            "default is off when the system is silent"
        );
        prefs.reduced_motion = true;
        let text = serde_json::to_string(&prefs).map_err(|e| e.to_string())?;
        let back: Preferences = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert!(back.reduced_motion);
        // A profile saved before the setting existed loads with it off.
        let mut value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| e.to_string())?;
        value
            .as_object_mut()
            .ok_or("not an object")?
            .remove("reduced_motion");
        let old: Preferences = serde_json::from_value(value).map_err(|e| e.to_string())?;
        assert!(!old.reduced_motion);
        Ok(())
    }
}
