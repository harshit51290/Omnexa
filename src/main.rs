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
    let mut vp = egui::ViewportBuilder::default()
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([1024.0, 620.0])
        .with_title("Omnexa — Universal Data Saver");
    // Taskbar/window icon from the embedded brand logo.
    if let Ok(img) = image::load_from_memory(include_bytes!("../assets/logo.png")) {
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        if w > 0 && h > 0 && w * h <= 4_000_000 {
            vp = vp.with_icon(egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            });
        }
    }
    let opts = eframe::NativeOptions {
        viewport: vp,
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
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(14.0, 8.0);
            cc.egui_ctx.set_style(style);
            cc.egui_ctx.set_pixels_per_point(1.0);
            Ok(Box::new(App::new(rx)))
        }),
    )
}
