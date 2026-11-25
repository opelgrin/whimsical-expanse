//! The speed panel: the simulation speed, and a single step while paused.

use super::OverlayRequest;
use crate::pacer::Speed;

/// The speeds the panel offers, with their labels, in display order.
const SPEED_CHOICES: [(Speed, &str); 4] = [
    (Speed::Paused, "Pause"),
    (Speed::Normal, "1×"),
    (Speed::Fast, "2×"),
    (Speed::Faster, "3×"),
];

pub(super) fn show(ui: &mut egui::Ui, speed: Speed, requests: &mut Vec<OverlayRequest>) {
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
