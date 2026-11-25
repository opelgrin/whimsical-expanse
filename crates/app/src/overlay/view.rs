//! The view panel: what colours the terrain, and the obstacle layer.

use std::iter;

use expanse_render::{TerrainDisplay, TerrainView};
use expanse_terrain::Field;

use super::OverlayRequest;

pub(super) fn show(ui: &mut egui::Ui, display: TerrainDisplay, requests: &mut Vec<OverlayRequest>) {
    egui::ComboBox::from_label("View")
        .selected_text(view_name(display.view))
        .show_ui(ui, |ui| {
            let fields = Field::ALL.into_iter().map(TerrainView::Field);
            for choice in iter::once(TerrainView::Biomes).chain(fields) {
                let selected = display.view == choice;
                if ui.selectable_label(selected, view_name(choice)).clicked() && !selected {
                    requests.push(OverlayRequest::SetDisplay(TerrainDisplay {
                        view: choice,
                        ..display
                    }));
                }
            }
        });
    let mut obstacles = display.obstacles;
    if ui.checkbox(&mut obstacles, "Obstacle layer").changed() {
        requests.push(OverlayRequest::SetDisplay(TerrainDisplay {
            obstacles,
            ..display
        }));
    }
}

const fn view_name(view: TerrainView) -> &'static str {
    match view {
        TerrainView::Biomes => "Biomes",
        TerrainView::Field(field) => field_name(field),
    }
}

/// The name of `field`, in the view menu and the tile inspector.
pub(super) const fn field_name(field: Field) -> &'static str {
    match field {
        Field::Elevation => "Elevation",
        Field::WinterTemperature => "Winter temperature",
        Field::SummerTemperature => "Summer temperature",
        Field::Moisture => "Moisture",
        Field::Vegetation => "Vegetation",
    }
}
