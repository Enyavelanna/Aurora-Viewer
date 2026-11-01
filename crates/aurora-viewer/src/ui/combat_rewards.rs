//! Compact reward studio for Combat: one selected event, one preview and one
//! editor. The same presentation metadata drives the in-world medal banner.
use super::{options, widgets};
use crate::{combat::awards::Award, theme::Palette};
use aurora_i18n::tr;
use egui::{Color32, RichText, Vec2};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Rapid,
    Streak,
    Rivalry,
}

fn family(award: Award) -> Family {
    match award {
        Award::First | Award::Double | Award::Triple | Award::Multi | Award::Ultra => Family::Rapid,
        Award::Five | Award::Ten | Award::Fifteen | Award::Twenty | Award::Thirty => Family::Streak,
        Award::Revenge | Award::Dominating | Award::Comeback => Family::Rivalry,
    }
}

fn family_index(award: Award) -> usize {
    match family(award) {
        Family::Rapid => 0,
        Family::Streak => 1,
        Family::Rivalry => 2,
    }
}

fn family_range(index: usize) -> std::ops::Range<usize> {
    match index {
        1 => 5..10,
        2 => 10..13,
        _ => 0..5,
    }
}

pub(crate) fn accent(p: &Palette, award: Award) -> Color32 {
    match family(award) {
        Family::Rapid => p.violet_light,
        Family::Streak => p.amber,
        Family::Rivalry => p.teal,
    }
}

pub(crate) fn icon(award: Award) -> &'static str {
    match family(award) {
        Family::Rapid => "bolt-filled",
        Family::Streak => "crown-filled",
        Family::Rivalry => "trophy-filled",
    }
}

pub(crate) fn family_label(award: Award) -> &'static str {
    match family(award) {
        Family::Rapid => tr!("combat-reward-family-rapid"),
        Family::Streak => tr!("combat-reward-family-streak"),
        Family::Rivalry => tr!("combat-reward-family-rivalry"),
    }
}

pub(crate) fn rule(award: Award) -> &'static str {
    match award {
        Award::First => tr!("combat-reward-rule-first"),
        Award::Double => tr!("combat-reward-rule-double"),
        Award::Triple => tr!("combat-reward-rule-triple"),
        Award::Multi => tr!("combat-reward-rule-multi"),
        Award::Ultra => tr!("combat-reward-rule-ultra"),
        Award::Five => tr!("combat-reward-rule-five"),
        Award::Ten => tr!("combat-reward-rule-ten"),
        Award::Fifteen => tr!("combat-reward-rule-fifteen"),
        Award::Twenty => tr!("combat-reward-rule-twenty"),
        Award::Thirty => tr!("combat-reward-rule-thirty"),
        Award::Revenge => tr!("combat-reward-rule-revenge"),
        Award::Dominating => tr!("combat-reward-rule-dominating"),
        Award::Comeback => tr!("combat-reward-rule-comeback"),
    }
}

fn configured(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| !id.is_nil())
}

fn asset_state(ui: &mut egui::Ui, p: &Palette, icon: &str, label: &str, ready: bool) {
    ui.horizontal(|ui| {
        widgets::glyph(ui, icon, if ready { p.success } else { p.muted_dim }, 14.0);
        ui.label(RichText::new(label).size(11.0).color(if ready { p.muted } else { p.muted_dim }));
    });
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    p: Palette,
    cfg: &mut crate::combat::Settings,
    state: &mut options::OptionsUi,
    result: &mut options::OptionsResult,
) -> bool {
    let awards = Award::ALL;
    cfg.announcement_assets.resize(awards.len(), String::new());
    cfg.medal_textures.resize(awards.len(), String::new());
    state.combat_reward_selected = state.combat_reward_selected.min(awards.len() - 1);
    let mut changed = false;
    let selected = awards[state.combat_reward_selected];

    ui.vertical(|ui| {
        widgets::surface(ui, &p, |ui| {
            ui.horizontal_top(|ui| {
                let texture = cfg
                    .medal_textures
                    .get(state.combat_reward_selected)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .and_then(|id| state.combat_medal_images.get(&id));
                widgets::picture(ui, &p, texture, Vec2::splat(92.0), icon(selected), tr!("combat-reward-no-image"));
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(tr!("combat-reward-studio"))
                            .size(11.0)
                            .strong()
                            .color(accent(&p, selected)),
                    );
                    ui.label(RichText::new(selected.label()).size(20.0).strong());
                    ui.label(RichText::new(rule(selected)).color(p.muted));
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        asset_state(
                            ui,
                            &p,
                            "volume-filled",
                            tr!("combat-reward-sound-ready"),
                            configured(&cfg.announcement_assets[state.combat_reward_selected]),
                        );
                        asset_state(
                            ui,
                            &p,
                            "photo-filled",
                            tr!("combat-reward-image-ready"),
                            configured(&cfg.medal_textures[state.combat_reward_selected]),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if widgets::action_button_if(
                        ui,
                        &p,
                        "player-play-filled",
                        tr!("combat-test-reward"),
                        widgets::ActionKind::Primary,
                        Vec2::new(0.0, 30.0),
                        cfg.enabled,
                    )
                    .clicked()
                    {
                        result.combat_preview = Some(state.combat_reward_selected);
                    }
                });
            });
        });

        ui.add_space(8.0);
        let mut category = family_index(selected);
        if widgets::segmented(
            ui,
            &p,
            &mut category,
            &[
                (tr!("combat-reward-family-rapid"), true),
                (tr!("combat-reward-family-streak"), true),
                (tr!("combat-reward-family-rivalry"), true),
            ],
        ) {
            state.combat_reward_selected = family_range(category).start;
        }

        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(245.0);
                for slot in family_range(category) {
                    let award = awards[slot];
                    let sound = configured(&cfg.announcement_assets[slot]);
                    let image = configured(&cfg.medal_textures[slot]);
                    let count = match (sound, image) {
                        (true, true) => tr!("combat-reward-complete"),
                        (true, false) => tr!("combat-reward-sound-only"),
                        (false, true) => tr!("combat-reward-image-only"),
                        (false, false) => tr!("combat-reward-empty"),
                    };
                    if widgets::navigation_wrapped(
                        ui,
                        &p,
                        icon(award),
                        award.label(),
                        Some(count),
                        state.combat_reward_selected == slot,
                    )
                    .clicked()
                    {
                        state.combat_reward_selected = slot;
                    }
                }
            });
            ui.add_space(10.0);
            widgets::surface(ui, &p, |ui| {
                ui.vertical(|ui| {
                    ui.set_min_width((ui.available_width() - 4.0).max(300.0));
                    let slot = state.combat_reward_selected;
                    let award = awards[slot];
                    ui.horizontal(|ui| {
                        widgets::glyph(ui, icon(award), accent(&p, award), 20.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(award.label()).size(16.0).strong());
                            ui.label(RichText::new(family_label(award)).size(11.0).color(p.muted));
                        });
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new(tr!("combat-sound-uuid")).strong());
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut cfg.announcement_assets[slot]).hint_text(tr!("combat-reward-optional-sound")))
                        .changed();
                    ui.add_space(8.0);
                    ui.label(RichText::new(tr!("combat-medal-image-uuid")).strong());
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut cfg.medal_textures[slot]).hint_text(tr!("combat-reward-optional-image")))
                        .changed();
                    ui.add_space(8.0);
                    ui.label(RichText::new(tr!("combat-reward-behaviour")).strong());
                    ui.label(RichText::new(rule(award)).color(p.muted));
                });
            });
        });

        ui.add_space(8.0);
        widgets::collapsing(ui, &p, "reward-rules", tr!("combat-rules"), false, |ui| {
            ui.label(tr!("combat-rules-about"));
        });
    });
    changed
}
