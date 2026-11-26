//! The atlas viewer: the cooked atlas pages, to check what the art looks like
//! as the game loaded it.

use expanse_render::AtlasTexture;

/// What the game loaded, for the overlay to show.
#[derive(Debug, Clone, Copy)]
pub struct AssetReading<'a> {
    /// The atlas pages.
    pub atlases: &'a [AtlasTexture],
    /// How many grounds and sprites the container holds.
    pub ground_count: usize,
    pub sprite_count: usize,
}

/// The viewer's state, kept between frames.
#[derive(Debug, Clone, Copy)]
pub(super) struct AtlasViewer {
    open: bool,
    page: usize,
    zoom: f32,
}

impl Default for AtlasViewer {
    fn default() -> Self {
        Self {
            open: false,
            page: 0,
            zoom: 2.0,
        }
    }
}

/// The line in the debug panel: what was loaded, and whether to show it.
pub(super) fn show_summary(ui: &mut egui::Ui, viewer: &mut AtlasViewer, assets: AssetReading<'_>) {
    ui.horizontal(|ui| {
        ui.label(format!(
            "Assets: {} grounds, {} sprites",
            assets.ground_count, assets.sprite_count
        ));
        ui.checkbox(&mut viewer.open, "Atlas");
    });
}

/// The viewer's window, if it is open.
pub(super) fn show_window(
    context: &egui::Context,
    viewer: &mut AtlasViewer,
    assets: AssetReading<'_>,
) {
    let mut open = viewer.open;
    egui::Window::new("Atlas")
        .open(&mut open)
        .default_size([640.0, 360.0])
        .show(context, |ui| {
            let page_count = assets.atlases.len();
            ui.horizontal(|ui| {
                if page_count > 1 {
                    let last = page_count - 1;
                    ui.add(egui::Slider::new(&mut viewer.page, 0..=last).text("Page"));
                }
                ui.add(
                    egui::Slider::new(&mut viewer.zoom, 1.0..=6.0)
                        .step_by(1.0)
                        .text("Zoom"),
                );
            });
            let Some(page) = assets
                .atlases
                .get(viewer.page.min(page_count.saturating_sub(1)))
            else {
                ui.label("No atlas pages.");
                return;
            };
            ui.label(format!("{} × {} px", page.size[0], page.size[1]));
            #[expect(
                clippy::cast_precision_loss,
                reason = "atlas pages are at most a few thousand pixels wide"
            )]
            let size = egui::vec2(page.size[0] as f32, page.size[1] as f32) * viewer.zoom;
            egui::ScrollArea::both().show(ui, |ui| {
                ui.add(egui::Image::new((page.texture_id, size)));
            });
        });
    viewer.open = open;
}
