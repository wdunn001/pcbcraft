//! Keyboard and screen-reader support for the application shell.
//!
//! Three small pieces live here so `lib.rs` only needs thin hooks:
//! accessible names for controls that show no text, a visible focus ring
//! for keyboard users, and the interface scale preference.

use egui::{Color32, Context, Id, LayerId, Order, Response, Stroke, StrokeKind};

/// Smallest interface scale the settings accept.
pub(crate) const MIN_UI_SCALE: f32 = 0.75;
/// Largest interface scale the settings accept.
pub(crate) const MAX_UI_SCALE: f32 = 2.0;
/// Scale the app starts with.
pub(crate) const DEFAULT_UI_SCALE: f32 = 1.0;
/// Steps offered in Settings, as (label, scale).
pub(crate) const UI_SCALE_STEPS: [(&str, f32); 5] = [
    ("80%", 0.8),
    ("100%", 1.0),
    ("125%", 1.25),
    ("150%", 1.5),
    ("200%", 2.0),
];

/// Bring any stored or typed scale into the supported range; junk becomes
/// the default.
pub(crate) fn clamp_ui_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE)
    } else {
        DEFAULT_UI_SCALE
    }
}

/// Give a control the name a screen reader announces. Use it for controls
/// whose visible face is a glyph, an abbreviation or a bare symbol.
pub(crate) fn name_control(response: &Response, name: &str, selected: Option<bool>) {
    let enabled = response.enabled();
    response.widget_info(|| match selected {
        Some(on) => egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name),
        None => egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name),
    });
}

/// Keep the egui zoom factor and the saved scale in step. A change made
/// with the keyboard zoom keys is adopted into the preference; a change
/// made in Settings is pushed to egui.
pub(crate) fn sync_ui_scale(ctx: &Context, scale: &mut f32) {
    let key = Id::new("a11y-applied-ui-scale");
    let zoom = ctx.zoom_factor();
    let last = ctx.data(|d| d.get_temp::<f32>(key)).unwrap_or(zoom);
    *scale = clamp_ui_scale(*scale);
    let applied = if (zoom - last).abs() > 1e-4 {
        // Zoomed from the keyboard since the last frame.
        clamp_ui_scale(zoom)
    } else {
        *scale
    };
    *scale = applied;
    if (zoom - applied).abs() > 1e-4 {
        ctx.set_zoom_factor(applied);
    }
    ctx.data_mut(|d| d.insert_temp(key, applied));
}

/// Draw a clear ring around the widget that has keyboard focus. Stock
/// egui widgets only shade a focused control; this makes the position
/// obvious on every dock, menu and tab.
pub(crate) fn paint_focus_ring(ctx: &Context, color: Color32) {
    let Some(id) = ctx.memory(|m| m.focused()) else {
        return;
    };
    let Some(response) = ctx.read_response(id) else {
        return;
    };
    if !response.rect.is_finite() || response.rect.width() < 2.0 || response.rect.height() < 2.0 {
        return;
    }
    ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("a11y-focus-ring")))
        .rect_stroke(
            response.rect.expand(2.0),
            egui::CornerRadius::same(5),
            Stroke::new(2.0, color),
            StrokeKind::Outside,
        );
}

#[cfg(test)]
pub(crate) mod audit {
    use super::*;
    use egui::accesskit::Role;

    /// Roles a person can act on; each must carry a name. Combo boxes are
    /// left out until each one gets a label (a child issue tracks them): egui
    /// announces their value but not their purpose.
    fn needs_name(role: Role) -> bool {
        matches!(
            role,
            Role::Button | Role::CheckBox | Role::RadioButton | Role::Link | Role::Tab
        )
    }

    /// Every interactive node in this frame's accessibility tree, as
    /// (role, name). The name is empty when the control announces nothing.
    pub(crate) fn interactive_nodes(output: &egui::FullOutput) -> Vec<(Role, String)> {
        let Some(update) = &output.platform_output.accesskit_update else {
            return Vec::new();
        };
        update
            .nodes
            .iter()
            .filter(|(_, node)| needs_name(node.role()))
            .map(|(_, node)| {
                (
                    node.role(),
                    node.label().unwrap_or_default().trim().to_owned(),
                )
            })
            .collect()
    }

    /// Roles of interactive nodes with an empty name.
    pub(crate) fn unnamed(output: &egui::FullOutput) -> Vec<Role> {
        interactive_nodes(output)
            .into_iter()
            .filter(|(_, name)| name.is_empty())
            .map(|(role, _)| role)
            .collect()
    }

    /// The name of whatever holds keyboard focus after this frame.
    pub(crate) fn focused_name(ctx: &Context, output: &egui::FullOutput) -> Option<String> {
        let id = ctx.memory(|m| m.focused())?;
        let update = output.platform_output.accesskit_update.as_ref()?;
        let target = id.accesskit_id();
        update
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == target)
            .and_then(|(_, node)| node.label().map(str::to_owned))
    }
}
