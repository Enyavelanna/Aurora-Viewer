//! The kill feed: which kills are listed, how a row is marked, where the
//! feed sits and the made-up kills of its test mode.
use super::{Event, Outcome, RecordedEvent, Settings, awards::Award};
use crate::theme::Palette;
use aurora_i18n::tr;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, vec2};
use std::{collections::BTreeSet, sync::Arc};
use uuid::Uuid;

pub(crate) const STYLE_COMPACT: u8 = 0;
pub(crate) const STYLE_CARDS: u8 = 1;
pub(crate) const FILTER_ALL: u8 = 0;
pub(crate) const FILTER_MINE: u8 = 1;
pub(crate) const FILTER_ALLIES: u8 = 2;

const FADE_IN: f32 = 0.18;
const FADE_OUT: f32 = 0.6;
/// How close, as a share of the screen, a dragged feed has to come to an edge to be pulled to it.
const SNAP: f32 = 0.04;
/// The gap kept between a snapped feed and the edge of the window, in points.
const MARGIN: f32 = 12.0;
const SLIDE: f32 = 14.0;
/// A row of the test mode never ages out.
const FOREVER: f32 = 1.0e9;

/// One row of the feed, with every name already resolved.
pub(crate) struct Entry {
    /// `None` when the environment killed.
    pub killer: Option<String>,
    pub victim: String,
    pub outcome: Outcome,
    pub killer_me: bool,
    pub victim_me: bool,
    pub killer_ally: bool,
    pub victim_ally: bool,
    /// An ally (or you) killed an ally (or you).
    pub friendly_fire: bool,
    pub damage_type: Option<i32>,
    pub weapon: Option<String>,
    pub awards: Vec<Award>,
    pub age: f32,
}

/// Whether the people of a kill are on your side: you and your allies.
fn friend(id: &Uuid, me: Uuid, allies: &BTreeSet<Uuid>) -> bool {
    *id == me || allies.contains(id)
}

fn involves(e: &Event, me: Uuid) -> bool {
    e.actor == me || e.target == me
}

/// Whether the filters let this kill into the feed.
pub(crate) fn keep(cfg: &Settings, e: &Event, me: Uuid, allies: &BTreeSet<Uuid>) -> bool {
    if cfg.feed_always_mine && involves(e, me) {
        return true;
    }
    if cfg.feed_avatars_only && e.target_is_avatar == Some(false) {
        return false;
    }
    match cfg.feed_filter {
        FILTER_MINE => involves(e, me),
        FILTER_ALLIES => involves(e, me) || allies.contains(&e.actor) || allies.contains(&e.target),
        _ => true,
    }
}

/// The name of a damage type (the LSL `DAMAGE_TYPE_*` constants); custom ones have none.
pub(crate) fn damage_name(t: i32) -> Option<&'static str> {
    Some(match t {
        -1 => tr!("damage-impact"),
        0 => tr!("damage-generic"),
        1 => tr!("damage-acid"),
        2 => tr!("damage-bludgeoning"),
        3 => tr!("damage-cold"),
        4 => tr!("damage-electric"),
        5 => tr!("damage-fire"),
        6 => tr!("damage-force"),
        7 => tr!("damage-necrotic"),
        8 => tr!("damage-piercing"),
        9 => tr!("damage-poison"),
        10 => tr!("damage-psychic"),
        11 => tr!("damage-radiant"),
        12 => tr!("damage-slashing"),
        13 => tr!("damage-sonic"),
        14 => tr!("damage-emotional"),
        _ => return None,
    })
}

fn damage_text(cfg: &Settings, t: i32) -> String {
    match damage_name(t) {
        Some(name) if cfg.feed_damage => name.to_owned(),
        _ => format!("#{t}"),
    }
}

/// A recorded kill as a row.
pub(crate) fn entry(
    row: &RecordedEvent,
    now: std::time::Instant,
    world: &crate::world::World,
    medals: bool,
    allies: &BTreeSet<Uuid>,
) -> Entry {
    let e = &row.event;
    let me = world.agent_id;
    let outcome = if e.actor == e.target {
        Outcome::Suicide
    } else if e.actor.is_nil() {
        Outcome::Environment
    } else {
        e.outcome(me)
    };
    let killer = (outcome != Outcome::Environment && outcome != Outcome::Suicide).then(|| world.social.display_name_of(&e.actor));
    let friendly_fire = matches!(outcome, Outcome::Kill | Outcome::Death | Outcome::Observed)
        && friend(&e.actor, me, allies)
        && friend(&e.target, me, allies);
    Entry {
        killer,
        victim: world.social.display_name_of(&e.target),
        outcome,
        killer_me: e.actor == me,
        victim_me: e.target == me,
        killer_ally: allies.contains(&e.actor),
        victim_ally: allies.contains(&e.target),
        friendly_fire,
        damage_type: e.damage_type,
        weapon: e.weapon.clone(),
        awards: if medals { row.awards.clone() } else { Vec::new() },
        age: now.saturating_duration_since(row.at).as_secs_f32(),
    }
}

/// The made-up kills of the test mode: one of every kind of row.
pub(crate) fn sample() -> Vec<Entry> {
    let plain = |killer: Option<&str>, victim: &str, outcome| Entry {
        killer: killer.map(str::to_owned),
        victim: victim.to_owned(),
        outcome,
        killer_me: false,
        victim_me: false,
        killer_ally: false,
        victim_ally: false,
        friendly_fire: false,
        damage_type: None,
        weapon: None,
        awards: Vec::new(),
        age: 1.0,
    };
    vec![
        Entry {
            killer_me: true,
            damage_type: Some(5),
            weapon: Some("Aurora Carbine".to_owned()),
            awards: vec![Award::Double, Award::Revenge],
            ..plain(Some("You"), "Kira Moon", Outcome::Kill)
        },
        Entry {
            victim_me: true,
            damage_type: Some(8),
            weapon: Some("Hunter Rifle".to_owned()),
            ..plain(Some("Aster Vale"), "You", Outcome::Death)
        },
        Entry {
            killer_ally: true,
            damage_type: Some(12),
            ..plain(Some("Orion Example"), "Jun Harker", Outcome::Observed)
        },
        Entry {
            killer_ally: true,
            victim_ally: true,
            friendly_fire: true,
            damage_type: Some(0),
            ..plain(Some("Orion Example"), "Violet Wolf", Outcome::Observed)
        },
        plain(Some("Mira Stone"), "Dax Holm", Outcome::Observed),
        plain(None, "Lio Park", Outcome::Environment),
        plain(None, "Sana Reed", Outcome::Suicide),
    ]
}

/// Opacity and slide (1 = just arrived) of a row of this age.
fn fade(age: f32, life: f32) -> (f32, f32) {
    let arrive = (age / FADE_IN).clamp(0.0, 1.0);
    let leave = ((life - age) / FADE_OUT).clamp(0.0, 1.0);
    (arrive.min(leave), 1.0 - arrive * (2.0 - arrive))
}

/// Pulls a dragged feed (`pos` as shares of the window, its top corner) to the edges and the
/// middle of the window when it comes close. `room` is the window, `feed` the size of the feed.
pub(crate) fn snap(mut pos: [f32; 2], room: egui::Vec2, feed: egui::Vec2) -> [f32; 2] {
    let (mx, my) = (MARGIN / room.x.max(1.0), MARGIN / room.y.max(1.0));
    let bottom = (1.0 - (feed.y + MARGIN) / room.y.max(1.0)).max(my);
    let middle = (0.5 - feed.y / 2.0 / room.y.max(1.0)).clamp(my, bottom);
    for target in [mx, 1.0 - mx] {
        if (pos[0] - target).abs() < SNAP {
            pos[0] = target;
        }
    }
    for target in [my, middle, bottom] {
        if (pos[1] - target).abs() < SNAP {
            pos[1] = target;
        }
    }
    pos
}

/// What the feed asks of the settings.
#[derive(Default)]
pub(crate) struct Edit {
    pub position: Option<[f32; 2]>,
    /// The "done" button of the test mode was pressed.
    pub done: bool,
}

/// A row, laid out: its pieces with their offsets and the size it takes.
struct Laid {
    size: egui::Vec2,
    texts: Vec<(egui::Pos2, Arc<egui::Galley>)>,
    chips: Vec<(Rect, Arc<egui::Galley>, Color32)>,
    /// Tabler icons: where their centre is, the icon, its colour and its size.
    glyphs: Vec<(egui::Pos2, &'static str, Color32, f32)>,
}

struct Look {
    accent: Color32,
    back: Color32,
    killer: Color32,
    victim: Color32,
    dim: Color32,
}

fn look(p: &Palette, e: &Entry, ally: Color32) -> Look {
    let (accent, back) = if e.friendly_fire {
        (p.amber, p.amber.gamma_multiply(0.22))
    } else if e.killer_me {
        (p.violet, p.violet.gamma_multiply(0.3))
    } else if e.victim_me {
        (p.danger, p.danger.gamma_multiply(0.26))
    } else {
        (p.muted_dim, p.panel.gamma_multiply(0.88))
    };
    let mine = e.killer_me || e.victim_me;
    let base = if mine { p.ink } else { p.muted };
    let who = |me: bool, is_ally: bool| {
        if is_ally {
            ally
        } else if me {
            p.violet_light
        } else {
            base
        }
    };
    Look {
        accent,
        back,
        killer: who(e.killer_me, e.killer_ally),
        victim: who(e.victim_me, e.victim_ally),
        dim: if mine { p.muted } else { p.muted_dim },
    }
}

fn lay(ui: &egui::Ui, p: &Palette, cfg: &Settings, e: &Entry, look: &Look) -> Laid {
    let size = cfg.feed_size.clamp(9.0, 32.0);
    let cards = cfg.feed_style == STYLE_CARDS;
    let font = |s: f32| FontId::proportional(s);
    let galley = |text: String, s: f32, color: Color32| ui.painter().layout_no_wrap(text, font(s), color);
    let (pad_x, pad_y, bar) = if cards { (12.0, 8.0, 4.0) } else { (9.0, 4.0, 3.0) };
    let mut texts = Vec::new();
    let mut glyphs: Vec<(egui::Pos2, &'static str, Color32, f32)> = Vec::new();
    let mut x = bar + pad_x;
    let head_size = if cards { size * 1.1 } else { size };
    let head = |text: String, color: Color32, texts: &mut Vec<(egui::Pos2, Arc<egui::Galley>)>, x: &mut f32| {
        let g = galley(text, head_size, color);
        texts.push((egui::pos2(*x, pad_y), g.clone()));
        *x += g.size().x;
    };
    if let Some(killer) = &e.killer {
        head(killer.clone(), look.killer, &mut texts, &mut x);
        let arrow = head_size * 0.9;
        glyphs.push((
            egui::pos2(x + 8.0 + arrow / 2.0, pad_y + head_size * 0.625),
            "arrow-right",
            look.dim,
            arrow,
        ));
        x += arrow + 16.0;
        head(e.victim.clone(), look.victim, &mut texts, &mut x);
    } else {
        head(e.victim.clone(), look.victim, &mut texts, &mut x);
        let why = if e.outcome == Outcome::Suicide {
            tr!("combat-suicide")
        } else {
            tr!("combat-environment")
        };
        head(format!("  ·  {why}"), look.dim, &mut texts, &mut x);
    }
    let detail: Vec<String> = e
        .damage_type
        .map(|t| damage_text(cfg, t))
        .into_iter()
        .chain(e.weapon.clone())
        .collect();
    let detail = detail.join("  ·  ");
    let small = size * 0.82;
    let chip_font = size * 0.78;
    let chip_pad = vec2(6.0, 2.0);
    let mut chips = Vec::new();
    let height;
    let mut width;
    if cards {
        // Line one is the kill, line two its damage, weapon and medals.
        let head_h = head_size * 1.25;
        let mut x2 = bar + pad_x;
        if !detail.is_empty() {
            let g = galley(detail, small, look.dim);
            texts.push((egui::pos2(x2, pad_y + head_h + 2.0), g.clone()));
            x2 += g.size().x + 10.0;
        }
        let y2 = pad_y + head_h + 2.0;
        for award in &e.awards {
            let g = galley(cfg.medal_name(*award).to_owned(), chip_font, p.ink);
            let r = Rect::from_min_size(egui::pos2(x2, y2), g.size() + chip_pad * 2.0);
            x2 = r.right() + 5.0;
            chips.push((r, g, crate::ui::combat_rewards::accent(p, *award)));
        }
        width = x.max(x2) + pad_x;
        height = pad_y * 2.0 + head_h + 2.0 + small * 1.25 + if e.awards.is_empty() { 0.0 } else { chip_pad.y * 2.0 - 2.0 };
    } else {
        let h = head_size * 1.25;
        if !detail.is_empty() {
            let g = galley(format!("  {detail}"), small, look.dim);
            texts.push((egui::pos2(x, pad_y + (h - g.size().y) / 2.0), g.clone()));
            x += g.size().x;
        }
        let mut cx = x + 8.0;
        for award in &e.awards {
            let g = galley(cfg.medal_name(*award).to_owned(), chip_font, p.ink);
            let r = Rect::from_min_size(
                egui::pos2(cx, pad_y + (h - g.size().y - chip_pad.y * 2.0) / 2.0),
                g.size() + chip_pad * 2.0,
            );
            cx = r.right() + 5.0;
            chips.push((r, g, crate::ui::combat_rewards::accent(p, *award)));
        }
        width = if e.awards.is_empty() { x } else { cx - 5.0 } + pad_x;
        height = h + pad_y * 2.0;
    }
    if e.friendly_fire {
        let g = galley(tr!("combat-feed-friendly").to_owned(), small, p.amber);
        let (icon, mid) = (small * 1.1, pad_y + head_size * 0.625);
        let left = width - pad_x + 10.0;
        glyphs.push((egui::pos2(left + icon / 2.0, mid), "alert-triangle-filled", p.amber, icon));
        texts.push((egui::pos2(left + icon + 4.0, mid - g.size().y / 2.0), g.clone()));
        width += 10.0 + icon + 4.0 + g.size().x;
    }
    Laid {
        size: vec2(width, height),
        texts,
        chips,
        glyphs,
    }
}

/// Draws the feed and returns what the person changed by dragging it. `rect` is the window,
/// `life` how long a kill stays, `ally` the colour of an ally's name.
pub(crate) fn show(ctx: &egui::Context, p: &Palette, cfg: &Settings, entries: &[Entry], ally: Color32, rect: Rect) -> Edit {
    let mut edit = Edit::default();
    if entries.is_empty() {
        return edit;
    }
    let test = cfg.feed_test;
    let life = if test { FOREVER } else { cfg.feed_seconds.clamp(1.0, 30.0) };
    let position = egui::pos2(
        rect.left() + rect.width() * cfg.feed_position[0].clamp(0.0, 1.0),
        rect.top() + rect.height() * cfg.feed_position[1].clamp(0.0, 1.0),
    );
    let right = cfg.feed_position[0] > 0.5;
    let id = egui::Id::unique("combat-killfeed");
    let response = egui::Area::new(id)
        .fixed_pos(position)
        .pivot(if right { Align2::RIGHT_TOP } else { Align2::LEFT_TOP })
        .order(egui::Order::Foreground)
        .interactable(test)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = if cfg.feed_style == STYLE_CARDS { 6.0 } else { 3.0 };
            let radius = ui.visuals().widgets.inactive.corner_radius;
            let rows: Vec<(Laid, Look, f32, f32)> = entries
                .iter()
                .map(|e| {
                    let look = look(p, e, ally);
                    let laid = lay(ui, p, cfg, e, &look);
                    let (alpha, slide) = fade(e.age, life);
                    (laid, look, alpha, slide)
                })
                .collect();
            let width = rows.iter().map(|r| r.0.size.x).fold(0.0, f32::max);
            let top = ui.cursor().min;
            for (laid, look, alpha, slide) in rows {
                let (slot, _) = ui.allocate_exact_size(vec2(width, laid.size.y), Sense::hover());
                let slid = if right { slide * SLIDE } else { -slide * SLIDE };
                let at = if right { slot.right() - laid.size.x } else { slot.left() } + slid;
                let row = Rect::from_min_size(egui::pos2(at, slot.top()), laid.size);
                let mut painter = ui.painter().clone().with_clip_rect(ui.ctx().content_rect());
                // The whole row fades through the painter: a galley keeps the colours it was
                // laid out with, so a faded colour passed to `galley` (its fallback, used for
                // `Color32::PLACEHOLDER` only) would leave the text opaque.
                painter.set_opacity(alpha * cfg.feed_opacity.clamp(0.0, 1.0));
                // a dense base so the names read over a bright scene, the role's tint over it
                painter.rect_filled(row, radius, p.panel.gamma_multiply(0.88));
                painter.rect_filled(row, radius, look.back);
                let bar = Rect::from_min_size(row.min, vec2(if cfg.feed_style == STYLE_CARDS { 4.0 } else { 3.0 }, row.height()));
                painter.rect_filled(bar, radius, look.accent);
                for (offset, galley) in laid.texts {
                    painter.galley(row.min + offset.to_vec2(), galley, p.ink);
                }
                for (at, icon, color, size) in laid.glyphs {
                    crate::ui::widgets::paint_glyph(&painter, icon, row.min + at.to_vec2(), size, color);
                }
                for (r, galley, accent) in laid.chips {
                    let r = r.translate(row.min.to_vec2());
                    painter.rect_filled(r, radius, p.raised.gamma_multiply(0.9));
                    painter.rect_stroke(r, radius, Stroke::new(1.0, accent), StrokeKind::Inside);
                    painter.galley(r.min + vec2(6.0, 2.0), galley, p.ink);
                }
            }
            let used = Rect::from_min_max(top, ui.min_rect().max);
            if test {
                let drag = ui.interact(used, id.with("drag"), Sense::drag());
                ui.painter()
                    .rect_stroke(used.expand(5.0), radius, Stroke::new(1.5, p.violet_light), StrokeKind::Outside);
                if drag.hovered() || drag.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                let pointer = ui.ctx().pointer_interact_pos();
                let grab_id = id.with("grab");
                if drag.drag_started()
                    && let Some(pointer) = pointer
                {
                    ui.data_mut(|d| d.insert_temp(grab_id, pointer - position));
                }
                if drag.dragged()
                    && let Some(pointer) = pointer
                {
                    let grab: egui::Vec2 = ui.data(|d| d.get_temp(grab_id)).unwrap_or_default();
                    let anchor = pointer - grab;
                    let mut pos = [
                        ((anchor.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0),
                        ((anchor.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0),
                    ];
                    if cfg.feed_snap {
                        pos = snap(pos, rect.size(), used.size());
                    }
                    edit.position = Some(pos);
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(tr!("combat-feed-test-hint")).color(p.violet_light));
                if crate::ui::widgets::action_button(
                    ui,
                    p,
                    "check",
                    tr!("combat-feed-test-done"),
                    crate::ui::widgets::ActionKind::Primary,
                    vec2(0.0, 28.0),
                )
                .clicked()
                {
                    edit.done = true;
                }
            }
        });
    drop(response);
    if test || entries.iter().any(|e| e.age < life) {
        ctx.request_repaint();
    }
    edit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(actor: u128, target: u128) -> Event {
        Event {
            actor: Uuid::from_u128(actor),
            target: Uuid::from_u128(target),
            kind: super::super::Kind::Death,
            damage: None,
            damage_type: None,
            weapon: None,
            source: None,
            rezzer: None,
            target_is_avatar: Some(true),
        }
    }

    #[test]
    fn the_filters_list_what_they_say_and_your_own_kills_stay() {
        let me = Uuid::from_u128(1);
        let allies: BTreeSet<Uuid> = [Uuid::from_u128(2)].into();
        let mut cfg = Settings::default();
        assert!(keep(&cfg, &event(8, 9), me, &allies));
        cfg.feed_filter = FILTER_MINE;
        assert!(!keep(&cfg, &event(2, 9), me, &allies));
        assert!(keep(&cfg, &event(1, 9), me, &allies));
        cfg.feed_filter = FILTER_ALLIES;
        assert!(keep(&cfg, &event(8, 2), me, &allies));
        assert!(!keep(&cfg, &event(8, 9), me, &allies));
        cfg.feed_filter = FILTER_MINE;
        cfg.feed_always_mine = false;
        cfg.feed_avatars_only = true;
        let mut object = event(1, 9);
        object.target_is_avatar = Some(false);
        assert!(!keep(&cfg, &object, me, &allies));
        cfg.feed_always_mine = true;
        assert!(keep(&cfg, &object, me, &allies));
    }

    #[test]
    fn damage_types_have_names_until_they_are_custom() {
        assert!(damage_name(5).is_some() && damage_name(-1).is_some() && damage_name(14).is_some());
        assert!(damage_name(100).is_none());
    }

    #[test]
    fn a_dragged_feed_is_pulled_to_the_edges_and_left_alone_elsewhere() {
        let room = vec2(1600.0, 900.0);
        let feed = vec2(300.0, 120.0);
        let near_right = snap([0.985, 0.4], room, feed);
        assert!((near_right[0] - (1.0 - MARGIN / 1600.0)).abs() < 1e-6);
        assert_eq!(snap([0.3, 0.2], room, feed), [0.3, 0.2]);
        let near_bottom = snap([0.3, 0.84], room, feed);
        assert!((near_bottom[1] - (1.0 - (120.0 + MARGIN) / 900.0)).abs() < 1e-6);
    }

    #[test]
    fn a_row_fades_in_and_out() {
        assert_eq!(fade(0.0, 6.0).0, 0.0);
        assert_eq!(fade(1.0, 6.0), (1.0, 0.0));
        assert!(fade(5.8, 6.0).0 < 0.5);
    }

    #[test]
    fn the_test_mode_has_a_row_of_every_kind() {
        let rows = sample();
        assert!(rows.iter().any(|r| r.killer_me) && rows.iter().any(|r| r.victim_me));
        assert!(rows.iter().any(|r| r.friendly_fire) && rows.iter().any(|r| r.outcome == Outcome::Suicide));
    }
}
