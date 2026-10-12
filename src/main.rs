mod core;
mod parsers;
mod ui;

use eframe::egui;

use std::sync::mpsc;

use crate::core::{backup, settings};
use crate::ui::app::{self, App};
use crate::ui::theme::*;

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--scan" || a == "--details" || a == "--selftest") {
        return app::headless();
    }
    let (tx, rx) = mpsc::channel::<String>();
    let boot_root = backup::default_backup_root();
    let boot_root = settings::load(&boot_root).backup_root;
    settings::spawn_watchdog(boot_root, tx);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1024.0, 620.0])
            .with_title("Omnexa — Universal Data Saver"),
        ..Default::default()
    };
    eframe::run_native(
        "omnexa",
        opts,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            style.visuals.window_fill = BG;
            style.visuals.panel_fill = PANEL;
            style.visuals.faint_bg_color = CARD;
            style.visuals.extreme_bg_color = BG;
            style.visuals.widgets.inactive.bg_fill = CARD;
            style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);
            style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(10);
            style.visuals.widgets.hovered.bg_fill = CARD_HOVER;
            style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(10);
            style.visuals.widgets.active.bg_fill = CARD_HOVER;
            style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(10);
            style.visuals.selection.bg_fill = ACCENT;
            style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT_INK);
            style.visuals.window_corner_radius = egui::CornerRadius::same(14);
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            style.spacing.button_padding = egui::vec2(18.0, 12.0);
            cc.egui_ctx.set_style(style);
            cc.egui_ctx.set_pixels_per_point(1.15);
            Ok(Box::new(App::new(rx)))
        }),
    )
}
