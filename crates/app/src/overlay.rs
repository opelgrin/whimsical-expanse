//! The debug overlay: development controls drawn with `egui` over the world.
//!
//! The overlay never acts on the simulation itself. It shows what it is given
//! and returns what the developer asked for, which the window then applies.

use std::ops::RangeInclusive;
use std::time::Duration;

use egui::{Align2, ClippedPrimitive, Color32, TexturesDelta, ViewportId};
use expanse_core::time::Tick;
use expanse_render::{TerrainView, UiFrame};
use expanse_terrain::{BiomeTable, Field};
use expanse_worldgen::SurfaceParams;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::inspector::{ObstacleReport, TileReport};
use crate::pacer::Speed;

/// The weight of the newest frame in the averaged frame time. Small enough
/// that the displayed rate is readable, large enough to follow a change
/// within a second.
const FRAME_TIME_SMOOTHING: f32 = 0.05;

/// The speeds the overlay offers, with their labels, in display order.
const SPEED_CHOICES: [(Speed, &str); 4] = [
    (Speed::Paused, "Pause"),
    (Speed::Normal, "1×"),
    (Speed::Fast, "2×"),
    (Speed::Faster, "3×"),
];

/// The terrain views the overlay offers, with their labels, in display order.
const VIEW_CHOICES: [(TerrainView, &str); 6] = [
    (TerrainView::Biomes, "Biomes"),
    (TerrainView::Field(Field::Elevation), "Elevation"),
    (
        TerrainView::Field(Field::WinterTemperature),
        "Winter temperature",
    ),
    (
        TerrainView::Field(Field::SummerTemperature),
        "Summer temperature",
    ),
    (TerrainView::Field(Field::Moisture), "Moisture"),
    (TerrainView::Field(Field::Vegetation), "Vegetation"),
];

/// What the overlay shows about the running game.
#[derive(Debug, Clone, Copy)]
pub struct OverlayStatus<'a> {
    /// The tick of the most recent snapshot.
    pub tick: Tick,
    /// The selected simulation speed.
    pub speed: Speed,
    /// What colours the terrain.
    pub view: TerrainView,
    /// Whether the obstacle layer is shown.
    pub obstacles: bool,
    /// The tile under the cursor, if any.
    pub inspected: Option<&'a TileReport>,
    /// The biomes and obstacle kinds the terrain refers to.
    pub biome_table: &'a BiomeTable,
    /// Why the last regeneration failed, if it did.
    pub generation_error: Option<&'a str>,
}

/// A change the developer asked for through the overlay.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayRequest {
    /// Run the simulation at this speed.
    SetSpeed(Speed),
    /// Run one tick while paused.
    Step,
    /// Colour the terrain this way.
    SetView(TerrainView),
    /// Show or hide the obstacle layer.
    ShowObstacles(bool),
    /// Start a new world from these settings.
    Regenerate(Box<GenerationSettings>),
}

/// What the overlay lets the developer change when starting a new world: the
/// seed, and the surface parameters that change it the most. The other
/// parameters are kept as they are.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationSettings {
    pub seed: u64,
    pub surface: SurfaceParams,
}

/// What the overlay produced for one frame.
pub struct OverlayFrame {
    /// The changes asked for during the frame, in order.
    pub requests: Vec<OverlayRequest>,
    primitives: Vec<ClippedPrimitive>,
    textures_delta: TexturesDelta,
    pixels_per_point: f32,
}

impl OverlayFrame {
    /// What to paint over the world.
    #[must_use]
    pub fn ui(&self) -> UiFrame<'_> {
        UiFrame {
            primitives: &self.primitives,
            textures_delta: &self.textures_delta,
            pixels_per_point: self.pixels_per_point,
        }
    }
}

/// Development controls and readings, hidden until toggled.
///
/// `egui` runs every frame, even while the overlay is hidden, so that it keeps
/// track of held buttons and the cursor and is up to date when shown.
pub struct DebugOverlay {
    visible: bool,
    context: egui::Context,
    window_input: egui_winit::State,
    average_frame_seconds: Option<f32>,
    /// The settings being edited for the next world.
    generation: GenerationSettings,
}

impl DebugOverlay {
    /// Creates a hidden overlay for `window`, whose generation settings start
    /// as `generation`.
    #[must_use]
    pub fn new(window: &Window, generation: GenerationSettings) -> Self {
        let context = egui::Context::default();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "HiDPI scale factors fit in f32"
        )]
        let window_input = egui_winit::State::new(
            context.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        Self {
            visible: false,
            context,
            window_input,
            average_frame_seconds: None,
            generation,
        }
    }

    /// Shows the overlay if it is hidden, and hides it otherwise.
    pub const fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Passes `event` to the overlay. Returns whether the overlay consumed it,
    /// so that it does not also act on the world. A hidden overlay consumes
    /// nothing.
    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.window_input.on_window_event(window, event);
        self.visible && response.consumed
    }

    /// Runs the overlay for one frame that lasted `frame_time`, showing
    /// `status`.
    pub fn run(
        &mut self,
        window: &Window,
        frame_time: Duration,
        status: OverlayStatus<'_>,
    ) -> OverlayFrame {
        self.record_frame_time(frame_time);
        let raw_input = self.window_input.take_egui_input(window);
        let visible = self.visible;
        let average_frame_seconds = self.average_frame_seconds;
        let generation = &mut self.generation;
        let mut requests = Vec::new();
        let output = self.context.run_pass(raw_input, |context| {
            // `egui` may run a pass again and discard the first one, so only
            // the requests of the last pass count.
            requests.clear();
            if visible {
                show(
                    context,
                    status,
                    average_frame_seconds,
                    generation,
                    &mut requests,
                );
            }
        });
        self.window_input
            .handle_platform_output(window, output.platform_output);
        let primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        OverlayFrame {
            requests,
            primitives,
            textures_delta: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        }
    }

    fn record_frame_time(&mut self, frame_time: Duration) {
        let seconds = frame_time.as_secs_f32();
        if seconds <= 0.0 {
            return;
        }
        self.average_frame_seconds = Some(match self.average_frame_seconds {
            Some(average) => average + (seconds - average) * FRAME_TIME_SMOOTHING,
            None => seconds,
        });
    }
}

fn show(
    context: &egui::Context,
    status: OverlayStatus<'_>,
    average_frame_seconds: Option<f32>,
    generation: &mut GenerationSettings,
    requests: &mut Vec<OverlayRequest>,
) {
    egui::Window::new("Debug")
        .anchor(Align2::LEFT_TOP, [8.0, 8.0])
        .resizable(false)
        .collapsible(false)
        .show(context, |ui| {
            ui.label(format!("Tick {}", status.tick.count()));
            if let Some(seconds) = average_frame_seconds {
                ui.label(format!(
                    "{:.0} fps ({:.1} ms)",
                    seconds.recip(),
                    seconds * 1000.0
                ));
            }
            ui.separator();
            show_speed(ui, status.speed, requests);
            ui.separator();
            show_view(ui, status.view, status.obstacles, requests);
            ui.separator();
            show_inspector(ui, status.inspected, status.biome_table);
            ui.separator();
            show_generation(ui, generation, status.generation_error, requests);
        });
}

fn show_speed(ui: &mut egui::Ui, speed: Speed, requests: &mut Vec<OverlayRequest>) {
    ui.horizontal(|ui| {
        for (choice, label) in SPEED_CHOICES {
            let selected = speed == choice;
            if ui.selectable_label(selected, label).clicked() && !selected {
                requests.push(OverlayRequest::SetSpeed(choice));
            }
        }
    });
    let paused = speed == Speed::Paused;
    if ui.add_enabled(paused, egui::Button::new("Step")).clicked() {
        requests.push(OverlayRequest::Step);
    }
}

fn show_view(
    ui: &mut egui::Ui,
    view: TerrainView,
    obstacles: bool,
    requests: &mut Vec<OverlayRequest>,
) {
    let label = |view| {
        VIEW_CHOICES
            .iter()
            .find(|&&(choice, _)| choice == view)
            .map_or("", |&(_, label)| label)
    };
    egui::ComboBox::from_label("View")
        .selected_text(label(view))
        .show_ui(ui, |ui| {
            for (choice, label) in VIEW_CHOICES {
                if ui.selectable_label(view == choice, label).clicked() && view != choice {
                    requests.push(OverlayRequest::SetView(choice));
                }
            }
        });
    let mut shown = obstacles;
    if ui.checkbox(&mut shown, "Obstacle layer").changed() {
        requests.push(OverlayRequest::ShowObstacles(shown));
    }
}

fn show_inspector(ui: &mut egui::Ui, inspected: Option<&TileReport>, table: &BiomeTable) {
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

fn show_generation(
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

const fn field_name(field: Field) -> &'static str {
    match field {
        Field::Elevation => "Elevation",
        Field::WinterTemperature => "Winter",
        Field::SummerTemperature => "Summer",
        Field::Moisture => "Moisture",
        Field::Vegetation => "Vegetation",
    }
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
