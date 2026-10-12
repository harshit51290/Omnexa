use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;

// ---------------- "Vault" gaming theme ----------------
// Deep-space blues, neon cyan primary, violet secondary, gold highlights.
// Built to feel like a game library (Steam/GOG), not a settings utility.

pub(crate) const BG: egui::Color32 = egui::Color32::from_rgb(5, 11, 20);
pub(crate) const PANEL: egui::Color32 = egui::Color32::from_rgb(11, 22, 40);
pub(crate) const CARD: egui::Color32 = egui::Color32::from_rgb(16, 29, 50);
pub(crate) const CARD_HOVER: egui::Color32 = egui::Color32::from_rgb(22, 38, 63);
pub(crate) const BORDER: egui::Color32 = egui::Color32::from_rgb(24, 52, 84);
pub(crate) const BORDER_GLOW: egui::Color32 = egui::Color32::from_rgb(53, 215, 255);
pub(crate) const TEXT: egui::Color32 = egui::Color32::from_rgb(234, 242, 255);
pub(crate) const WEAK: egui::Color32 = egui::Color32::from_rgb(155, 170, 193);
pub(crate) const WEAK_DIM: egui::Color32 = egui::Color32::from_rgb(99, 116, 141);
pub(crate) const ACCENT: egui::Color32 = egui::Color32::from_rgb(22, 140, 255);
pub(crate) const ACCENT_INK: egui::Color32 = egui::Color32::from_rgb(4, 20, 26);
pub(crate) const CYAN: egui::Color32 = egui::Color32::from_rgb(53, 215, 255);
pub(crate) const GOOD: egui::Color32 = egui::Color32::from_rgb(53, 214, 160);
pub(crate) const GOOD_DIM: egui::Color32 = egui::Color32::from_rgb(11, 46, 35);
pub(crate) const WARN: egui::Color32 = egui::Color32::from_rgb(255, 181, 46);
pub(crate) const WARN_DIM: egui::Color32 = egui::Color32::from_rgb(58, 42, 8);
pub(crate) const ERROR: egui::Color32 = egui::Color32::from_rgb(255, 99, 116);

pub(crate) const AVATAR_COLORS: [egui::Color32; 8] = [
    egui::Color32::from_rgb(34, 211, 238),
    egui::Color32::from_rgb(53, 215, 255),
    egui::Color32::from_rgb(52, 211, 153),
    egui::Color32::from_rgb(96, 165, 250),
    egui::Color32::from_rgb(248, 113, 113),
    egui::Color32::from_rgb(45, 212, 191),
    egui::Color32::from_rgb(251, 146, 60),
    egui::Color32::from_rgb(148, 163, 184),
];

pub(crate) fn avatar_color(name: &str) -> egui::Color32 {
    let h: usize = name.bytes().fold(0, |a, b| a.wrapping_add(b as usize));
    AVATAR_COLORS[h % AVATAR_COLORS.len()]
}

/// GPU texture for a cached logo file, loaded once.
pub(crate) fn logo_texture(
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
    path: &PathBuf,
) -> Option<egui::TextureHandle> {
    let key = path.to_string_lossy().to_string();
    if let Some(h) = textures.get(&key) {
        return Some(h.clone());
    }
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 || w * h > 8_000_000 {
        return None;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([w, h], &img.into_raw());
    let handle = ctx.load_texture(&key, color, egui::TextureOptions::LINEAR);
    textures.insert(key, handle.clone());
    Some(handle)
}

/// Colored initial bauble: games are recognisable at a glance, no icons needed.
pub(crate) fn avatar(ui: &mut egui::Ui, name: &str, size: f32) {
    let initial = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('?')
        .to_string()
        .to_uppercase();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter();
    p.circle_filled(rect.center(), size / 2.0, avatar_color(name));
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(size * 0.45),
        ACCENT_INK,
    );
}

/// Small dashboard number in the header strip.
pub(crate) fn header_stat(ui: &mut egui::Ui, value: &str, label: &str) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 5))
        .corner_radius(10.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(value).size(14.0).strong().color(TEXT));
                ui.label(egui::RichText::new(label).size(10.5).color(WEAK));
            });
        });
}

/// Stat chip for the game hero: dim label over a bright value.
pub(crate) fn detail_chip(ui: &mut egui::Ui, icon: &str, label: &str, value: &str) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 6))
        .corner_radius(10.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon).color(CYAN).size(13.0));
                    ui.label(egui::RichText::new(label).size(10.0).color(WEAK_DIM));
                });
                ui.label(egui::RichText::new(value).size(13.5).strong().color(TEXT));
            });
        });
}

/// Brand art baked into the binary (assets/ is also the README source).
const LOGO_BYTES: &[u8] = include_bytes!("../../assets/logo.png");
const BANNER_BYTES: &[u8] = include_bytes!("../../assets/banner.png");

/// Brand texture from embedded PNGs, decoded once and cached.
pub(crate) fn brand_texture(
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
    key: &'static str,
    bytes: &'static [u8],
) -> Option<egui::TextureHandle> {
    if let Some(h) = textures.get(key) {
        return Some(h.clone());
    }
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 || w * h > 12_000_000 {
        return None;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([w, h], &img.into_raw());
    let handle = ctx.load_texture(key, color, egui::TextureOptions::LINEAR);
    textures.insert(key.to_string(), handle.clone());
    Some(handle)
}

/// App logo (embedded assets/logo.png), with drawn fallback.
pub(crate) fn brand_logo(
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
    ui: &mut egui::Ui,
    size: f32,
) {
    if let Some(tex) = brand_texture(ctx, textures, "brand-logo", LOGO_BYTES) {
        ui.add(
            egui::Image::new(&tex)
                .fit_to_exact_size(egui::vec2(size, size))
                .corner_radius(size * 0.25),
        );
    } else {
        logo_mark(ui, size);
    }
}

/// Welcome banner (embedded assets/banner.png). Returns false when missing.
pub(crate) fn welcome_banner(
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
    ui: &mut egui::Ui,
) {
    if let Some(tex) = brand_texture(ctx, textures, "brand-banner", BANNER_BYTES) {
        egui::Frame::new()
            .corner_radius(16.0)
            .stroke(egui::Stroke::new(1.0_f32, BORDER))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::Image::new(&tex)
                        .max_height(170.0)
                        .corner_radius(14.0),
                );
            });
        ui.add_space(10.0);
    }
}
pub(crate) fn logo_mark(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, size * 0.28, egui::Color32::from_rgb(10, 34, 64));
    p.rect_stroke(
        rect,
        size * 0.28,
        egui::Stroke::new(2.0_f32, ACCENT),
        egui::StrokeKind::Outside,
    );
    let c = rect.center();
    let r = size * 0.22;
    // shield-ish ring
    let pts: Vec<egui::Pos2> = (0..48)
        .map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            c + egui::vec2(a.cos() * r, a.sin() * r)
        })
        .collect();
    p.line(pts, egui::Stroke::new(2.0_f32, CYAN));
    // check
    let k = size * 0.11;
    p.line_segment(
        [
            c + egui::vec2(-k, k * 0.1),
            c + egui::vec2(-k * 0.15, k * 0.75),
        ],
        egui::Stroke::new(3.0_f32, TEXT),
    );
    p.line_segment(
        [
            c + egui::vec2(-k * 0.15, k * 0.75),
            c + egui::vec2(k, -k * 0.7),
        ],
        egui::Stroke::new(3.0_f32, TEXT),
    );
}

/// Storage ring: honest fraction of app-managed bytes that are backups.
pub(crate) fn storage_ring(ui: &mut egui::Ui, frac: f32, center_text: &str) {
    let size = egui::vec2(104.0, 92.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    p.circle_stroke(c, 38.0, egui::Stroke::new(8.0_f32, BORDER));
    let frac = frac.clamp(0.0, 1.0);
    if frac > 0.005 {
        let n = 72_usize;
        let upto = ((n as f32 * frac).round() as usize).max(2);
        let pts: Vec<egui::Pos2> = (0..=upto)
            .map(|i| {
                let a =
                    -std::f32::consts::FRAC_PI_2 + (i as f32 / n as f32) * std::f32::consts::TAU;
                c + egui::vec2(a.cos() * 38.0, a.sin() * 38.0)
            })
            .collect();
        p.line(pts, egui::Stroke::new(8.0_f32, ACCENT));
    }
    p.text(
        c,
        egui::Align2::CENTER_CENTER,
        center_text,
        egui::FontId::proportional(17.0),
        TEXT,
    );
}

/// Small caps section header with an accent count on the right.
pub(crate) fn section_title(ui: &mut egui::Ui, icon: &str, title: &str, extra: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{icon} {title}"))
                .color(CYAN)
                .size(12.5)
                .strong(),
        );
        if !extra.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(extra).color(WEAK).size(12.0));
            });
        }
    });
}
