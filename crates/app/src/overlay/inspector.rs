//! The inspector panel: what the terrain holds at the tile under the cursor,
//! from the [tile inspector](crate::inspector).

use expanse_terrain::{BiomeTable, Field};

use super::view::field_name;
use crate::inspector::{ObstacleReport, TileReport};

pub(super) fn show(ui: &mut egui::Ui, inspected: Option<&TileReport>, table: &BiomeTable) {
    let Some(report) = inspected else {
        ui.label("No tile under the cursor");
        return;
    };
    egui::Grid::new("tile-inspector")
        .num_columns(2)
        .show(ui, |ui| {
            let mut row = |name: &str, value: String| {
                ui.label(name);
                ui.label(value);
                ui.end_row();
            };
            row("Tile", format!("{}, {}", report.tile.x, report.tile.y));
            row("Chunk", format!("{}, {}", report.chunk.x, report.chunk.y));
            if let Some(latitude) = report.latitude {
                row("Latitude", format_latitude(latitude.degrees()));
            }
            row("Biome", table.biome(report.biome).name().to_owned());
            for field in Field::ALL {
                let value = field.to_unit(report.fields.get(field));
                row(field_name(field), format_field(field, value));
            }
            let obstacle = match report.obstacle {
                ObstacleReport::None => "none".to_owned(),
                ObstacleReport::Anchor(kind) => {
                    format!("{} (anchor)", table.obstacle_kind(kind).name())
                }
                ObstacleReport::Part { kind, anchor } => format!(
                    "{}, anchored at {}, {}",
                    kind.map_or("unknown", |kind| table.obstacle_kind(kind).name()),
                    anchor.x,
                    anchor.y
                ),
            };
            row("Obstacle", obstacle);
        });
}

/// A field's value in its unit, as the biome table writes its ranges.
fn format_field(field: Field, value: f64) -> String {
    match field {
        Field::Elevation => format!("{value:+.3}"),
        Field::WinterTemperature | Field::SummerTemperature => format!("{value:.1} °C"),
        Field::Moisture | Field::Vegetation => format!("{value:.3}"),
    }
}

fn format_latitude(degrees: f64) -> String {
    let hemisphere = if degrees < 0.0 { 'S' } else { 'N' };
    format!("{:.2}° {hemisphere}", degrees.abs())
}
