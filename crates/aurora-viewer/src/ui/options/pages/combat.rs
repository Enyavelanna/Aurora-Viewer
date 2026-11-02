//! Shared preference rows for the dedicated Combat window.
use super::super::cx::{Custom, Cx, Nothing, field, fields};
use crate::settings::Settings;
use aurora_i18n::tr;

pub fn page(cx: &mut Cx, tab: usize) {
    match tab {
        0 => overview(cx),
        1 => feedback(cx),
        2 => rewards(cx),
        _ => tactical(cx, tab),
    }
}

fn overview(cx: &mut Cx) {
    cx.group(tr!("combat-title"), |cx| {
        cx.toggle(tr!("combat-journal"), "", &field!(combat.journal));
        cx.toggle(tr!("combat-show-tab"), "", &field!(combat.show_tab));
        cx.toggle(tr!("combat-feed"), "", &field!(combat.feed));
        cx.toggle(tr!("combat-killmarker"), "", &field!(combat.killmarker));
        cx.toggle(tr!("combat-kill-sound"), "", &field!(combat.kill_sound));
        cx.toggle(tr!("combat-voices"), "", &field!(combat.voices));
        cx.toggle(tr!("combat-medals"), "", &field!(combat.medals));
    });
    cx.group(tr!("tactical-title"), |cx| {
        cx.toggle(tr!("tactical-tools"), "", &field!(maps.tactical.tools_enabled));
        cx.toggle(tr!("tactical-markers"), "", &field!(maps.tactical.enabled));
        cx.toggle(tr!("tactical-pings"), "", &field!(maps.tactical.pings_enabled));
        cx.toggle(tr!("tactical-share"), "", &field!(maps.tactical.share));
    });
}

fn feedback(cx: &mut Cx) {
    let p = cx.p;
    cx.group(tr!("combat-journal-title"), |cx| {
        cx.toggle(tr!("combat-journal"), "", &field!(combat.journal));
        cx.toggle(tr!("combat-show-tab"), "", &field!(combat.show_tab));
    });
    cx.group(tr!("combat-zoom-sensitivity"), |cx| {
        cx.slider(
            tr!("combat-zoom-sensitivity"),
            tr!("combat-zoom-sensitivity-help"),
            &field!(combat.zoom_sensitivity),
            0.0..=1.0,
            "",
        );
    });
    cx.group(tr!("combat-killmarker"), |cx| {
        cx.toggle(tr!("combat-killmarker"), "", &field!(combat.killmarker));
        cx.subs(false, |cx| {
            cx.slider(tr!("combat-size"), "", &field!(combat.marker_size), 4.0..=80.0, "");
            cx.slider(tr!("combat-duration"), "", &field!(combat.marker_seconds), 0.05..=2.0, "");
            cx.slider(tr!("combat-opacity"), "", &field!(combat.marker_opacity), 0.0..=1.0, "");
            cx.row(tr!("common-color"), "", &field!(combat.marker_color), |ui, s| {
                crate::ui::widgets::color_button(ui, &p, "hitmarker-color", &mut s.combat.marker_color, true).changed()
            });
        });
    });
    cx.group(tr!("combat-kill-sound"), |cx| {
        cx.toggle(tr!("combat-kill-sound"), "", &field!(combat.kill_sound));
        cx.subs(false, |cx| {
            cx.row(tr!("combat-asset"), "", &field!(combat.kill_asset), |ui, s| {
                ui.text_edit_singleline(&mut s.combat.kill_asset).changed()
            });
            cx.slider(tr!("combat-volume"), "", &field!(combat.kill_volume), 0.0..=1.0, "");
        });
    });
    cx.group(tr!("combat-feed"), |cx| {
        cx.toggle(tr!("combat-feed"), "", &field!(combat.feed));
        cx.subs(false, |cx| {
            cx.slider(tr!("combat-size"), "", &field!(combat.feed_size), 9.0..=32.0, "");
            cx.slider(tr!("combat-opacity"), "", &field!(combat.feed_opacity), 0.1..=1.0, "");
            cx.slider(tr!("combat-duration"), "", &field!(combat.feed_seconds), 1.0..=30.0, "");
            cx.slider(tr!("combat-entries"), "", &field!(combat.feed_entries), 1..=20, "");
            cx.below().row(tr!("combat-position"), "", &field!(combat.feed_position), |ui, s| {
                ui.add(egui::Slider::new(&mut s.combat.feed_position[0], 0.0..=1.0).text(tr!("combat-horizontal")))
                    .changed()
                    | ui.add(egui::Slider::new(&mut s.combat.feed_position[1], 0.0..=1.0).text(tr!("combat-vertical")))
                        .changed()
            });
        });
    });
}

fn rewards(cx: &mut Cx) {
    let p = cx.p;
    cx.group(tr!("combat-rewards"), |cx| {
        cx.toggle(tr!("combat-voices"), "", &field!(combat.voices));
        cx.slider(tr!("combat-volume"), "", &field!(combat.voice_volume), 0.0..=1.0, "");
        cx.toggle(tr!("combat-medals"), "", &field!(combat.medals));
        cx.section_body().row_with(
            tr!("combat-reward-assets"),
            "",
            &fields!(combat.announcement_assets, combat.medal_textures),
            |ui, s, st, r| crate::ui::combat_rewards::show(ui, p, &mut s.combat, st, r),
        );
    });
}

fn tactical(cx: &mut Cx, tab: usize) {
    match tab {
        3 => minimap(cx),
        4 => sets(cx),
        5 => pings(cx),
        _ => sharing(cx),
    }
}

/// Whether the combat mini-map tools can work: Combat mode and their switch.
fn tools_off(cx: &Cx) -> bool {
    !cx.s.combat.enabled || !cx.s.maps.tactical.tools_enabled
}

fn minimap(cx: &mut Cx) {
    let p = cx.p;
    let combat_off = !cx.s.combat.enabled;
    cx.group(tr!("tactical-title"), |cx| {
        cx.greyed(combat_off);
        cx.toggle(
            tr!("tactical-tools"),
            tr!("tactical-tools-help"),
            &field!(maps.tactical.tools_enabled),
        );
        let off = tools_off(cx);
        cx.greyed(off);
        cx.toggle(tr!("tactical-fixed-center"), "", &field!(maps.tactical.fixed_center));
        cx.toggle(tr!("tactical-rotate-avatar"), "", &field!(maps.tactical.rotate_avatar));
    });
    cx.group(tr!("tactical-dots-title"), |cx| {
        let off = tools_off(cx);
        cx.greyed(off);
        cx.unfolded().toggle(
            tr!("tactical-dots-enabled"),
            tr!("tactical-dots-enabled-help"),
            &field!(maps.tactical.dots_enabled),
        );
        let dots_off = !cx.s.maps.tactical.dots_enabled;
        cx.subs(dots_off, |cx| {
            cx.slider(tr!("tactical-dots"), "", &field!(maps.tactical.dot_scale), 0.5..=3.0, "×");
            cx.slider(
                tr!("tactical-outline-width"),
                "",
                &field!(maps.tactical.outline_width),
                0.0..=4.0,
                " px",
            );
            cx.row(tr!("tactical-outline"), "", &field!(maps.tactical.outline), |ui, s| {
                crate::ui::widgets::color_button(ui, &p, "tactical-outline", &mut s.maps.tactical.outline, true).changed()
            });
        });
    });
    cx.group(tr!("tactical-colors-title"), |cx| {
        let off = tools_off(cx) || !cx.s.maps.tactical.dots_enabled;
        cx.note(tr!("tactical-colors-note"));
        cx.greyed(off);
        cx.section_body().row_with(
            tr!("tactical-colors-title"),
            "",
            &fields!(maps.tactical.player_colors, maps.tactical.group_colors),
            |ui, s, st, _| {
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    crate::combat::tactical::avatar_colors(ui, &p, &st.tactical_context, &mut s.maps.tactical)
                })
                .inner
            },
        );
    });
}

fn sets(cx: &mut Cx) {
    let p = cx.p;
    cx.group(tr!("combat-page-sets"), |cx| {
        cx.keywords(
            &[
                tr!("tactical-markers"),
                tr!("tactical-labels"),
                tr!("tactical-edit"),
                tr!("combat-size"),
            ]
            .join(" "),
        );
        cx.section_body().row_with(
            tr!("combat-page-sets"),
            "",
            &fields!(maps.tactical.enabled, maps.tactical.labels, maps.tactical.size),
            |ui, s, st, _| {
                // Reset only preferences, preserving annotations and session pings.
                let before = s.maps.tactical.clone();
                ui.vertical(|ui| {
                    crate::combat::tactical::panel(ui, &p, &st.tactical_context, &mut s.maps.tactical, 4);
                });
                before != s.maps.tactical
            },
        );
    });
}

fn pings(cx: &mut Cx) {
    let p = cx.p;
    cx.group(tr!("ping-group-placing"), |cx| {
        let off = tools_off(cx);
        cx.note(tr!("ping-gesture-help"));
        cx.greyed(off);
        cx.unfolded()
            .toggle(tr!("tactical-pings"), "", &field!(maps.tactical.pings_enabled));
        let pings_off = !cx.s.maps.tactical.pings_enabled;
        cx.subs(pings_off, |cx| {
            cx.row(tr!("ping-key"), tr!("tactical-key"), &field!(maps.tactical.key), |ui, s| {
                let cfg = &mut s.maps.tactical;
                let label = if cfg.capture_key {
                    tr!("ping-key-press").to_owned()
                } else {
                    crate::keybinds::Input::Key(cfg.key.clone()).label()
                };
                if ui.button(label).clicked() {
                    cfg.capture_key = true;
                }
                false
            });
            cx.slider(
                tr!("tactical-hold"),
                tr!("tactical-hold-help"),
                &field!(maps.tactical.hold_ms),
                150.0..=800.0,
                " ms",
            );
            cx.slider(
                tr!("ping-duration"),
                tr!("ping-duration-help"),
                &field!(maps.tactical.seconds),
                1.0..=60.0,
                " s",
            );
        });
    });
    let off = tools_off(cx) || !cx.s.maps.tactical.pings_enabled;
    cx.group(tr!("ping-group-aiming"), |cx| {
        cx.greyed(off);
        cx.toggle(tr!("ping-auto-contact"), "", &field!(maps.tactical.auto_contact));
        cx.toggle(tr!("ping-double-tap"), "", &field!(maps.tactical.double_tap));
        cx.toggle(tr!("ping-aim-fade"), "", &field!(maps.tactical.aim_fade));
    });
    cx.group(tr!("ping-group-screen"), |cx| {
        cx.greyed(off);
        cx.toggle(
            tr!("tactical-offscreen-pings"),
            tr!("tactical-offscreen-pings-help"),
            &field!(maps.tactical.offscreen_pings),
        );
        cx.toggle(tr!("tactical-distances"), "", &field!(maps.tactical.distances));
    });
    cx.group(tr!("ping-group-kinds"), |cx| {
        cx.greyed(off);
        for kind in [0, 1, 2, 3] {
            let track = Custom {
                path: "maps.tactical.colors",
                modified: move |s: &Settings, d: &Settings| {
                    s.maps.tactical.colors[kind] != d.maps.tactical.colors[kind]
                        || s.maps.tactical.icons[kind] != d.maps.tactical.icons[kind]
                },
                reset: move |s: &mut Settings, d: &Settings| {
                    s.maps.tactical.colors[kind] = d.maps.tactical.colors[kind];
                    s.maps.tactical.icons[kind] = d.maps.tactical.icons[kind].clone();
                },
            };
            cx.row(crate::combat::tactical::kind_name(kind), "", &track, |ui, s| {
                crate::combat::tactical::kind_style(ui, &p, &mut s.maps.tactical, kind)
            });
        }
    });
    cx.group(tr!("ping-group-sound"), |cx| {
        cx.greyed(off);
        cx.unfolded().toggle(tr!("ping-sound"), "", &field!(maps.tactical.ping_sound));
        let sound_off = !cx.s.maps.tactical.ping_sound;
        cx.subs(sound_off, |cx| {
            cx.row(tr!("combat-sound-uuid"), "", &field!(maps.tactical.sound_asset), |ui, s| {
                ui.text_edit_singleline(&mut s.maps.tactical.sound_asset).changed()
            });
            cx.slider(tr!("combat-volume"), "", &field!(maps.tactical.sound_volume), 0.0..=1.0, "");
        });
    });
    cx.group(tr!("ping-group-status"), |cx| {
        cx.section_body().row_with(tr!("ping-active"), "", &Nothing, |ui, s, st, _| {
            let cfg = &mut s.maps.tactical;
            let remaining = cfg.last_ping.map(|at| (3.0 - at.elapsed().as_secs_f32()).max(0.0)).unwrap_or(0.0);
            let cooldown = if remaining > 0.0 {
                aurora_i18n::tr!("ping-cooldown-left", seconds = format!("{remaining:.1}"))
            } else {
                tr!("tactical-cooldown").to_owned()
            };
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(cooldown).color(p.muted));
                crate::combat::ping_ui::active(ui, &p, cfg, &st.tactical_context.identities);
            });
            false
        });
    });
}

fn sharing(cx: &mut Cx) {
    let p = cx.p;
    cx.keywords(
        &[
            tr!("tactical-share"),
            tr!("sharing-sending"),
            tr!("sharing-receiving"),
            tr!("tactical-ping-sharing"),
            tr!("sharing-teammates"),
            tr!("tactical-share-group"),
            tr!("tactical-send"),
            tr!("sharing-inbox"),
        ]
        .join(" "),
    );
    cx.section_body().row_with(
        tr!("combat-page-sharing"),
        "",
        &fields!(
            maps.tactical.share,
            maps.tactical.send_enabled,
            maps.tactical.receive_enabled,
            maps.tactical.ping_audience,
            maps.tactical.share_group,
            maps.tactical.recipients
        ),
        |ui, s, st, _| {
            let available = s.combat.enabled && s.maps.tactical.tools_enabled;
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                crate::ui::combat_sharing::show(
                    ui,
                    &p,
                    &mut s.maps.tactical,
                    &st.tactical_context,
                    available,
                    &mut st.combat_sharing,
                )
            })
            .inner
        },
    );
}
