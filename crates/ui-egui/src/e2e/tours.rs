//! Help > Tours lists every shipped tour by its catalog title.

use super::super::authoring;
use super::support::*;

#[test]
fn a_newcomer_opens_Help_Tours_and_sees_every_tour_listed() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Help")?;
    authoring::tests::tap(&ctx, &mut app, "Tours")?;
    for tour in &app.tours.tour {
        let title = pcbcraft_text::lookup(&tour.list_key(), &[]);
        assert!(
            paints_text(&ctx, &mut app, &title)?,
            "Help > Tours lists {title}"
        );
    }
    Ok(())
}
