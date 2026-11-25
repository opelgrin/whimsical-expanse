//! The generation panel: the settings of the next world, and regeneration.

use std::ops::RangeInclusive;

use egui::Color32;
use expanse_worldgen::SurfaceParams;

use super::OverlayRequest;

/// What the overlay lets the developer change when starting a new world: the
/// seed, and the surface parameters that change it the most. The other
/// parameters are kept as they are.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationSettings {
    pub seed: u64,
    pub surface: SurfaceParams,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    generation: &mut GenerationSettings,
    error: Option<&str>,
    requests: &mut Vec<OverlayRequest>,
) {
    egui::CollapsingHeader::new("Generation").show(ui, |ui| {
        egui::Grid::new("generation").num_columns(2).show(ui, |ui| {
            ui.label("Seed");
            ui.add(egui::DragValue::new(&mut generation.seed));
            ui.end_row();
            let surface = &mut generation.surface;
            let latitudes = -90.0..=90.0;
            let celsius = -150.0..=150.0;
            edit(
                ui,
                "North latitude °",
                &mut surface.north_latitude_degrees,
                latitudes.clone(),
            );
            edit(
                ui,
                "South latitude °",
                &mut surface.south_latitude_degrees,
                latitudes,
            );
            edit(ui, "Water share", &mut surface.water_share, 0.0..=1.0);
            let temperature = &mut surface.temperature;
            edit(
                ui,
                "Equator mean °C",
                &mut temperature.equator_mean_celsius,
                celsius.clone(),
            );
            edit(
                ui,
                "Pole mean °C",
                &mut temperature.pole_mean_celsius,
                celsius.clone(),
            );
            edit(
                ui,
                "Equator swing °C",
                &mut temperature.equator_swing_celsius,
                celsius.clone(),
            );
            edit(
                ui,
                "Pole swing °C",
                &mut temperature.pole_swing_celsius,
                celsius.clone(),
            );
            edit(
                ui,
                "Cooling at highest °C",
                &mut temperature.cooling_at_highest_celsius,
                celsius,
            );
        });
        if ui.button("Regenerate").clicked() {
            requests.push(OverlayRequest::Regenerate(Box::new(generation.clone())));
        }
        if let Some(error) = error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
    });
}

/// A row of the generation grid that edits `value` within `range`.
fn edit(ui: &mut egui::Ui, name: &str, value: &mut f64, range: RangeInclusive<f64>) {
    ui.label(name);
    let span = range.end() - range.start();
    ui.add(
        egui::DragValue::new(value)
            .range(range)
            .speed(span / 1000.0)
            .max_decimals(2),
    );
    ui.end_row();
}
