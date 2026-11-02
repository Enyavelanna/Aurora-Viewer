//! Compact reward studio for Combat: one selected medal, one preview and one
//! editor. The same presentation metadata drives the in-world medal banner.
use super::{combat_cards, options, widgets};
use crate::{
    combat::awards::{Award, Family},
    theme::Palette,
};
use aurora_i18n::tr;
use egui::{Color32, RichText, Vec2};
use uuid::Uuid;

/// The longest name a medal can be given.
const NAME_LEN: usize = 32;

pub(crate) fn accent(p: &Palette, award: Award) -> Color32 {
    match award.family() {
        Family::Combo => p.violet_light,
        Family::Streak => p.amber,
        Family::Event => p.teal,
    }
}

pub(crate) fn icon(award: Award) -> &'static str {
    match award.family() {
        Family::Combo => "bolt-filled",
        Family::Streak => "crown-filled",
        Family::Event => "trophy-filled",
    }
}

fn family_name(family: Family) -> &'static str {
    match family {
        Family::Combo => tr!("combat-reward-family-rapid"),
        Family::Streak => tr!("combat-reward-family-streak"),
        Family::Event => tr!("combat-reward-family-rivalry"),
    }
}

pub(crate) fn family_label(award: Award) -> &'static str {
    family_name(award.family())
}

/// What earns the medal, in a sentence.
pub(crate) fn rule(award: Award) -> &'static str {
    match award {
        Award::Double => tr!("combat-reward-rule-double"),
        Award::Triple => tr!("combat-reward-rule-triple"),
        Award::Quad => tr!("combat-reward-rule-quad"),
        Award::Penta => tr!("combat-reward-rule-penta"),
        Award::Streak10 => tr!("combat-reward-rule-streak10"),
        Award::Streak20 => tr!("combat-reward-rule-streak20"),
        Award::Streak30 => tr!("combat-reward-rule-streak30"),
        Award::Streak50 => tr!("combat-reward-rule-streak50"),
        Award::Revenge => tr!("combat-reward-rule-revenge"),
        Award::Domination => tr!("combat-reward-rule-dominating"),
        Award::Shutdown => tr!("combat-reward-rule-shutdown"),
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
    cfg.migrate_rewards();
    state.combat_reward_selected = state.combat_reward_selected.min(awards.len() - 1);
    let mut changed = false;
    let selected = awards[state.combat_reward_selected];

    ui.vertical(|ui| {
        widgets::surface(ui, &p, |ui| {
            ui.horizontal_top(|ui| {
                let texture = cfg
                    .medal_textures
                    .get(selected.slot())
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
                    ui.label(RichText::new(cfg.medal_name(selected)).size(20.0).strong());
                    ui.label(RichText::new(rule(selected)).color(p.muted));
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        asset_state(
                            ui,
                            &p,
                            "volume-filled",
                            tr!("combat-reward-sound-ready"),
                            configured(&cfg.announcement_assets[selected.slot()]),
                        );
                        asset_state(
                            ui,
                            &p,
                            "photo-filled",
                            tr!("combat-reward-image-ready"),
                            configured(&cfg.medal_textures[selected.slot()]),
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
        let mut category = selected.family().index();
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
            state.combat_reward_selected = Family::ALL[category].slots().start;
        }
        let family = Family::ALL[category];

        // the family: on or off, and how loud
        ui.add_space(4.0);
        changed |= combat_cards::switch_row(
            ui,
            &p,
            &mut cfg.family_on[category],
            tr!("combat-reward-family-on"),
            tr!("combat-reward-family-on-help"),
        );
        let on = cfg.family_on[category];
        changed |= combat_cards::under(ui, on, |ui| {
            combat_cards::slider_row(
                ui,
                tr!("combat-reward-family-volume"),
                tr!("combat-reward-family-volume-help"),
                &mut cfg.family_volume[category],
                0.0..=1.0,
                "",
            )
        });

        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(245.0);
                for slot in family.slots() {
                    let award = awards[slot];
                    let sound = configured(&cfg.announcement_assets[slot]);
                    let image = configured(&cfg.medal_textures[slot]);
                    let count = if !cfg.medal_on(award) {
                        tr!("combat-reward-off")
                    } else {
                        match (sound, image) {
                            (true, true) => tr!("combat-reward-complete"),
                            (true, false) => tr!("combat-reward-sound-only"),
                            (false, true) => tr!("combat-reward-image-only"),
                            (false, false) => tr!("combat-reward-empty"),
                        }
                    };
                    if widgets::navigation_wrapped(
                        ui,
                        &p,
                        icon(award),
                        cfg.medal_name(award),
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
                            ui.label(RichText::new(cfg.medal_name(award)).size(16.0).strong());
                            ui.label(RichText::new(family_label(award)).size(11.0).color(p.muted));
                        });
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new(tr!("combat-reward-behaviour")).strong());
                    ui.label(RichText::new(rule(award)).color(p.muted));
                    ui.add_space(8.0);
                    changed |= combat_cards::switch_row(
                        ui,
                        &p,
                        &mut cfg.medal_enabled[slot],
                        tr!("combat-reward-enabled"),
                        tr!("combat-reward-enabled-help"),
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new(tr!("combat-reward-name")).strong());
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut cfg.medal_names[slot])
                                    .char_limit(NAME_LEN)
                                    .hint_text(award.label()),
                            )
                            .changed();
                        if widgets::icon_button(
                            ui,
                            &p,
                            "arrow-back-up",
                            tr!("combat-reward-name-reset"),
                            !cfg.medal_names[slot].is_empty(),
                        ) {
                            cfg.medal_names[slot].clear();
                            changed = true;
                        }
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
