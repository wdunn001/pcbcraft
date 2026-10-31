//! 3D model placement editor for project-library footprints.
use crate::App;
use egui::{Color32, Pos2, Sense, Stroke, Vec2};
use pcbcraft_library::FootprintDefinition;
use pcbcraft_text::t;
use serde_json::json;

pub(super) fn project_footprint_name(project: &pcbcraft_doc::Project, name: &str) -> bool {
    project
        .library
        .footprints
        .iter()
        .any(|f| f.name == name && f.model.is_some())
}

fn sync_model_from_project(app: &mut App, footprint_name: &str, draft: &mut FootprintDefinition) {
    if let Some(live) = app
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name == footprint_name)
        .and_then(|f| f.model.clone())
    {
        draft.model = Some(live);
    }
}

fn triple_row(ui: &mut egui::Ui, label: &str, values: &mut [f64; 3], suffix: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
            let r = ui.add(crate::number_entry::localized_active(
                egui::DragValue::new(&mut values[i])
                    .speed(0.05)
                    .suffix(format!(" {suffix}")),
                3,
            ));
            if r.changed() {
                changed = true;
            }
            r.named(format!("{label} {axis}"));
        }
    });
    changed
}

fn run_transform(app: &mut App, name: &str, params: serde_json::Value) {
    app.run("library.footprint.set_model_transform", params);
}

fn drag_plane(
    ui: &mut egui::Ui,
    title: &str,
    tip: &str,
    horizontal: f64,
    vertical: f64,
    on_drag: &mut dyn FnMut(f64, f64),
) {
    ui.label(title);
    let size = Vec2::new(ui.available_width().max(120.0), 96.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    response.on_hover_text(tip);
    let painter = ui.painter_at(rect);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_gray(90)),
        egui::StrokeKind::Inside,
    );
    let mid = rect.center();
    let scale = (rect.width().min(rect.height()) * 0.35 / 5.0).max(4.0);
    let marker = Pos2::new(
        mid.x + (horizontal as f32) * scale,
        mid.y - (vertical as f32) * scale,
    );
    painter.circle_filled(marker, 5.0, Color32::from_rgb(120, 180, 255));
    painter.line_segment([mid, marker], Stroke::new(1.0, Color32::from_gray(140)));
    if response.dragged() {
        let delta = response.drag_delta();
        on_drag(
            horizontal + (delta.x as f64) / (scale as f64),
            vertical - (delta.y as f64) / (scale as f64),
        );
    }
}

pub(super) fn footprint_model_transform_panel(
    ui: &mut egui::Ui,
    app: &mut App,
    footprint_name: &str,
    def: &mut FootprintDefinition,
) {
    let Some(model) = def.model.as_mut() else {
        ui.label(t!("library.model_transform.no_model"));
        return;
    };
    if !project_footprint_name(&app.project, footprint_name) {
        ui.label(t!("library.model_transform.save_first"));
        return;
    }

    ui.heading(t!("library.model_transform.heading"));
    ui.label(t!("library.model_transform.hint"));

    let mut offset = model.offset;
    let mut scale = model.scale;
    let mut rotate = model.rotate;

    let offset_changed = triple_row(ui, &t!("library.model_transform.offset"), &mut offset, "mm");
    let scale_changed = triple_row(ui, &t!("library.model_transform.scale"), &mut scale, "");
    let rotate_changed =
        triple_row(ui, &t!("library.model_transform.rotate"), &mut rotate, "°");

    if offset_changed || scale_changed || rotate_changed {
        run_transform(
            app,
            footprint_name,
            json!({
                "name": footprint_name,
                "offset": offset,
                "scale": scale,
                "rotate": rotate,
            }),
        );
        sync_model_from_project(app, footprint_name, def);
        return;
    }

    ui.horizontal(|ui| {
        if ui
            .button(t!("library.model_transform.rotate_90"))
            .on_hover_text(t!("library.model_transform.rotate_90.tip"))
            .clicked()
        {
            run_transform(
                app,
                footprint_name,
                json!({"name": footprint_name, "preset": "rotate_90"}),
            );
        }
        if ui
            .button(t!("library.model_transform.mirror"))
            .on_hover_text(t!("library.model_transform.mirror.tip"))
            .clicked()
        {
            run_transform(
                app,
                footprint_name,
                json!({"name": footprint_name, "preset": "mirror"}),
            );
        }
        if ui
            .button(t!("library.model_transform.pin1_cycle"))
            .on_hover_text(t!("library.model_transform.pin1_cycle.tip"))
            .clicked()
        {
            run_transform(
                app,
                footprint_name,
                json!({"name": footprint_name, "preset": "pin1_cycle"}),
            );
        }
    });

    ui.separator();
    ui.label(t!("library.model_transform.drag_views"));
    let mut drag_commit = None;
    drag_plane(
        ui,
        &t!("library.model_transform.top_view"),
        &t!("library.model_transform.top_view.tip"),
        model.offset[0],
        model.offset[1],
        &mut |x, y| drag_commit = Some([x, y, model.offset[2]]),
    );
    if let Some(offset) = drag_commit {
        run_transform(
            app,
            footprint_name,
            json!({"name": footprint_name, "offset": offset}),
        );
        sync_model_from_project(app, footprint_name, def);
        return;
    }
    drag_plane(
        ui,
        &t!("library.model_transform.side_view"),
        &t!("library.model_transform.side_view.tip"),
        model.offset[1],
        model.offset[2],
        &mut |y, z| {
            drag_commit = Some([model.offset[0], y, z]);
        },
    );
    if let Some(offset) = drag_commit {
        run_transform(
            app,
            footprint_name,
            json!({"name": footprint_name, "offset": offset}),
        );
        sync_model_from_project(app, footprint_name, def);
    }

    ui.label(
        egui::RichText::new(model.path.clone())
            .small()
            .color(Color32::GRAY),
    );
}
