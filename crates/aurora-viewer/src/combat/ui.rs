//! Aurora views of death confirmations; rendering never waits for name resolution.
use super::{Outcome, RecordedEvent, Settings, State};
use crate::{theme::Palette, world::World};
use aurora_i18n::tr;
use egui::{Align2, Stroke, vec2};
use std::{collections::HashMap, time::Instant};
use uuid::Uuid;
pub(crate) fn line(row: &RecordedEvent, world: &World) -> String {
    let e = &row.event;
    let victim = world.social.display_name_of(&e.target);
    let outcome = if e.actor == e.target {
        Outcome::Suicide
    } else if e.actor.is_nil() {
        Outcome::Environment
    } else {
        e.outcome(world.agent_id)
    };
    let mut text = match outcome {
        Outcome::Suicide => format!("{victim} · {}", tr!("combat-suicide")),
        Outcome::Environment => format!("{victim} · {}", tr!("combat-environment")),
        _ => format!("{} {} → {victim}", world.social.display_name_of(&e.actor), tr!("combat-kill")),
    };
    if let Some(t) = e.damage_type {
        text.push_str(&format!("  [{t}]"));
    }
    if let Some(w) = &e.weapon {
        text.push_str(&format!("  {w}"));
    }
    text
}
pub(crate) fn journal(ui: &mut egui::Ui, world: &World, state: &mut State) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.personal_view, false, tr!("combat-global"));
        ui.selectable_value(&mut state.personal_view, true, tr!("combat-personal"));
        if ui
            .add_enabled(!state.journal.clearing(), egui::Button::new(tr!("combat-clear-journal")))
            .clicked()
        {
            state.journal.request_clear(state.history.len());
        }
    });
    ui.label(tr!(
        "combat-status",
        received = state.received,
        recorded = state.history.len() as u64,
        saved = state.journal.written as u64,
        lost = state.lost
    ));
    if let Some(error) = &state.journal.error {
        ui.label(tr!("combat-journal-error"));
        ui.label(error);
    }
    let count = if state.personal_view {
        state.personal.len()
    } else {
        state.history.len()
    };
    let height = ui.text_style_height(&egui::TextStyle::Body).max(20.0);
    egui::ScrollArea::vertical()
        .id_salt("combat-deaths")
        .stick_to_bottom(true)
        .show_rows(ui, height, count, |ui, range| {
            for index in range {
                let index = if state.personal_view { state.personal[index] } else { index };
                if let Some(row) = state.history.get(index) {
                    let text = format!("[{}] {}", row.timestamp, line(row, world));
                    ui.add_sized(
                        [ui.available_width(), height],
                        egui::Label::new(&text).halign(egui::Align::Min).truncate(),
                    )
                    .on_hover_text(text);
                }
            }
        });
}
pub(crate) fn overlays(
    ctx: &egui::Context,
    p: &Palette,
    world: &World,
    state: &mut State,
    cfg: &Settings,
    images: &HashMap<Uuid, egui::TextureHandle>,
) {
    let now = Instant::now();
    if !cfg.enabled {
        return;
    }
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("combat-confirmations"),
    ));
    if cfg.killmarker
        && let Some(at) = state.last_kill
    {
        let age = now.saturating_duration_since(at).as_secs_f32();
        let duration = cfg.marker_seconds.clamp(0.05, 2.0);
        if age < duration {
            let [r, g, b, a] = cfg.marker_color;
            // theme-guard: content — configured kill confirmation colour.
            let c = egui::Color32::from_rgba_unmultiplied(
                r,
                g,
                b,
                (a as f32 * cfg.marker_opacity.clamp(0.0, 1.0) * (1.0 - age / duration)) as u8,
            );
            let size = cfg.marker_size.clamp(4.0, 80.0);
            for v in [vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0)] {
                painter.line_segment(
                    [rect.center() + v * size * 0.25, rect.center() + v * size * 0.5],
                    Stroke::new(2.0, c),
                );
            }
            ctx.request_repaint();
        }
    }
    if cfg.feed {
        let position = egui::pos2(
            rect.left() + rect.width() * cfg.feed_position[0].clamp(0.0, 1.0),
            rect.top() + rect.height() * cfg.feed_position[1].clamp(0.0, 1.0),
        );
        let align = if cfg.feed_position[0] > 0.5 {
            Align2::RIGHT_TOP
        } else {
            Align2::LEFT_TOP
        };
        let rows: Vec<_> = state
            .feed
            .iter()
            .rev()
            .filter(|r| now.saturating_duration_since(r.at).as_secs_f32() < cfg.feed_seconds.clamp(1.0, 30.0))
            .take(cfg.feed_entries.clamp(1, 20))
            .map(|r| line(r, world))
            .collect();
        if !rows.is_empty() {
            egui::Area::new(egui::Id::unique("combat-killfeed"))
                .fixed_pos(position)
                .pivot(align)
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    ui.set_opacity(cfg.feed_opacity.clamp(0.0, 1.0));
                    ui.set_max_width((rect.width() * 0.55).clamp(200.0, 720.0));
                    crate::ui::widgets::overlay(ui, p, egui::Margin::same(6), |ui| {
                        for text in rows {
                            ui.add(egui::Label::new(egui::RichText::new(text).size(cfg.feed_size.clamp(9.0, 32.0))).truncate());
                        }
                    });
                });
        }
    }
    if let Some((award, alpha, reveal)) = state.medal(cfg.medals, now) {
        let accent = crate::ui::combat_rewards::accent(p, award);
        egui::Area::new(egui::Id::unique("combat-medal"))
            .anchor(Align2::CENTER_TOP, vec2(0.0, 62.0 - (1.0 - reveal) * 18.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                egui::Frame::new().inner_margin(8).show(ui, |ui| {
                    ui.set_width(310.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(crate::ui::combat_rewards::family_label(award))
                                .size(11.0)
                                .strong()
                                .color(accent),
                        );
                        ui.add_space(2.0);
                        let texture = award
                            .medal_slot()
                            .and_then(|n| cfg.medal_textures.get(n))
                            .and_then(|s| Uuid::parse_str(s).ok())
                            .and_then(|id| images.get(&id));
                        let (medal_rect, _) = ui.allocate_exact_size(egui::Vec2::splat(104.0), egui::Sense::hover());
                        if let Some(texture) = texture {
                            let side = 72.0 + 22.0 * reveal;
                            ui.painter().image(
                                texture.id(),
                                egui::Rect::from_center_size(medal_rect.center(), egui::Vec2::splat(side)),
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                crate::ui::widgets::PICTURE_TINT,
                            );
                        } else {
                            crate::ui::widgets::paint_glyph(
                                ui.painter(),
                                crate::ui::combat_rewards::icon(award),
                                medal_rect.center(),
                                48.0 + 14.0 * reveal,
                                accent,
                            );
                        }
                        for (text, size, color) in [(award.label(), 22.0, p.ink), (tr!("combat-reward-confirmed"), 11.0, p.muted)] {
                            let (rect, _) = ui.allocate_exact_size(vec2(310.0, size + 4.0), egui::Sense::hover());
                            crate::ui::minimap::text_shadow(ui.painter(), rect.center(), Align2::CENTER_CENTER, text, size, color);
                        }
                    });
                });
            });
        ctx.request_repaint();
    }
}
