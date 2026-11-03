//! Aurora's hold gesture and compact, identity-aware ping presentation.
use super::{
    pings::ShareState,
    tactical::{Ping, Settings, color, kind_name, paint_icon},
};
use crate::{
    theme::Palette,
    ui::{minimap::text_shadow, widgets},
    world::World,
};
use aurora_i18n::tr;
use egui::{Align2, FontId, Pos2, Stroke, vec2};
use std::time::Instant;
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Press {
    pub since: Instant,
    pub point: Option<[f64; 3]>,
    pub center: Pos2,
    pub target: Option<Uuid>,
    pub upgrade: Option<Uuid>,
    pub contact: bool,
    pub region: Option<u64>,
}
impl Press {
    pub fn wheel(&self, hold_ms: f32) -> bool {
        self.since.elapsed().as_millis() as f32 >= hold_ms
    }
}

pub(crate) fn short_name(ping: &Ping, world: &World) -> String {
    if ping.own() {
        return tr!("ping-you").into();
    }
    let name = world.social.display_name_of(&ping.sender);
    first_name(&name)
}
fn first_name(name: &str) -> String {
    name.split_whitespace()
        .next()
        .filter(|s| *s != "…")
        .map(|s| s.chars().take(24).collect())
        .unwrap_or_else(|| tr!("ping-resident").into())
}

pub(crate) fn label(ping: &Ping, world: &World, cfg: &Settings, distance: f32) -> String {
    let mut label = format!("{} · {}", short_name(ping, world), kind_name(ping.kind));
    if cfg.distances {
        label.push_str(&format!("  {distance:.0} m"));
        let height = ping.marker.position[2] - world.agent.position.z as f64;
        if height.abs() >= 5.0 {
            label.push_str(&format!("  {} {:.0} m", if height > 0.0 { "↑" } else { "↓" }, height.abs()));
        }
    }
    if ping.acknowledged {
        label.push_str(&format!("  ✓ {}", tr!("ping-on-my-way")));
    }
    if !ping.responders.is_empty() {
        label.push_str(&format!(
            "  ✓ {}",
            tr!("ping-responding", count = ping.responders.len().to_string())
        ));
    }
    label
}

pub(crate) fn audience(cfg: &Settings) -> &'static str {
    if !cfg.share || !cfg.send_enabled {
        return tr!("tactical-local");
    }
    match cfg.ping_audience {
        1 => tr!("tactical-selected"),
        2 => tr!("tactical-active-group"),
        super::allies::AUDIENCE => tr!("tactical-allies"),
        _ => tr!("tactical-local"),
    }
}

pub(crate) fn sharing(state: ShareState) -> &'static str {
    match state {
        ShareState::Local => tr!("ping-local"),
        ShareState::Queued => tr!("ping-queued"),
        ShareState::Attempted => tr!("ping-attempted"),
        ShareState::Failed => tr!("ping-failed"),
    }
}

pub(crate) fn active(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, identities: &std::collections::BTreeMap<Uuid, String>) {
    widgets::collapsing(ui, p, "active-pings", tr!("ping-active"), true, |ui| {
        if cfg.pings.is_empty() {
            ui.label(tr!("ping-none"));
        }
        let mut respond = None;
        for ping in &cfg.pings {
            ui.push_id(ping.marker.id, |ui| {
                widgets::surface(ui, p, |ui| {
                    let name = if ping.own() {
                        tr!("ping-you").to_owned()
                    } else {
                        identities.get(&ping.sender).cloned().unwrap_or_else(|| ping.sender.to_string())
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{name} · {}", kind_name(ping.kind)))
                            .on_hover_text(ping.sender.to_string());
                        let can_respond =
                            cfg.available() && cfg.pings_enabled && (ping.own() || cfg.share && cfg.send_enabled && !ping.acknowledged);
                        let label = if ping.own() {
                            tr!("ping-cancel")
                        } else if ping.acknowledged {
                            tr!("ping-on-my-way")
                        } else {
                            tr!("ping-on-my-way-action")
                        };
                        if widgets::action_button_if(
                            ui,
                            p,
                            if ping.own() { "x" } else { "check" },
                            label,
                            widgets::ActionKind::Flat,
                            egui::vec2(136.0, 26.0),
                            can_respond,
                        )
                        .clicked()
                        {
                            respond = Some(ping.marker.id);
                        }
                    });
                    if ping.own() {
                        ui.label(egui::RichText::new(sharing(ping.sharing)).small().color(p.muted));
                        for id in &ping.responders {
                            ui.label(format!("✓ {}", identities.get(id).cloned().unwrap_or_else(|| id.to_string())));
                        }
                    }
                });
            });
        }
        if let Some(id) = respond {
            cfg.respond(id, Instant::now());
        }
    });
}

/// A ring leaves the aim point visible. Its four sectors use the configured
/// ping colour for meaning and Aurora palette roles for their surfaces.
pub(crate) fn wheel(ctx: &egui::Context, p: &Palette, cfg: &Settings, center: Pos2, valid: bool) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("ping-wheel")));
    let selected = super::tactical::wheel_selection(ctx, center);
    let cooldown = cfg.cooldown(Instant::now());
    let ready = valid && cooldown <= 0.0;
    for kind in 0..4 {
        let angle = kind as f32 * std::f32::consts::FRAC_PI_2;
        let start = angle - std::f32::consts::FRAC_PI_4 + 0.035;
        let end = angle + std::f32::consts::FRAC_PI_4 - 0.035;
        let active = selected == Some(kind);
        let tint = color(cfg.colors[kind]);
        let face = if active { p.raised } else { p.panel }.gamma_multiply(0.9);
        let mut mesh = egui::Mesh::default();
        let mut outer = Vec::new();
        for i in 0..=24 {
            let a = start + (end - start) * i as f32 / 24.0;
            let dir = vec2(a.cos(), a.sin());
            mesh.colored_vertex(center + dir * 52.0, face);
            mesh.colored_vertex(center + dir * 136.0, face);
            outer.push(center + dir * 136.0);
            if i > 0 {
                let n = i * 2;
                mesh.add_triangle(n - 2, n - 1, n);
                mesh.add_triangle(n - 1, n + 1, n);
            }
        }
        painter.add(egui::Shape::mesh(mesh));
        painter.add(egui::Shape::line(
            outer,
            Stroke::new(if active { 3.0 } else { 1.0 }, if active { tint } else { p.muted_dim }),
        ));
        let at = center + vec2(angle.cos(), angle.sin()) * 96.0;
        paint_icon(
            &painter,
            at - vec2(0.0, 9.0),
            14.0,
            &cfg.icons[kind],
            if active { tint } else { p.ink },
        );
        let text = painter.layout(kind_name(kind).to_owned(), FontId::proportional(11.0), p.ink, 82.0);
        painter.galley(at + vec2(-text.size().x * 0.5, 10.0), text, p.ink);
    }
    let center_text = if !valid {
        tr!("ping-no-surface")
    } else if cooldown > 0.0 {
        tr!("ping-wait")
    } else if selected.is_some() {
        tr!("ping-release")
    } else {
        tr!("ping-choose")
    };
    if let Some(kind) = selected
        && ready
    {
        let title = painter.layout(
            kind_name(kind).to_owned(),
            FontId::proportional(12.0),
            color(cfg.colors[kind]),
            88.0,
        );
        painter.galley(
            center + vec2(-title.size().x * 0.5, -22.0 - title.size().y * 0.5),
            title,
            color(cfg.colors[kind]),
        );
    }
    text_shadow(
        &painter,
        center,
        Align2::CENTER_CENTER,
        center_text,
        12.0,
        if ready { p.ink } else { p.warn },
    );
    if cooldown > 0.0 {
        text_shadow(
            &painter,
            center + vec2(0.0, 20.0),
            Align2::CENTER_TOP,
            &format!("{cooldown:.1} s"),
            12.0,
            p.warn,
        );
    }
    egui::Area::new(egui::Id::unique("ping-wheel-footer"))
        .fixed_pos(center + vec2(-150.0, 151.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            widgets::overlay(ui, p, egui::Margin::same(8), |ui| {
                ui.set_width(284.0);
                ui.vertical_centered(|ui| {
                    let status = if cooldown > 0.0 {
                        format!("{cooldown:.1} s")
                    } else {
                        tr!("ping-ready").into()
                    };
                    ui.label(egui::RichText::new(format!("{} · {status}", audience(cfg))).color(p.ink));
                    ui.label(egui::RichText::new(tr!("ping-wheel-help")).small().color(p.muted));
                });
            });
        });
}

pub(crate) fn target_hint(ctx: &egui::Context, p: &Palette, cfg: &Settings, id: Uuid, at: Pos2) {
    let Some(ping) = cfg.pings.iter().find(|ping| ping.marker.id == id) else {
        return;
    };
    let text = if ping.own() {
        tr!("ping-cancel")
    } else if ping.acknowledged {
        tr!("ping-on-my-way")
    } else if cfg.share && cfg.send_enabled {
        tr!("ping-on-my-way-action")
    } else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("ping-target-hint")));
    let key = crate::keybinds::Input::Key(cfg.key.clone()).label();
    text_shadow(
        &painter,
        at + vec2(0.0, 42.0),
        Align2::CENTER_TOP,
        &format!("{key} · {text}"),
        13.0,
        p.ink,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ping_names_are_short_unicode_safe_and_do_not_block_on_lookup() {
        assert_eq!(first_name("Ødesså Drækåvik"), "Ødesså");
        assert_eq!(first_name("  Jane   Doe "), "Jane");
        assert!(!first_name("…").contains('…'));
        assert_eq!(first_name(&"é".repeat(80)).chars().count(), 24);
    }
}
