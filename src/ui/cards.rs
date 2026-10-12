use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::parsers::{elden, hollow, minecraft, sekiro, shady};
use crate::ui::theme::*;

/// One friendly game row: cover-first library card, Steam-style.
pub(crate) fn game_card(
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
    ui: &mut egui::Ui,
    name: &str,
    sub: &str,
    active: bool,
    protected: bool,
    running: bool,
    checked: bool,
    logo: Option<PathBuf>,
) -> (bool, bool) {
    let frame = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(0))
        .corner_radius(14.0)
        .stroke(if active {
            egui::Stroke::new(2.0_f32, BORDER_GLOW)
        } else {
            egui::Stroke::new(1.0_f32, BORDER)
        })
        .fill(if active { CARD_HOVER } else { CARD });
    let mut clicked = false;
    let mut toggled = false;
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(tex) = logo.as_ref().and_then(|p| logo_texture(ctx, textures, p)) {
            ui.add(
                egui::Image::new(&tex)
                    .fit_to_exact_size(egui::vec2(ui.available_width(), 96.0))
                    .corner_radius(10.0),
            );
        } else {
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 64.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 10.0, avatar_color(name));
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                name.chars()
                    .find(|c| c.is_alphanumeric())
                    .unwrap_or('?')
                    .to_string()
                    .to_uppercase(),
                egui::FontId::proportional(28.0),
                egui::Color32::from_rgba_unmultiplied(10, 14, 24, 220),
            );
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let r = ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(name).strong().size(15.5).color(TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut c = checked;
                            if ui
                                .checkbox(&mut c, "")
                                .on_hover_text("Select for batch backup")
                                .changed()
                            {
                                toggled = true;
                            }
                        });
                    });
                    ui.label(egui::RichText::new(sub).color(WEAK).size(12.5));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let (txt, fg, bg) = if protected {
                            ("PROTECTED", GOOD, GOOD_DIM)
                        } else {
                            ("NOT BACKED UP", WARN, WARN_DIM)
                        };
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .corner_radius(20.0)
                            .fill(bg)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(txt).size(10.5).strong().color(fg));
                            });
                        if running {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(8, 3))
                                .corner_radius(20.0)
                                .fill(GOOD_DIM)
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("RUNNING")
                                            .size(10.5)
                                            .strong()
                                            .color(GOOD),
                                    );
                                });
                        }
                    });
                });
                if r.response.interact(egui::Sense::click()).clicked() {
                    clicked = true;
                }
            });
    });
    ui.add_space(8.0);
    (clicked, toggled)
}
/// One Sekiro character, explained like the game explains it.
pub(crate) fn sekiro_card(ui: &mut egui::Ui, s: &sekiro::SekiroSlot) {
    let journey = if s.journey == 0 {
        "First journey".to_string()
    } else {
        format!("Journey {} (NG+{})", s.journey + 1, s.journey)
    };
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Character {}", s.index + 1))
                        .strong()
                        .size(15.0)
                        .color(TEXT),
                );
                ui.label(egui::RichText::new("● In progress").color(GOOD).strong());
            });
            ui.label(
                egui::RichText::new(format!(
                    "{journey} · {} played",
                    sekiro::fmt_playtime(s.playtime_secs)
                ))
                .color(WEAK),
            );
            ui.add_space(4.0);
            let num = sekiro::fmt_num;
            match s.attack {
                Some(a) if a > 1 => ui.label(format!("⚔ Attack {a} · {} bosses beaten", a - 1)),
                _ => ui.label("⚔ Attack 1 · no bosses beaten yet"),
            };
            match s.vitality {
                Some(v) if v > 1 => ui.label(format!("❤ Vitality {v} · {} necklaces used", v - 1)),
                _ => ui.label("❤ Vitality 1 · no necklaces yet"),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "Health {}",
                    s.hp.map(num).as_deref().unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!(
                    "Posture {}",
                    s.posture.map(num).as_deref().unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!("Sen {}", s.sen.map(num).as_deref().unwrap_or("—")));
            });
            match s.gourd {
                Some(g) if g > 1 => ui.label(format!(
                    "🎃 Gourd: {g} sips · {} seeds given to Emma",
                    g - 1
                )),
                _ => ui.label("🎃 Gourd: 1 sip"),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("✦ {} skill points", num(s.skill_points)));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!("💀 {} memories held", s.memories_held));
            });
            if !s.bosses.is_empty() {
                ui.label(
                    egui::RichText::new(format!("👹 Beaten: {}", s.bosses.join(" · "))).size(12.0),
                );
            }
            let idols_lit: usize = s.idol_areas.iter().map(|(_, l, _)| l).sum();
            let idols_total: usize = s.idol_areas.iter().map(|(_, _, t)| t).sum();
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("🕯 {idols_lit} of {idols_total} idols lit"));
                if !s.endings.is_empty() {
                    ui.label(egui::RichText::new("·").color(WEAK));
                    ui.label(format!("🏁 {}", s.endings.join(", ")));
                }
            });
            if !s.cats.is_empty() {
                let line: Vec<String> = s.cats.iter().map(|(n, c)| format!("{n}: {c}")).collect();
                ui.label(
                    egui::RichText::new(format!(
                        "🎒 {} carried · {} in storage — {}",
                        s.carried,
                        s.stored,
                        line.join(" · ")
                    ))
                    .color(WEAK)
                    .size(12.0),
                );
            }
            let areas: Vec<String> = s
                .idol_areas
                .iter()
                .filter(|(_, l, _)| *l > 0)
                .map(|(a, l, t)| format!("{a} {l}/{t}"))
                .collect();
            if !areas.is_empty() {
                ui.label(
                    egui::RichText::new(areas.join(" · "))
                        .color(WEAK)
                        .size(12.0),
                );
            }
        });
    ui.add_space(4.0);
}

/// One Elden Ring character, explained like the status screen.
pub(crate) fn er_card(ui: &mut egui::Ui, s: &elden::ErSlot) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(
                        s.name
                            .clone()
                            .unwrap_or_else(|| format!("Character {}", s.index + 1)),
                    )
                    .strong()
                    .size(15.0)
                    .color(TEXT),
                );
                ui.label(
                    egui::RichText::new(format!("Level {}", s.level))
                        .color(CYAN)
                        .strong(),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "Runes {}",
                    s.runes.map(sekiro::fmt_num).as_deref().unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!(
                    "Health {}",
                    s.hp.map(sekiro::fmt_num).as_deref().unwrap_or("—")
                ));
            });
            if let Some(a) = s.attrs {
                let names = elden::attr_names();
                let line: Vec<String> = names
                    .iter()
                    .zip(a.iter())
                    .map(|(n, v)| format!("{n} {v}"))
                    .collect();
                ui.label(egui::RichText::new(line.join(" · ")).color(WEAK).size(12.0));
            } else {
                ui.label(
                    egui::RichText::new("Details still being mapped for this one.").color(WEAK),
                );
            }
        });
    ui.add_space(4.0);
}

pub(crate) fn hollow_card(ui: &mut egui::Ui, label: &str, s: &hollow::HollowSave) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).strong().size(15.0).color(TEXT));
                ui.label(egui::RichText::new("● In progress").color(GOOD).strong());
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "❤ {} masks",
                    s.health
                        .or(s.max_health)
                        .map(|n| n.to_string())
                        .as_deref()
                        .unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!(
                    "🪙 {} geo",
                    s.geo.map(|n| n.to_string()).as_deref().unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!(
                    "🗡 Nail {}",
                    s.nail.map(|n| n.to_string()).as_deref().unwrap_or("—")
                ));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{} played",
                    s.play_secs
                        .map(hollow::fmt_playtime)
                        .unwrap_or_else(|| "—".into())
                ));
                if let Some(c) = s.completion {
                    if c > 0.0 {
                        ui.label(egui::RichText::new("·").color(WEAK));
                        ui.label(format!("{c:.0}% complete"));
                    }
                }
            });
            if let Some(v) = &s.version {
                ui.label(
                    egui::RichText::new(format!("game version {v}"))
                        .color(WEAK)
                        .size(12.0),
                );
            }
        });
    ui.add_space(4.0);
}

pub(crate) fn ue_card(ui: &mut egui::Ui, agg: &UeAggregate) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !agg.engine.is_empty() {
                ui.label(
                    egui::RichText::new(&agg.engine)
                        .strong()
                        .size(15.0)
                        .color(TEXT),
                );
            }
            if !agg.class.is_empty() {
                ui.label(egui::RichText::new(&agg.class).color(WEAK).size(12.0));
            }
            ui.label(format!(
                "🗂 {} checkpoint file{}",
                agg.files,
                if agg.files == 1 { "" } else { "s" }
            ));
            for (name, when) in agg.recent.iter().take(5) {
                ui.label(
                    egui::RichText::new(format!(
                        "  • {name}{}",
                        if when.is_empty() {
                            String::new()
                        } else {
                            format!(" — {when}")
                        }
                    ))
                    .color(WEAK)
                    .size(12.0),
                );
            }
            if agg.partial && agg.labels.is_empty() {
                ui.label(
                    egui::RichText::new("Newest file ends mid-write — shown what could be read.")
                        .color(WEAK)
                        .size(12.0),
                );
            }
            if !agg.labels.is_empty() {
                ui.label(
                    egui::RichText::new(format!(
                        "Values mentioned: {}",
                        agg.labels
                            .iter()
                            .take(12)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" · ")
                    ))
                    .color(WEAK)
                    .size(12.0),
                );
            }
        });
    ui.add_space(4.0);
}

pub(crate) fn mc_card(ui: &mut egui::Ui, label: &str, w: &minecraft::McWorld) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if label.is_empty() { &w.name } else { label })
                        .strong()
                        .size(15.0)
                        .color(TEXT),
                );
                ui.label(egui::RichText::new("● In progress").color(GOOD).strong());
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} mode", w.mode));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!("{} played", hollow::fmt_playtime(w.play_mins * 60)));
                if w.cheats {
                    ui.label(egui::RichText::new("·").color(WEAK));
                    ui.label("cheats on");
                }
            });
        });
    ui.add_space(4.0);
}

pub(crate) fn shady_card(ui: &mut egui::Ui, s: &shady::ShadySave) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Scene {}", s.scene))
                        .strong()
                        .size(15.0)
                        .color(TEXT),
                );
                ui.label(egui::RichText::new("● In progress").color(GOOD).strong());
            });
            if !s.checkpoint.is_empty() {
                ui.label(format!("📍 Checkpoint: {}", s.checkpoint));
            }
            if s.max_scene > s.scene {
                ui.label(
                    egui::RichText::new(format!(
                        "Furthest: scene {}{}",
                        s.max_scene,
                        if s.max_checkpoint.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", s.max_checkpoint)
                        }
                    ))
                    .color(WEAK),
                );
            }
            ui.label(format!("🦢 Origami folded: {}", s.origami));
        });
    ui.add_space(4.0);
}

pub(crate) fn json_card(ui: &mut egui::Ui, file: &str, fields: &[(String, String)], keys: usize) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(file).strong().size(14.0).color(TEXT));
            for (k, v) in fields.iter().take(8) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{k}:")).color(WEAK).size(12.0));
                    ui.label(egui::RichText::new(v).size(12.0));
                });
            }
            if keys > fields.len() {
                ui.label(
                    egui::RichText::new(format!("+ {} more details", keys - fields.len()))
                        .color(WEAK)
                        .size(12.0),
                );
            }
        });
    ui.add_space(4.0);
}

pub(crate) fn backup_badge(has: bool, line: &str) -> egui::RichText {
    if has {
        egui::RichText::new(format!("🛡 {} ", line))
            .color(GOOD)
            .strong()
            .size(14.0)
    } else {
        egui::RichText::new(format!("○ {} — back it up", line))
            .color(WARN)
            .strong()
            .size(14.0)
    }
}

#[derive(Clone, Default)]
pub(crate) struct UeAggregate {
    pub(crate) engine: String,
    pub(crate) class: String,
    pub(crate) files: usize,
    pub(crate) recent: Vec<(String, String)>,
    pub(crate) partial: bool,
    pub(crate) labels: Vec<String>,
}
