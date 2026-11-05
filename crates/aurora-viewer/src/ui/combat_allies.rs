//! The Allies tab of the Combat window: the residents you stand with. You tick
//! the residents around you, ally the group you wear and the group titles you
//! see over other residents' heads. Allies receive your pings, may send you
//! theirs, and the mini-map draws them in their own colour. The rules live in
//! `combat/allies.rs`; the pings are sent by `app/bridge.rs`.
use super::{
    combat_cards::{badge, color_row, muted, notice, switch_row},
    combat_sharing::{tile, title},
};
use crate::{
    combat::{
        allies,
        tactical::{Context, Nearby, Settings, distance_text},
    },
    theme::Palette,
    ui::widgets,
};
use aurora_i18n::tr;
use egui::{RichText, vec2};
use uuid::Uuid;

/// The tab's own state: the search of the residents list.
#[derive(Default)]
pub(crate) struct View {
    search: String,
}

/// One resident of the list.
struct Entry {
    id: Uuid,
    name: String,
    detail: String,
    distance: Option<f32>,
    /// Ticked in this list (the Sharing teammates).
    ticked: bool,
    /// An ally through the worn group or a group title, not ticked.
    through_group: bool,
    away: bool,
    title: Option<String>,
}

/// Something is declared: a ticked resident, the worn group or a group title.
fn declared(cfg: &Settings) -> bool {
    cfg.recipients.values().any(|on| *on) || cfg.allies.active_group || !cfg.allies.tags.is_empty()
}

/// True when a setting changed.
pub(crate) fn show(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, context: &Context, available: bool, view: &mut View) -> bool {
    // Each card says whether it changed a setting.
    let mut changed = false;
    let declared_before = declared(cfg);
    if !available {
        notice(ui, p, tr!("allies-unavailable"));
        ui.add_space(8.0);
    }
    ui.add_enabled_ui(available, |ui| {
        changed |= header(ui, p, cfg, context);
        ui.add_space(10.0);
        ui.add_enabled_ui(cfg.allies.enabled, |ui| {
            changed |= pings_card(ui, p, cfg);
            ui.add_space(10.0);
            if ui.available_width() >= 760.0 {
                ui.columns(2, |columns| {
                    changed |= residents(&mut columns[0], p, cfg, context, view);
                    columns[1].vertical(|ui| {
                        changed |= groups(ui, p, cfg, context);
                        ui.add_space(10.0);
                        changed |= minimap(ui, p, cfg);
                    });
                });
            } else {
                changed |= residents(ui, p, cfg, context, view);
                ui.add_space(10.0);
                changed |= groups(ui, p, cfg, context);
                ui.add_space(10.0);
                changed |= minimap(ui, p, cfg);
            }
        });
    });
    // The first ally turns the sharing of pings on, so that it is shared at once;
    // an audience the person chose on purpose (the active group) is left alone.
    if !declared_before && declared(cfg) && cfg.allies.enabled {
        cfg.share = true;
        if cfg.ping_audience <= 1 {
            cfg.ping_audience = allies::AUDIENCE;
        }
        changed = true;
    }
    changed
}

/// True when the allies switch flipped.
fn header(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, context: &Context) -> bool {
    let mut changed = false;
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            changed |= widgets::switch(ui, p, &mut cfg.allies.enabled).changed();
            ui.label(RichText::new(tr!("allies-title")).strong().size(15.0).color(p.ink));
            if cfg.allies.enabled {
                badge(ui, p, &context.allies.len().to_string(), p.violet);
            }
        });
        ui.add_space(4.0);
        muted(ui, p, tr!("allies-help"));
        if context.allies_loading {
            muted(ui, p, tr!("sharing-group-loading"));
        }
    });
    changed
}

/// True when the ping sharing switch flipped.
fn pings_card(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) -> bool {
    let mut changed = false;
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        title(ui, p, "share", tr!("allies-pings"), None);
        ui.add_space(4.0);
        let mut on = cfg.share && cfg.send_enabled && cfg.ping_audience == allies::AUDIENCE;
        if widgets::switch_labeled(ui, p, &mut on, tr!("allies-share-pings")).changed() {
            if on {
                cfg.share = true;
                cfg.send_enabled = true;
                cfg.ping_audience = allies::AUDIENCE;
            } else if cfg.ping_audience == allies::AUDIENCE {
                cfg.ping_audience = 0;
            }
            changed = true;
        }
        muted(ui, p, tr!("allies-share-pings-help"));
        if !cfg.pings_enabled {
            ui.add(egui::Label::new(RichText::new(tr!("allies-pings-off")).color(p.warn)).wrap());
        }
    });
    changed
}

fn entries(cfg: &Settings, context: &Context) -> Vec<Entry> {
    let ticked = |id: &Uuid| cfg.recipients.get(id) == Some(&true);
    let through_group = |id: &Uuid| !ticked(id) && context.allies.contains(id);
    let mut out: Vec<Entry> = context
        .nearby
        .iter()
        .map(|n: &Nearby| Entry {
            id: n.id,
            name: n.name.clone(),
            detail: distance_text(n),
            distance: n.distance,
            ticked: ticked(&n.id),
            through_group: through_group(&n.id),
            away: false,
            title: n.title.clone(),
        })
        .collect();
    for (id, _) in cfg
        .recipients
        .iter()
        .filter(|(id, on)| **on && !context.nearby.iter().any(|n| n.id == **id))
    {
        out.push(Entry {
            id: *id,
            name: context.name_of(id),
            detail: tr!("sharing-away").to_owned(),
            distance: None,
            ticked: true,
            through_group: false,
            away: true,
            title: None,
        });
    }
    // allies first, then the nearest, then by name
    out.sort_by(|a, b| {
        (b.ticked || b.through_group)
            .cmp(&(a.ticked || a.through_group))
            .then(a.distance.unwrap_or(f32::MAX).total_cmp(&b.distance.unwrap_or(f32::MAX)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// True when a resident was ticked or a title allied.
fn residents(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, context: &Context, view: &mut View) -> bool {
    let mut changed = false;
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        let ticked = cfg.recipients.values().filter(|on| **on).count();
        ui.horizontal(|ui| {
            title(ui, p, "users", tr!("allies-residents"), Some(ticked));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, p, "user-minus", tr!("allies-clear"), ticked > 0) {
                    cfg.recipients.clear();
                    changed = true;
                }
                if widgets::icon_button(ui, p, "user-plus", tr!("allies-add-nearby"), !context.nearby.is_empty()) {
                    for n in &context.nearby {
                        cfg.recipients.insert(n.id, true);
                    }
                    changed = true;
                }
            });
        });
        muted(ui, p, tr!("allies-residents-help"));
        ui.add_space(6.0);
        widgets::search_field(ui, &mut view.search, tr!("sharing-search"), 220.0);
        ui.add_space(6.0);
        let query = view.search.to_lowercase();
        let list: Vec<Entry> = entries(cfg, context)
            .into_iter()
            .filter(|e| query.is_empty() || e.name.to_lowercase().contains(&query))
            .collect();
        if list.is_empty() {
            let text = if context.nearby.is_empty() && query.is_empty() {
                tr!("sharing-nobody-nearby")
            } else {
                tr!("sharing-no-match")
            };
            ui.add_space(12.0);
            ui.vertical_centered(|ui| muted(ui, p, text));
            ui.add_space(12.0);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("allies-residents")
            .max_height(380.0)
            .show(ui, |ui| {
                for e in list {
                    changed |= resident_row(ui, p, cfg, &e);
                    ui.add_space(4.0);
                }
            });
    });
    changed
}

/// True when the row's tick or title button changed a setting.
fn resident_row(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, e: &Entry) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        tile(ui, p, e.id, &e.name);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&e.name).color(if e.away { p.muted } else { p.ink }));
                if e.through_group {
                    badge(ui, p, tr!("allies-through-group"), p.teal);
                }
            });
            let mut line = e.detail.clone();
            if let Some(title) = &e.title {
                if !line.is_empty() {
                    line.push_str(" · ");
                }
                line.push_str(title);
            }
            if !line.is_empty() {
                ui.label(RichText::new(line).size(11.0).color(p.muted));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut on = e.ticked;
            if widgets::checkbox(ui, p, &mut on, "").on_hover_text(tr!("allies-toggle")).changed() {
                if on {
                    cfg.recipients.insert(e.id, true);
                } else {
                    cfg.recipients.remove(&e.id);
                }
                changed = true;
            }
            if let Some(title) = &e.title
                && !cfg.allies.has_tag(title)
            {
                let tip = tr!("allies-add-tag-hint", title = title.clone());
                if widgets::icon_button(ui, p, "users-group", &tip, true) {
                    cfg.allies.add_tag(title);
                    changed = true;
                }
            }
        });
    });
    changed
}

fn wearing(context: &Context, tag: &str) -> usize {
    context
        .nearby
        .iter()
        .filter(|n| {
            n.title
                .as_deref()
                .is_some_and(|t| allies::clean_tag(t).to_lowercase() == tag.to_lowercase())
        })
        .count()
}

/// True when the worn group or a title changed.
fn groups(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, context: &Context) -> bool {
    let mut changed = false;
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        title(ui, p, "users-group", tr!("allies-groups"), None);
        ui.add_space(4.0);
        match &context.active_group {
            None => muted(ui, p, tr!("sharing-no-group")),
            Some(group) => {
                ui.label(RichText::new(&group.name).strong().color(p.ink));
                let line = match &group.members {
                    None => tr!("sharing-group-loading").to_owned(),
                    Some(members) => tr!(
                        "sharing-group-members",
                        count = members.len().to_string(),
                        nearby = context.nearby.iter().filter(|n| members.contains(&n.id)).count().to_string()
                    ),
                };
                muted(ui, p, &line);
            }
        }
        ui.add_enabled_ui(context.active_group.is_some(), |ui| {
            changed |= widgets::switch_labeled(ui, p, &mut cfg.allies.active_group, tr!("allies-worn-group")).changed();
        });
        // One group switch for pings and marker sets: the sets follow the allied group.
        changed |= cfg.share_group != cfg.allies.active_group;
        cfg.share_group = cfg.allies.active_group;
        muted(ui, p, tr!("allies-worn-group-help"));
        ui.add_space(10.0);
        ui.label(RichText::new(tr!("allies-tags")).strong().color(p.ink));
        muted(ui, p, tr!("allies-tags-help"));
        ui.add_space(4.0);
        if cfg.allies.tags.is_empty() {
            muted(ui, p, tr!("allies-tags-none"));
        }
        let mut remove = None;
        for tag in &cfg.allies.tags {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tag).color(p.ink));
                let n = wearing(context, tag);
                if n > 0 {
                    badge(ui, p, &tr!("allies-tag-around", count = n.to_string()), p.teal);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, p, "x", tr!("allies-tag-remove"), true) {
                        remove = Some(tag.clone());
                    }
                });
            });
        }
        if let Some(tag) = remove {
            cfg.allies.remove_tag(&tag);
            changed = true;
        }
        ui.add_space(8.0);
        ui.label(RichText::new(tr!("allies-tags-near")).size(12.0).color(p.muted));
        let mut around = crate::combat::tactical::titles_around(context);
        let any_title = !around.is_empty();
        around.retain(|(title, _)| !cfg.allies.has_tag(title));
        if around.is_empty() {
            muted(
                ui,
                p,
                if any_title {
                    tr!("allies-titles-all-allied")
                } else {
                    tr!("allies-no-titles")
                },
            );
        }
        for (title, count) in around {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&title).color(p.ink));
                ui.label(RichText::new(count.to_string()).size(11.0).color(p.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::action_button(
                        ui,
                        p,
                        "users-group",
                        tr!("allies-tag-add"),
                        widgets::ActionKind::Flat,
                        vec2(0.0, 24.0),
                    )
                    .clicked()
                    {
                        cfg.allies.add_tag(&title);
                        changed = true;
                    }
                });
            });
        }
    });
    changed
}

/// True when the mini-map look of the allies changed.
fn minimap(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) -> bool {
    let mut changed = false;
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        title(ui, p, "map", tr!("allies-minimap"), None);
        ui.add_space(4.0);
        changed |= switch_row(ui, p, &mut cfg.allies.minimap, tr!("allies-minimap-color"), "");
        ui.add_enabled_ui(cfg.allies.minimap, |ui| {
            changed |= color_row(ui, p, "allies-color", tr!("common-color"), &mut cfg.allies.color, false);
            changed |= switch_row(
                ui,
                p,
                &mut cfg.allies.ring,
                tr!("allies-minimap-ring"),
                tr!("allies-minimap-ring-help"),
            );
        });
    });
    changed
}
