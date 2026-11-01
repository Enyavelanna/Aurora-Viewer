//! Shared preference rows for the dedicated Combat window.
use super::super::cx::{Cx, field, fields};
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
    cx.group(tr!("combat-title"), |cx| {
        cx.toggle(tr!("combat-journal"), "", &field!(combat.journal));
        cx.toggle(tr!("combat-show-tab"), "", &field!(combat.show_tab));
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
    let p = cx.p;
    let title = match tab {
        3 => tr!("combat-page-minimap"),
        4 => tr!("combat-page-sets"),
        5 => tr!("combat-page-pings"),
        _ => tr!("combat-page-sharing"),
    };
    cx.group(title, |cx| {
        cx.keywords(
            &[
                tr!("tactical-markers"),
                tr!("tactical-pings"),
                tr!("tactical-labels"),
                tr!("tactical-distances"),
                tr!("tactical-dots-enabled"),
                tr!("tactical-dots"),
                tr!("tactical-outline"),
                tr!("tactical-key"),
                tr!("tactical-hold"),
                tr!("tactical-sound"),
                tr!("tactical-sharing"),
                tr!("tactical-share-group"),
                tr!("tactical-player-color"),
                tr!("combat-size"),
                tr!("combat-volume"),
                tr!("combat-duration"),
            ]
            .join(" "),
        );
        cx.section_body().row_with(
            tr!("tactical-title"),
            "",
            &fields!(
                maps.tactical.fixed_center,
                maps.tactical.enabled,
                maps.tactical.pings_enabled,
                maps.tactical.offscreen_pings,
                maps.tactical.labels,
                maps.tactical.distances,
                maps.tactical.size,
                maps.tactical.dot_scale,
                maps.tactical.dots_enabled,
                maps.tactical.outline,
                maps.tactical.outline_width,
                maps.tactical.player_colors,
                maps.tactical.colors,
                maps.tactical.icons,
                maps.tactical.seconds,
                maps.tactical.key,
                maps.tactical.hold_ms,
                maps.tactical.share,
                maps.tactical.share_group,
                maps.tactical.group_colors,
                maps.tactical.recipients,
                maps.tactical.ping_sound,
                maps.tactical.sound_asset,
                maps.tactical.sound_volume
            ),
            |ui, s, st, _| {
                // Reset only preferences, preserving annotations and session pings.
                let before = s.maps.tactical.clone();
                ui.vertical(|ui| {
                    crate::combat::tactical::panel(ui, &p, &st.tactical_context, &mut s.maps.tactical, tab);
                });
                before != s.maps.tactical
            },
        );
    });
}
