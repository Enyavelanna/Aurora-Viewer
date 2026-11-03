//! Aurora views of death confirmations; rendering never waits for name resolution.
use super::{Display, Outcome, RecordedEvent, Settings, State};
use crate::{theme::Palette, world::World};
use aurora_i18n::tr;
use egui::{Align2, Stroke, vec2};
use std::{collections::HashMap, time::Instant};
use uuid::Uuid;
pub(crate) fn line(row: &RecordedEvent, world: &World, display: &Display) -> String {
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
    if display.medals && !row.awards.is_empty() {
        let names: Vec<&str> = row.awards.iter().map(|a| display.name(*a)).collect();
        text.push_str(&format!("  ·  {}", names.join(", ")));
    }
    text
}
/// The fights that are over, newest first.
fn recaps(ui: &mut egui::Ui, state: &State) {
    if state.recaps.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(tr!("combat-recaps"))
        .id_salt("combat-recaps")
        .show(ui, |ui| {
            for recap in state.recaps.iter().rev() {
                let mut text = format!(
                    "[{}] {}",
                    recap.timestamp,
                    tr!(
                        "combat-recap-line",
                        kills = recap.kills.to_string(),
                        deaths = recap.deaths.to_string(),
                        streak = recap.best_streak.to_string(),
                        seconds = recap.seconds.to_string()
                    )
                );
                if !recap.medals.is_empty() {
                    let medals: Vec<String> = recap
                        .medals
                        .iter()
                        .map(|(award, n)| format!("{} ×{n}", state.display.name(*award)))
                        .collect();
                    text.push_str(&format!("  ·  {}", medals.join(", ")));
                }
                ui.add(egui::Label::new(text).wrap());
            }
        });
}
pub(crate) fn journal(ui: &mut egui::Ui, world: &World, state: &mut State) {
    recaps(ui, state);
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
                    let text = format!("[{}] {}", row.timestamp, line(row, world, &state.display));
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
    cfg: &mut Settings,
    allies: &std::collections::BTreeSet<Uuid>,
    ally_color: egui::Color32,
    images: &HashMap<Uuid, egui::TextureHandle>,
) {
    let now = Instant::now();
    state.sync_display(cfg);
    state.close_fight(now);
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
    let mut edit = super::feed::Edit::default();
    if cfg.feed || cfg.feed_test {
        let entries = if cfg.feed_test {
            super::feed::sample()
        } else {
            state
                .feed
                .iter()
                .rev()
                .filter(|r| now.saturating_duration_since(r.at).as_secs_f32() < cfg.feed_seconds.clamp(1.0, 30.0))
                .filter(|r| super::feed::keep(cfg, &r.event, world.agent_id, allies))
                .take(cfg.feed_entries.clamp(1, 20))
                .map(|r| super::feed::entry(r, now, world, state.display.medals, allies))
                .collect()
        };
        edit = super::feed::show(ctx, p, cfg, &entries, ally_color, rect);
    }
    if let Some(position) = edit.position {
        cfg.feed_position = position;
    }
    if edit.done {
        cfg.feed_test = false;
    }
    if let Some(view) = state.medal(cfg, now) {
        let (award, alpha, reveal) = (view.award, view.alpha, view.reveal);
        let scale = cfg.banner_scale.clamp(0.5, 2.0);
        let accent = crate::ui::combat_rewards::accent(p, award);
        egui::Area::new(egui::Id::unique("combat-medal"))
            .anchor(Align2::CENTER_TOP, vec2(0.0, cfg.banner_top - (1.0 - reveal) * 18.0 * scale))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                egui::Frame::new().inner_margin(8).show(ui, |ui| {
                    ui.set_width(310.0 * scale);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(crate::ui::combat_rewards::family_label(award))
                                .size(11.0 * scale)
                                .strong()
                                .color(accent),
                        );
                        ui.add_space(2.0);
                        let texture = cfg
                            .medal_textures
                            .get(award.slot())
                            .and_then(|s| Uuid::parse_str(s).ok())
                            .and_then(|id| images.get(&id));
                        let (medal_rect, _) = ui.allocate_exact_size(egui::Vec2::splat(104.0 * scale), egui::Sense::hover());
                        if let Some(texture) = texture {
                            let side = (72.0 + 22.0 * reveal) * scale;
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
                                (48.0 + 14.0 * reveal) * scale,
                                accent,
                            );
                        }
                        for (text, size, color) in [
                            (cfg.medal_name(award), 22.0 * scale, p.ink),
                            (tr!("combat-reward-confirmed"), 11.0 * scale, p.muted),
                        ] {
                            let (rect, _) = ui.allocate_exact_size(vec2(310.0 * scale, size + 4.0), egui::Sense::hover());
                            crate::ui::minimap::text_shadow(ui.painter(), rect.center(), Align2::CENTER_CENTER, text, size, color);
                        }
                        chips(ui, p, cfg, &view.chips, scale);
                    });
                });
            });
        ctx.request_repaint();
    }
}
/// The other medals the same kill earned, small, side by side under the banner.
fn chips(ui: &mut egui::Ui, p: &Palette, cfg: &Settings, awards: &[super::awards::Award], scale: f32) {
    if awards.is_empty() {
        return;
    }
    let font = egui::FontId::proportional(11.0 * scale);
    let pad = vec2(8.0, 3.0) * scale;
    let gap = 6.0 * scale;
    let galleys: Vec<_> = awards
        .iter()
        .map(|a| ui.painter().layout_no_wrap(cfg.medal_name(*a).to_owned(), font.clone(), p.ink))
        .collect();
    let total: f32 = galleys.iter().map(|g| g.size().x + pad.x * 2.0).sum::<f32>() + gap * (galleys.len() - 1) as f32;
    let height = galleys.iter().map(|g| g.size().y).fold(0.0, f32::max) + pad.y * 2.0;
    ui.add_space(4.0 * scale);
    let (row, _) = ui.allocate_exact_size(vec2(310.0 * scale, height), egui::Sense::hover());
    let radius = ui.visuals().widgets.inactive.corner_radius;
    let mut x = row.center().x - total.min(row.width()) / 2.0;
    for (award, galley) in awards.iter().zip(galleys) {
        let width = galley.size().x + pad.x * 2.0;
        let pill = egui::Rect::from_min_size(egui::pos2(x, row.top()), vec2(width, height));
        ui.painter().rect_filled(pill, radius, p.raised.gamma_multiply(0.9));
        ui.painter().rect_stroke(
            pill,
            radius,
            Stroke::new(1.0, crate::ui::combat_rewards::accent(p, *award)),
            egui::StrokeKind::Inside,
        );
        ui.painter().galley(pill.min + pad, galley, p.ink);
        x += width + gap;
    }
}
