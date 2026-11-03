//! The Sharing tab of the Combat window: who receives your pings and marker
//! sets, whom you accept them from, what is on its way and what arrived.
//! The rules themselves live in `app/bridge.rs`; this page only shows them.
use super::combat_cards::{badge, muted, notice, pill};
use crate::{
    combat::tactical::{Context, Settings},
    theme::Palette,
    ui::widgets,
};
use aurora_i18n::tr;
use egui::{RichText, Vec2, vec2};
use std::collections::BTreeSet;
use uuid::Uuid;

/// True when a setting changed.
/// `goto` asks the window to open another tab (the Allies one).
pub(crate) fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    cfg: &mut Settings,
    context: &Context,
    available: bool,
    goto: &mut Option<usize>,
) -> bool {
    let before = cfg.clone();
    if !available {
        notice(ui, p, tr!("sharing-unavailable"));
        ui.add_space(8.0);
    }
    ui.add_enabled_ui(available, |ui| {
        header(ui, p, cfg, context);
        ui.add_space(10.0);
        ui.add_enabled_ui(cfg.share, |ui| {
            audience(ui, p, cfg, context);
            ui.add_space(10.0);
            if ui.available_width() >= 760.0 {
                ui.columns(2, |columns| {
                    columns[0].vertical(|ui| {
                        who_card(ui, p, context, goto);
                        ui.add_space(10.0);
                        inbox_card(ui, p, cfg, context);
                    });
                    send_card(&mut columns[1], p, cfg, context);
                });
            } else {
                who_card(ui, p, context, goto);
                ui.add_space(10.0);
                send_card(ui, p, cfg, context);
                ui.add_space(10.0);
                inbox_card(ui, p, cfg, context);
            }
        });
    });
    *cfg != before
}

/// The members of the active group, when they are known.
fn group_members(context: &Context) -> Option<&BTreeSet<Uuid>> {
    context.active_group.as_ref().and_then(|g| g.members.as_ref())
}

fn teammate_ids(cfg: &Settings) -> BTreeSet<Uuid> {
    cfg.recipients.iter().filter(|(_, on)| **on).map(|(id, _)| *id).collect()
}

/// How many residents your pings reach; None while the group loads.
fn ping_reach(cfg: &Settings, context: &Context) -> Option<usize> {
    match cfg.ping_audience {
        1 => Some(teammate_ids(cfg).len()),
        2 => group_members(context).map(|m| m.len().saturating_sub(1)),
        crate::combat::allies::AUDIENCE => (!context.allies_loading).then_some(context.allies.len()),
        _ => Some(0),
    }
}

/// How many residents a marker set reaches; None while the group loads.
fn set_reach(cfg: &Settings, context: &Context) -> Option<usize> {
    let mut ids = teammate_ids(cfg);
    if cfg.share_group {
        ids.extend(group_members(context)?.iter().copied());
    }
    Some(ids.len().saturating_sub(usize::from(cfg.share_group)))
}

pub(super) fn title(ui: &mut egui::Ui, p: &Palette, icon: &str, text: &str, count: Option<usize>) {
    ui.horizontal(|ui| {
        if let Some(picture) = crate::ui::icons::global(icon) {
            ui.add(egui::Image::new((picture.id(), Vec2::splat(16.0))).tint(p.muted));
        }
        ui.label(RichText::new(text).strong().size(14.0).color(p.ink));
        if let Some(n) = count {
            badge(ui, p, &n.to_string(), p.violet);
        }
    });
}

fn header(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, context: &Context) {
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::switch(ui, p, &mut cfg.share);
            ui.label(RichText::new(tr!("tactical-share")).strong().size(15.0).color(p.ink));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(cfg.share, |ui| {
                    pill(
                        ui,
                        p,
                        &mut cfg.receive_enabled,
                        tr!("sharing-receiving"),
                        tr!("sharing-receiving-help"),
                    );
                    pill(ui, p, &mut cfg.send_enabled, tr!("sharing-sending"), tr!("sharing-sending-help"));
                });
            });
        });
        ui.add_space(4.0);
        muted(ui, p, &summary(cfg, context));
    });
}

/// One line saying where pings go and whom you accept from.
fn summary(cfg: &Settings, context: &Context) -> String {
    if !cfg.share {
        return tr!("sharing-summary-off").to_owned();
    }
    let sending = if !cfg.send_enabled {
        tr!("sharing-summary-not-sending").to_owned()
    } else {
        match ping_reach(cfg, context) {
            Some(0) => tr!("sharing-summary-pings-none").to_owned(),
            Some(n) => tr!("sharing-summary-pings", count = n.to_string()),
            None => tr!("sharing-group-loading").to_owned(),
        }
    };
    let receiving = if !cfg.receive_enabled {
        tr!("sharing-summary-not-receiving").to_owned()
    } else {
        tr!("sharing-summary-accept", count = context.allies.len().to_string())
    };
    format!("{sending} · {receiving}")
}

fn audience(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, context: &Context) {
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        title(ui, p, "share", tr!("tactical-ping-sharing"), None);
        ui.add_space(4.0);
        ui.add_enabled_ui(cfg.send_enabled, |ui| {
            let mut selected = usize::from(cfg.ping_audience == crate::combat::allies::AUDIENCE);
            let choices = [(tr!("tactical-local"), false), (tr!("tactical-allies"), false)];
            if widgets::segmented(ui, p, &mut selected, &choices) {
                cfg.ping_audience = if selected == 1 { crate::combat::allies::AUDIENCE } else { 0 };
            }
        });
        let reach = match (cfg.ping_audience, ping_reach(cfg, context)) {
            (0, _) => tr!("sharing-pings-only-me").to_owned(),
            (_, None) => tr!("sharing-group-loading").to_owned(),
            (_, Some(0)) => tr!("sharing-pings-nobody").to_owned(),
            (_, Some(n)) => tr!("sharing-summary-pings", count = n.to_string()),
        };
        muted(ui, p, &reach);
    });
}

/// Initials on a coloured tile, the colour picked from the resident's id.
pub(super) fn tile(ui: &mut egui::Ui, p: &Palette, id: Uuid, name: &str) {
    let colors = [p.violet, p.teal, p.indigo, p.amber, p.rose];
    let color = colors[id.as_bytes()[0] as usize % colors.len()];
    let initials: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, ui.visuals().window_corner_radius, color.gamma_multiply(0.85));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if initials.is_empty() { "?".into() } else { initials },
        egui::FontId::proportional(12.5),
        p.on_accent,
    );
}

/// Who receives: the allies, kept in the Allies tab, with the way there.
fn who_card(ui: &mut egui::Ui, p: &Palette, context: &Context, goto: &mut Option<usize>) {
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        title(ui, p, "heart-handshake", tr!("sharing-who"), Some(context.allies.len()));
        muted(ui, p, tr!("sharing-who-help"));
        ui.add_space(6.0);
        if widgets::action_button(
            ui,
            p,
            "heart-handshake",
            tr!("sharing-manage-allies"),
            widgets::ActionKind::Flat,
            vec2(0.0, 28.0),
        )
        .clicked()
        {
            *goto = Some(super::combat_window::ALLIES_TAB);
        }
    });
}

fn send_card(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, context: &Context) {
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        title(ui, p, "send-filled", tr!("sharing-send-title"), None);
        let name = cfg.active_set_name();
        let selected = cfg.markers.iter().filter(|m| m.selected).count();
        muted(
            ui,
            p,
            &tr!(
                "sharing-send-set",
                name = if name.is_empty() { "—".to_owned() } else { name },
                count = cfg.markers.len().to_string(),
                selected = selected.to_string()
            ),
        );
        let reach = set_reach(cfg, context);
        let reason = if !cfg.send_enabled {
            Some(tr!("sharing-send-off"))
        } else if reach == Some(0) {
            Some(tr!("sharing-send-nobody"))
        } else if reach.is_none() {
            Some(tr!("sharing-group-loading"))
        } else {
            None
        };
        let can = reason.is_none() && cfg.available();
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let size = vec2(0.0, 28.0);
            if widgets::action_button_if(
                ui,
                p,
                "send-filled",
                tr!("tactical-send"),
                widgets::ActionKind::Primary,
                size,
                can && selected > 0,
            )
            .clicked()
            {
                cfg.send_markers(true);
            }
            if widgets::action_button_if(
                ui,
                p,
                "share",
                tr!("tactical-send-set"),
                widgets::ActionKind::Flat,
                size,
                can && !cfg.markers.is_empty(),
            )
            .clicked()
            {
                cfg.send_markers(false);
            }
        });
        if let Some(reason) = reason {
            muted(ui, p, reason);
        } else if let Some(n) = reach {
            muted(ui, p, &tr!("sharing-send-reach", count = n.to_string()));
        }
        if !cfg.set_outgoing.is_empty() && cfg.set_total > 0 {
            let done = cfg.set_total - cfg.set_outgoing.len().min(cfg.set_total);
            let text = tr!("sharing-progress", done = done.to_string(), total = cfg.set_total.to_string());
            ui.add_space(4.0);
            widgets::progress(
                ui,
                p,
                "sharing-progress",
                done as f32 / cfg.set_total as f32,
                ui.available_width(),
                &text,
                None,
            );
        }
        if cfg.share_error {
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(tr!("tactical-share-error")).color(p.warn)).wrap());
        }
    });
}

fn inbox_card(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, context: &Context) {
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        let count = cfg.inbox.previews.len();
        title(ui, p, "inbox", tr!("sharing-inbox"), (count > 0).then_some(count));
        if !cfg.receive_enabled {
            muted(ui, p, tr!("sharing-summary-not-receiving"));
        }
        if count == 0 {
            muted(ui, p, tr!("sharing-inbox-empty"));
            return;
        }
        let in_region = cfg.current_region.is_some();
        let mut remove = None;
        for (index, set) in cfg.inbox.previews.iter().enumerate() {
            ui.add_space(4.0);
            widgets::surface(ui, p, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&set.name).strong().color(p.ink));
                let sender = cfg
                    .inbox
                    .senders
                    .get(&set.id)
                    .map(|id| context.name_of(id))
                    .unwrap_or_else(|| "…".into());
                let shapes: BTreeSet<&str> = set.markers.iter().map(|m| m.shape().label()).collect();
                muted(
                    ui,
                    p,
                    &tr!(
                        "sharing-inbox-line",
                        sender = sender,
                        count = set.markers.len().to_string(),
                        shapes = shapes.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                );
                ui.horizontal(|ui| {
                    let save = widgets::action_button_if(
                        ui,
                        p,
                        "download-filled",
                        tr!("tactical-import-copy"),
                        widgets::ActionKind::Primary,
                        vec2(0.0, 26.0),
                        in_region,
                    );
                    let save = if in_region {
                        save
                    } else {
                        save.on_disabled_hover_text(tr!("tactical-set-region-required"))
                    };
                    if save.clicked() {
                        remove = Some((index, true));
                    }
                    if widgets::action_button(ui, p, "x", tr!("sharing-dismiss"), widgets::ActionKind::Flat, vec2(0.0, 26.0)).clicked() {
                        remove = Some((index, false));
                    }
                });
            });
        }
        if let Some((index, save)) = remove {
            let set = cfg.inbox.previews.remove(index);
            cfg.inbox.senders.remove(&set.id);
            if save && let Some(key) = cfg.current_region.clone() {
                let region = cfg.library.region_mut(&key, "");
                let active = region.active;
                region.add(set.name.clone(), &set.markers);
                region.active = active;
            }
        }
    });
}
