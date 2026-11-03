//! The seven tabs of the Combat window, each a dashboard of cards
//! (`combat_cards`). They edit the settings directly: Combat's settings
//! live in this window and nowhere else.
use super::combat_cards::{Card, color_row, columns, field_row, gap, muted, notice, slider_row, switch_row, text_row, under};
use super::options::{OptionsResult, OptionsUi};
use crate::{combat::tactical::Settings as Tactical, settings::Settings, theme::Palette};
use aurora_i18n::tr;

/// One tab. True when a setting changed.
pub(crate) fn show(ui: &mut egui::Ui, p: &Palette, s: &mut Settings, st: &mut OptionsUi, r: &mut OptionsResult, tab: usize) -> bool {
    let d = Settings::default();
    let mut changed = false;
    if !s.combat.enabled && tab != 6 {
        notice(ui, p, tr!("combat-off-notice"));
        gap(ui);
    }
    ui.add_enabled_ui(s.combat.enabled || tab == 6, |ui| {
        changed = match tab {
            0 => overview(ui, p, s, &d),
            1 => feedback(ui, p, s, &d),
            2 => awards(ui, p, s, st, r),
            3 => minimap(ui, p, s, st, &d),
            4 => marker_sets(ui, p, s, st),
            5 => pings(ui, p, s, st, &d),
            6 => {
                let available = s.combat.enabled && s.maps.tactical.tools_enabled;
                super::combat_sharing::show(ui, p, &mut s.maps.tactical, &st.tactical_context, available, &mut st.combat_sharing)
            }
            _ => {
                let available = s.combat.enabled && s.maps.tactical.tools_enabled;
                super::combat_allies::show(ui, p, &mut s.maps.tactical, &st.tactical_context, available, &mut st.combat_allies)
            }
        };
    });
    changed
}

/// The mini-map tools can work: Combat mode and their own switch.
fn tools_off(s: &Settings) -> bool {
    !s.combat.enabled || !s.maps.tactical.tools_enabled
}

fn overview(ui: &mut egui::Ui, p: &Palette, s: &mut Settings, _d: &Settings) -> bool {
    let c = &s.combat;
    let t = &s.maps.tactical;
    let on = [
        c.journal,
        c.feed,
        c.killmarker,
        c.kill_sound,
        c.voices,
        c.medals,
        t.enabled,
        t.pings_enabled,
        t.share,
    ]
    .iter()
    .filter(|v| **v)
    .count();
    let mut changed = false;
    Card::new("crosshair", tr!("combat-preferences-title"))
        .badge(tr!("combat-overview-count", count = on.to_string(), total = "9".to_owned()))
        .help(tr!("combat-preferences-about"))
        .show(ui, p, |_| {});
    gap(ui);
    columns(
        ui,
        &mut (s, &mut changed),
        |ui, (s, changed)| {
            Card::new("bolt-filled", tr!("combat-page-feedback")).show(ui, p, |ui| {
                **changed |= switch_row(ui, p, &mut s.combat.journal, tr!("combat-journal"), "");
                **changed |= switch_row(ui, p, &mut s.combat.show_tab, tr!("combat-show-tab"), "");
                **changed |= switch_row(ui, p, &mut s.combat.feed, tr!("combat-feed"), "");
                **changed |= switch_row(ui, p, &mut s.combat.killmarker, tr!("combat-killmarker"), "");
                **changed |= switch_row(ui, p, &mut s.combat.kill_sound, tr!("combat-kill-sound"), "");
            });
            gap(ui);
            Card::new("star", tr!("combat-page-rewards")).show(ui, p, |ui| {
                **changed |= switch_row(ui, p, &mut s.combat.voices, tr!("combat-voices"), "");
                **changed |= switch_row(ui, p, &mut s.combat.medals, tr!("combat-medals"), "");
            });
        },
        |ui, (s, changed)| {
            let off = tools_off(s);
            Card::new("map", tr!("combat-overview-tactical")).show(ui, p, |ui| {
                **changed |= switch_row(
                    ui,
                    p,
                    &mut s.maps.tactical.tools_enabled,
                    tr!("tactical-tools"),
                    tr!("tactical-tools-help"),
                );
                ui.add_enabled_ui(!off, |ui| {
                    **changed |= switch_row(ui, p, &mut s.maps.tactical.enabled, tr!("tactical-markers"), "");
                    **changed |= switch_row(ui, p, &mut s.maps.tactical.pings_enabled, tr!("tactical-pings"), "");
                    **changed |= switch_row(ui, p, &mut s.maps.tactical.share, tr!("tactical-share"), "");
                });
            });
        },
    );
    changed
}

fn feedback(ui: &mut egui::Ui, p: &Palette, s: &mut Settings, d: &Settings) -> bool {
    let mut changed = false;
    columns(
        ui,
        &mut (s, &mut changed),
        |ui, (s, changed)| {
            let c = &mut s.combat;
            let dc = &d.combat;
            if Card::new("history", tr!("combat-journal-title"))
                .modified(c.journal != dc.journal || c.show_tab != dc.show_tab)
                .show(ui, p, |ui| {
                    **changed |= switch_row(ui, p, &mut c.journal, tr!("combat-journal"), "");
                    **changed |= switch_row(ui, p, &mut c.show_tab, tr!("combat-show-tab"), "");
                })
            {
                (c.journal, c.show_tab) = (dc.journal, dc.show_tab);
                **changed = true;
            }
            gap(ui);
            if Card::new("crosshair", tr!("combat-zoom-sensitivity"))
                .help(tr!("combat-zoom-sensitivity-help"))
                .modified(c.zoom_sensitivity != dc.zoom_sensitivity)
                .show(ui, p, |ui| {
                    **changed |= slider_row(ui, tr!("combat-zoom-sensitivity"), "", &mut c.zoom_sensitivity, 0.0..=1.0, "");
                })
            {
                c.zoom_sensitivity = dc.zoom_sensitivity;
                **changed = true;
            }
            gap(ui);
            let marker = (c.killmarker, c.marker_size, c.marker_seconds, c.marker_opacity, c.marker_color);
            let marker_default = (dc.killmarker, dc.marker_size, dc.marker_seconds, dc.marker_opacity, dc.marker_color);
            if Card::new("plus", tr!("combat-killmarker"))
                .modified(marker != marker_default)
                .show(ui, p, |ui| {
                    **changed |= switch_row(ui, p, &mut c.killmarker, tr!("combat-killmarker"), "");
                    let on = c.killmarker;
                    **changed |= under(ui, on, |ui| {
                        let mut any = slider_row(ui, tr!("combat-size"), "", &mut c.marker_size, 4.0..=80.0, "");
                        any |= slider_row(ui, tr!("combat-duration"), "", &mut c.marker_seconds, 0.05..=2.0, "");
                        any |= slider_row(ui, tr!("combat-opacity"), "", &mut c.marker_opacity, 0.0..=1.0, "");
                        any | color_row(ui, p, "hitmarker-color", tr!("common-color"), &mut c.marker_color, true)
                    });
                })
            {
                (c.killmarker, c.marker_size, c.marker_seconds, c.marker_opacity, c.marker_color) = marker_default;
                **changed = true;
            }
        },
        |ui, (s, changed)| {
            let c = &mut s.combat;
            let dc = &d.combat;
            let sound = (c.kill_sound, c.kill_asset.clone(), c.kill_volume);
            if Card::new("volume", tr!("combat-kill-sound"))
                .modified(sound != (dc.kill_sound, dc.kill_asset.clone(), dc.kill_volume))
                .show(ui, p, |ui| {
                    **changed |= switch_row(ui, p, &mut c.kill_sound, tr!("combat-kill-sound"), "");
                    let on = c.kill_sound;
                    **changed |= under(ui, on, |ui| {
                        let any = text_row(ui, tr!("combat-asset"), "", &mut c.kill_asset);
                        any | slider_row(ui, tr!("combat-volume"), "", &mut c.kill_volume, 0.0..=1.0, "")
                    });
                })
            {
                (c.kill_sound, c.kill_asset, c.kill_volume) = (dc.kill_sound, dc.kill_asset.clone(), dc.kill_volume);
                **changed = true;
            }
            gap(ui);
            let feed = |c: &crate::combat::Settings| {
                (
                    (c.feed, c.feed_size, c.feed_opacity, c.feed_seconds, c.feed_entries),
                    (c.feed_position, c.feed_medals, c.feed_style, c.feed_filter),
                    (c.feed_avatars_only, c.feed_always_mine, c.feed_damage, c.feed_snap),
                )
            };
            if Card::new("list-details-filled", tr!("combat-feed"))
                .modified(feed(c) != feed(dc))
                .show(ui, p, |ui| {
                    **changed |= switch_row(ui, p, &mut c.feed, tr!("combat-feed"), "");
                    **changed |= switch_row(ui, p, &mut c.feed_test, tr!("combat-feed-test"), tr!("combat-feed-test-help"));
                    let on = c.feed;
                    **changed |= under(ui, on, |ui| {
                        let mut style = usize::from(c.feed_style.min(crate::combat::feed::STYLE_CARDS));
                        let styles = [(tr!("combat-feed-style-compact"), false), (tr!("combat-feed-style-cards"), false)];
                        let mut any = field_row(ui, tr!("combat-feed-style"), "", |ui| {
                            super::widgets::segmented(ui, p, &mut style, &styles)
                        });
                        c.feed_style = style as u8;
                        let mut filter = usize::from(c.feed_filter.min(crate::combat::feed::FILTER_ALLIES));
                        let filters = [
                            (tr!("combat-feed-filter-all"), false),
                            (tr!("combat-feed-filter-mine"), false),
                            (tr!("combat-feed-filter-allies"), false),
                        ];
                        any |= field_row(ui, tr!("combat-feed-filter"), tr!("combat-feed-filter-help"), |ui| {
                            super::widgets::segmented(ui, p, &mut filter, &filters)
                        });
                        c.feed_filter = filter as u8;
                        any |= switch_row(ui, p, &mut c.feed_always_mine, tr!("combat-feed-always-mine"), "");
                        any |= switch_row(ui, p, &mut c.feed_avatars_only, tr!("combat-feed-avatars-only"), "");
                        any |= switch_row(ui, p, &mut c.feed_damage, tr!("combat-feed-damage"), tr!("combat-feed-damage-help"));
                        any |= switch_row(ui, p, &mut c.feed_medals, tr!("combat-feed-medals"), tr!("combat-feed-medals-help"));
                        any |= switch_row(ui, p, &mut c.feed_snap, tr!("combat-feed-snap"), "");
                        any |= slider_row(ui, tr!("combat-size"), "", &mut c.feed_size, 9.0..=32.0, "");
                        any |= slider_row(ui, tr!("combat-opacity"), "", &mut c.feed_opacity, 0.1..=1.0, "");
                        any |= slider_row(ui, tr!("combat-duration"), "", &mut c.feed_seconds, 1.0..=30.0, "");
                        any |= slider_row(ui, tr!("combat-entries"), "", &mut c.feed_entries, 1..=20, "");
                        any |= slider_row(ui, tr!("combat-horizontal"), "", &mut c.feed_position[0], 0.0..=1.0, "");
                        any | slider_row(ui, tr!("combat-vertical"), "", &mut c.feed_position[1], 0.0..=1.0, "")
                    });
                })
            {
                (c.feed, c.feed_size, c.feed_opacity, c.feed_seconds, c.feed_entries) =
                    (dc.feed, dc.feed_size, dc.feed_opacity, dc.feed_seconds, dc.feed_entries);
                (c.feed_position, c.feed_medals, c.feed_style, c.feed_filter) =
                    (dc.feed_position, dc.feed_medals, dc.feed_style, dc.feed_filter);
                (c.feed_avatars_only, c.feed_always_mine, c.feed_damage, c.feed_snap) =
                    (dc.feed_avatars_only, dc.feed_always_mine, dc.feed_damage, dc.feed_snap);
                **changed = true;
            }
        },
    );
    changed
}

fn awards(ui: &mut egui::Ui, p: &Palette, s: &mut Settings, st: &mut OptionsUi, r: &mut OptionsResult) -> bool {
    let mut changed = false;
    let d = crate::combat::Settings::default();
    let c = &mut s.combat;
    if Card::new("star", tr!("combat-rewards"))
        .modified(c.voices != d.voices || c.medals != d.medals || c.voice_volume != d.voice_volume)
        .show(ui, p, |ui| {
            changed |= switch_row(ui, p, &mut c.voices, tr!("combat-voices"), "");
            let on = c.voices;
            changed |= under(ui, on, |ui| {
                slider_row(ui, tr!("combat-volume"), "", &mut c.voice_volume, 0.0..=1.0, "")
            });
            changed |= switch_row(ui, p, &mut c.medals, tr!("combat-medals"), "");
        })
    {
        (c.voices, c.medals, c.voice_volume) = (d.voices, d.medals, d.voice_volume);
        changed = true;
    }
    gap(ui);
    changed |= super::combat_rewards::show(ui, *p, &mut s.combat, st, r);
    gap(ui);
    let c = &mut s.combat;
    let banner = (c.chips, c.banner_scale, c.banner_seconds, c.banner_top, c.banner_calm);
    let banner_default = (d.chips, d.banner_scale, d.banner_seconds, d.banner_top, d.banner_calm);
    if Card::new("eye-filled", tr!("combat-banner-title"))
        .modified(banner != banner_default)
        .show(ui, p, |ui| {
            changed |= switch_row(ui, p, &mut c.chips, tr!("combat-banner-chips"), tr!("combat-banner-chips-help"));
            changed |= slider_row(ui, tr!("combat-banner-scale"), "", &mut c.banner_scale, 0.5..=2.0, "");
            changed |= slider_row(ui, tr!("combat-banner-seconds"), "", &mut c.banner_seconds, 1.0..=8.0, " s");
            changed |= slider_row(ui, tr!("combat-banner-top"), "", &mut c.banner_top, 0.0..=400.0, "");
            changed |= switch_row(ui, p, &mut c.banner_calm, tr!("combat-banner-calm"), tr!("combat-banner-calm-help"));
        })
    {
        (c.chips, c.banner_scale, c.banner_seconds, c.banner_top, c.banner_calm) = banner_default;
        changed = true;
    }
    changed
}

fn minimap(ui: &mut egui::Ui, p: &Palette, s: &mut Settings, st: &mut OptionsUi, d: &Settings) -> bool {
    let mut changed = false;
    columns(
        ui,
        &mut (s, st, &mut changed),
        |ui, (s, _, changed)| {
            let off = !s.combat.enabled;
            let t = &mut s.maps.tactical;
            let dt = &d.maps.tactical;
            ui.add_enabled_ui(!off, |ui| {
                if Card::new("map", tr!("tactical-title"))
                    .modified(
                        t.tools_enabled != dt.tools_enabled || t.fixed_center != dt.fixed_center || t.rotate_avatar != dt.rotate_avatar,
                    )
                    .show(ui, p, |ui| {
                        **changed |= switch_row(ui, p, &mut t.tools_enabled, tr!("tactical-tools"), tr!("tactical-tools-help"));
                        let on = t.tools_enabled;
                        **changed |= under(ui, on, |ui| {
                            let a = switch_row(ui, p, &mut t.fixed_center, tr!("tactical-fixed-center"), "");
                            a | switch_row(ui, p, &mut t.rotate_avatar, tr!("tactical-rotate-avatar"), "")
                        });
                    })
                {
                    (t.tools_enabled, t.fixed_center, t.rotate_avatar) = (dt.tools_enabled, dt.fixed_center, dt.rotate_avatar);
                    **changed = true;
                }
            });
            gap(ui);
            let tools_on = !tools_off(s);
            let t = &mut s.maps.tactical;
            let dots = (t.dots_enabled, t.dot_scale, t.outline_width, t.outline);
            let dots_default = (dt.dots_enabled, dt.dot_scale, dt.outline_width, dt.outline);
            ui.add_enabled_ui(tools_on, |ui| {
                if Card::new("user", tr!("tactical-dots-title"))
                    .modified(dots != dots_default)
                    .show(ui, p, |ui| {
                        **changed |= switch_row(
                            ui,
                            p,
                            &mut t.dots_enabled,
                            tr!("tactical-dots-enabled"),
                            tr!("tactical-dots-enabled-help"),
                        );
                        let on = t.dots_enabled;
                        **changed |= under(ui, on, |ui| {
                            let mut any = slider_row(ui, tr!("tactical-dots"), "", &mut t.dot_scale, 0.5..=3.0, "×");
                            any |= slider_row(ui, tr!("tactical-outline-width"), "", &mut t.outline_width, 0.0..=4.0, " px");
                            any | color_row(ui, p, "tactical-outline", tr!("tactical-outline"), &mut t.outline, true)
                        });
                    })
                {
                    (t.dots_enabled, t.dot_scale, t.outline_width, t.outline) = dots_default;
                    **changed = true;
                }
            });
        },
        |ui, (s, st, changed)| {
            let off = tools_off(s) || !s.maps.tactical.dots_enabled;
            let colours = !s.maps.tactical.player_colors.is_empty() || !s.maps.tactical.group_colors.is_empty();
            let badge_text = s.maps.tactical.player_colors.len() + s.maps.tactical.group_colors.len();
            let card = Card::new("palette-filled", tr!("tactical-colors-title"))
                .help(tr!("tactical-colors-note"))
                .modified(colours);
            let card = if badge_text > 0 { card.badge(badge_text.to_string()) } else { card };
            ui.add_enabled_ui(!off, |ui| {
                if card.show(ui, p, |ui| {
                    **changed |= crate::combat::tactical::avatar_colors(ui, p, &st.tactical_context, &mut s.maps.tactical);
                }) {
                    s.maps.tactical.player_colors.clear();
                    s.maps.tactical.group_colors.clear();
                    **changed = true;
                }
            });
        },
    );
    changed
}

fn marker_sets(ui: &mut egui::Ui, p: &Palette, s: &mut Settings, st: &mut OptionsUi) -> bool {
    let mut changed = false;
    let off = tools_off(s);
    let d = Tactical::default();
    columns(
        ui,
        &mut (s, st, &mut changed),
        |ui, (s, _, changed)| {
            Card::new("map-pin-filled", tr!("combat-page-sets")).show(ui, p, |ui| {
                crate::combat::tactical::sets_panel(ui, p, &mut s.maps.tactical);
            });
            gap(ui);
            let t = &mut s.maps.tactical;
            let modified = (t.enabled, t.labels, t.size) != (d.enabled, d.labels, d.size);
            ui.add_enabled_ui(!off, |ui| {
                if Card::new("eye-filled", tr!("tactical-on-map"))
                    .modified(modified)
                    .show(ui, p, |ui| {
                        **changed |= crate::combat::tactical::marker_display(ui, p, t);
                    })
                {
                    (t.enabled, t.labels, t.size) = (d.enabled, d.labels, d.size);
                    **changed = true;
                }
            });
        },
        |ui, (s, st, changed)| {
            let count = s.maps.tactical.markers.len();
            let card = Card::new("map-pin-filled", tr!("tactical-markers-list"));
            let card = if count > 0 { card.badge(count.to_string()) } else { card };
            card.show(ui, p, |ui| {
                if count == 0 {
                    muted(ui, p, tr!("tactical-markers-empty"));
                }
                let before = s.maps.tactical.markers.clone();
                crate::combat::tactical::marker_list(ui, p, &st.tactical_context, &mut s.maps.tactical);
                **changed |= before != s.maps.tactical.markers;
            });
        },
    );
    changed
}

fn pings(ui: &mut egui::Ui, p: &Palette, s: &mut Settings, st: &mut OptionsUi, d: &Settings) -> bool {
    let mut changed = false;
    columns(
        ui,
        &mut (s, st, &mut changed),
        |ui, (s, _, changed)| {
            let off = tools_off(s);
            let t = &mut s.maps.tactical;
            let dt = &d.maps.tactical;
            ui.add_enabled_ui(!off, |ui| {
                let placing = (t.pings_enabled, t.key.clone(), t.hold_ms, t.seconds);
                if Card::new("flag-filled", tr!("ping-group-placing"))
                    .help(tr!("ping-gesture-help"))
                    .modified(placing != (dt.pings_enabled, dt.key.clone(), dt.hold_ms, dt.seconds))
                    .show(ui, p, |ui| {
                        **changed |= switch_row(ui, p, &mut t.pings_enabled, tr!("tactical-pings"), "");
                        let on = t.pings_enabled;
                        **changed |= under(ui, on, |ui| {
                            field_row(ui, tr!("ping-key"), tr!("tactical-key"), |ui| {
                                let label = if t.capture_key {
                                    tr!("ping-key-press").to_owned()
                                } else {
                                    crate::keybinds::Input::Key(t.key.clone()).label()
                                };
                                if ui.button(label).clicked() {
                                    t.capture_key = true;
                                }
                                false
                            });
                            let mut any = slider_row(
                                ui,
                                tr!("tactical-hold"),
                                tr!("tactical-hold-help"),
                                &mut t.hold_ms,
                                150.0..=800.0,
                                " ms",
                            );
                            any |= slider_row(
                                ui,
                                tr!("ping-duration"),
                                tr!("ping-duration-help"),
                                &mut t.seconds,
                                1.0..=60.0,
                                " s",
                            );
                            any
                        });
                    })
                {
                    (t.pings_enabled, t.key, t.hold_ms, t.seconds) = (dt.pings_enabled, dt.key.clone(), dt.hold_ms, dt.seconds);
                    **changed = true;
                }
            });
            gap(ui);
            let off = tools_off(s) || !s.maps.tactical.pings_enabled;
            let t = &mut s.maps.tactical;
            ui.add_enabled_ui(!off, |ui| {
                let aiming = (t.auto_contact, t.double_tap, t.aim_fade);
                if Card::new("crosshair", tr!("ping-group-aiming"))
                    .modified(aiming != (dt.auto_contact, dt.double_tap, dt.aim_fade))
                    .show(ui, p, |ui| {
                        **changed |= switch_row(ui, p, &mut t.auto_contact, tr!("ping-auto-contact"), "");
                        **changed |= switch_row(ui, p, &mut t.double_tap, tr!("ping-double-tap"), "");
                        **changed |= switch_row(ui, p, &mut t.aim_fade, tr!("ping-aim-fade"), "");
                    })
                {
                    (t.auto_contact, t.double_tap, t.aim_fade) = (dt.auto_contact, dt.double_tap, dt.aim_fade);
                    **changed = true;
                }
                gap(ui);
                if Card::new("eye-filled", tr!("ping-group-screen"))
                    .modified((t.offscreen_pings, t.distances) != (dt.offscreen_pings, dt.distances))
                    .show(ui, p, |ui| {
                        **changed |= switch_row(
                            ui,
                            p,
                            &mut t.offscreen_pings,
                            tr!("tactical-offscreen-pings"),
                            tr!("tactical-offscreen-pings-help"),
                        );
                        **changed |= switch_row(ui, p, &mut t.distances, tr!("tactical-distances"), "");
                    })
                {
                    (t.offscreen_pings, t.distances) = (dt.offscreen_pings, dt.distances);
                    **changed = true;
                }
            });
        },
        |ui, (s, st, changed)| {
            let off = tools_off(s) || !s.maps.tactical.pings_enabled;
            let t = &mut s.maps.tactical;
            let dt = &d.maps.tactical;
            ui.add_enabled_ui(!off, |ui| {
                if Card::new("palette-filled", tr!("ping-group-kinds"))
                    .modified(t.colors != dt.colors || t.icons != dt.icons)
                    .show(ui, p, |ui| {
                        for kind in 0..4 {
                            field_row(ui, crate::combat::tactical::kind_name(kind), "", |ui| {
                                crate::combat::tactical::kind_style(ui, p, t, kind)
                            });
                        }
                    })
                {
                    (t.colors, t.icons) = (dt.colors, dt.icons.clone());
                    **changed = true;
                }
                gap(ui);
                let sound = (t.ping_sound, t.sound_asset.clone(), t.sound_volume);
                if Card::new("volume", tr!("ping-group-sound"))
                    .modified(sound != (dt.ping_sound, dt.sound_asset.clone(), dt.sound_volume))
                    .show(ui, p, |ui| {
                        **changed |= switch_row(ui, p, &mut t.ping_sound, tr!("ping-sound"), "");
                        let on = t.ping_sound;
                        **changed |= under(ui, on, |ui| {
                            let any = text_row(ui, tr!("combat-sound-uuid"), "", &mut t.sound_asset);
                            any | slider_row(ui, tr!("combat-volume"), "", &mut t.sound_volume, 0.0..=1.0, "")
                        });
                    })
                {
                    (t.ping_sound, t.sound_asset, t.sound_volume) = (dt.ping_sound, dt.sound_asset.clone(), dt.sound_volume);
                    **changed = true;
                }
            });
            gap(ui);
            let remaining = t.last_ping.map(|at| (3.0 - at.elapsed().as_secs_f32()).max(0.0)).unwrap_or(0.0);
            let active = t.pings.len();
            let card = Card::new("history", tr!("ping-group-status"));
            let card = if active > 0 { card.badge(active.to_string()) } else { card };
            card.show(ui, p, |ui| {
                let text = if remaining > 0.0 {
                    tr!("ping-cooldown-left", seconds = format!("{remaining:.1}"))
                } else {
                    tr!("tactical-cooldown").to_owned()
                };
                muted(ui, p, &text);
                crate::combat::ping_ui::active(ui, p, t, &st.tactical_context.identities);
            });
        },
    );
    changed
}
