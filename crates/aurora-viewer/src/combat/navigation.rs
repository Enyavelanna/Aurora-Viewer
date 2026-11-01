//! Camera-relative guidance for explicit pings, never inferred contacts.
//! Layout is independent of simulation latency and does not send any packets.
use super::tactical::{Ping, Settings, paint_icon};
use crate::{
    ui::{hud::Projector, minimap::text_shadow},
    world::World,
};
use egui::{Align2, Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use glam::Vec3;
use std::time::Instant;

const SECTORS: usize = 8;
const MAX_GUIDES: usize = 4;

#[derive(Debug, Clone, Copy)]
enum Location {
    World(Pos2),
    Edge { sector: usize, direction: Vec2 },
}

/// Clip coordinates retain their lateral signs behind the camera. Dividing by
/// negative w would reverse left/right. Rear targets occupy the lower rim;
/// an exactly rear-centred target has a stable down indicator instead of NaN.
fn locate(projector: &Projector, position: Vec3, safe: Rect) -> Option<Location> {
    if !position.is_finite() || !safe.is_positive() || !projector.ppp.is_finite() || projector.ppp <= 0.0 {
        return None;
    }
    let clip = projector.view_proj * position.extend(1.0);
    if !clip.is_finite() {
        return None;
    }
    if clip.w > 0.05 {
        let point = pos2(
            (clip.x / clip.w * 0.5 + 0.5) * projector.width / projector.ppp,
            (0.5 - clip.y / clip.w * 0.5) * projector.height / projector.ppp,
        );
        if safe.contains(point) {
            return Some(Location::World(point));
        }
    }
    let mut direction = vec2(clip.x * projector.width, -clip.y * projector.height);
    if clip.w <= 0.05 {
        direction.y = direction.y.abs() + clip.w.abs() * projector.height;
    }
    if !direction.is_finite() {
        return None;
    }
    direction = if direction.length_sq() > 1e-8 {
        direction.normalized()
    } else {
        Vec2::Y
    };
    let angle = direction.y.atan2(direction.x);
    let sector = ((angle / std::f32::consts::FRAC_PI_4).round() as i32).rem_euclid(SECTORS as i32) as usize;
    Some(Location::Edge { sector, direction })
}

struct Guide<'a> {
    ping: &'a Ping,
    sector: usize,
    direction: Vec2,
    count: usize,
    distance: f32,
}

fn priority(kind: usize) -> u8 {
    match kind {
        3 => 3,
        1 => 2,
        2 => 1,
        _ => 0,
    }
}

struct WorldGuide<'a> {
    ping: &'a Ping,
    at: Pos2,
    distance: f32,
    count: usize,
}
fn insert_world<'a>(groups: &mut Vec<WorldGuide<'a>>, ping: &'a Ping, at: Pos2, distance: f32) {
    if let Some(group) = groups
        .iter_mut()
        .find(|g| g.ping.sender == ping.sender && (g.at.x - at.x).abs() < 96.0 && (g.at.y - at.y).abs() < 36.0)
    {
        group.count += 1;
        if (priority(ping.kind), ping.at) >= (priority(group.ping.kind), group.ping.at) {
            group.ping = ping;
            group.at = at;
            group.distance = distance;
        }
    } else {
        groups.push(WorldGuide {
            ping,
            at,
            distance,
            count: 1,
        });
    }
}

fn insert<'a>(groups: &mut [Vec<Guide<'a>>; SECTORS], ping: &'a Ping, sector: usize, direction: Vec2, distance: f32) {
    if let Some(group) = groups[sector].iter_mut().find(|g| g.ping.sender == ping.sender) {
        group.count += 1;
        if (priority(ping.kind), ping.at) >= (priority(group.ping.kind), group.ping.at) {
            group.ping = ping;
            group.direction = direction;
            group.distance = distance;
        }
    } else {
        groups[sector].push(Guide {
            ping,
            sector,
            direction,
            count: 1,
            distance,
        });
    }
}

fn guides(groups: [Vec<Guide<'_>>; SECTORS]) -> Vec<Guide<'_>> {
    let mut visible: Vec<_> = groups.into_iter().flatten().collect();
    visible.sort_by(|a, b| {
        priority(b.ping.kind)
            .cmp(&priority(a.ping.kind))
            .then(b.ping.at.cmp(&a.ping.at))
            .then(a.sector.cmp(&b.sector))
    });
    visible.truncate(MAX_GUIDES);
    visible
}

pub(super) fn show(ctx: &egui::Context, projector: &Projector, world: &World, cfg: &Settings, mouselook: bool) {
    let area = crate::ui::layout::work_area(ctx);
    // Leave room for the glyph, arrow and label inside the work area, including
    // the top/bottom Aurora bars. Never reserve or intercept pointer input.
    if area.width() < 240.0 || area.height() < 180.0 {
        return;
    }
    let safe = area.shrink(56.0);
    let aim = (mouselook && cfg.aim_fade).then(|| ctx.content_rect().center());
    // Like native world name/hover labels, stay behind Aurora floaters.
    let painter = ctx.layer_painter(egui::LayerId::background());
    let (ox, oy) = world.main_origin().unwrap_or_default();
    let now = Instant::now();
    let mut groups: [Vec<Guide<'_>>; SECTORS] = std::array::from_fn(|_| Vec::new());
    let mut world_groups = Vec::new();
    for ping in &cfg.pings {
        let age = now.saturating_duration_since(ping.at).as_secs_f32();
        if age >= cfg.seconds.clamp(1.0, 60.0) {
            continue;
        }
        let marker = &ping.marker;
        let position = Vec3::new(
            (marker.position[0] - ox as f64) as f32,
            (marker.position[1] - oy as f64) as f32,
            marker.position[2] as f32,
        );
        let distance = position.distance(world.agent.position);
        if !distance.is_finite() {
            continue;
        }
        let color = Color32::from_rgba_unmultiplied(marker.color[0], marker.color[1], marker.color[2], marker.color[3]);
        // With guidance off retain the original projection and HUD behaviour.
        if !cfg.offscreen_pings {
            if let Some((at, _)) = projector.project(position) {
                paint_world(&painter, at, ping, distance, color, cfg, world, 1, aim, &[]);
            }
            continue;
        }
        match locate(projector, position, safe) {
            Some(Location::World(at)) => insert_world(&mut world_groups, ping, at, distance),
            Some(Location::Edge { sector, direction }) => insert(&mut groups, ping, sector, direction, distance),
            None => {}
        }
    }
    let mut occupied = Vec::new();
    for group in world_groups {
        let c = group.ping.marker.color;
        occupied.push(paint_world(
            &painter,
            group.at,
            group.ping,
            group.distance,
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
            cfg,
            world,
            group.count,
            aim,
            &occupied,
        ));
    }
    let mut slots = [0_usize; SECTORS];
    for guide in guides(groups) {
        let angle = guide.sector as f32 * std::f32::consts::FRAC_PI_4;
        let slot = vec2(angle.cos(), angle.sin());
        let offset = slots[guide.sector] as f32 * 44.0;
        slots[guide.sector] += 1;
        let at =
            safe.clamp(safe.center() + vec2(slot.x * safe.width() * 0.5, slot.y * safe.height() * 0.5) + vec2(-slot.y, slot.x) * offset);
        let marker = &guide.ping.marker;
        let color = Color32::from_rgba_unmultiplied(marker.color[0], marker.color[1], marker.color[2], marker.color[3]);
        paint_icon(&painter, at, cfg.size.clamp(6.0, 12.0), &cfg.icons[guide.ping.kind.min(3)], color);
        let tip = at + guide.direction * 24.0;
        let tangent = vec2(-guide.direction.y, guide.direction.x);
        for sign in [-1.0, 1.0] {
            painter.line_segment([tip - guide.direction * 7.0 + tangent * sign * 5.0, tip], Stroke::new(2.0, color));
        }
        let mut label = super::ping_ui::label(guide.ping, world, cfg, guide.distance);
        if guide.count > 1 {
            label.push_str(&format!("  +{}", guide.count - 1));
        }
        let (offset, align) = if slot.x > 0.5 {
            (vec2(-20.0, 0.0), Align2::RIGHT_CENTER)
        } else if slot.x < -0.5 {
            (vec2(20.0, 0.0), Align2::LEFT_CENTER)
        } else if slot.y > 0.5 {
            (vec2(0.0, -20.0), Align2::CENTER_BOTTOM)
        } else {
            (vec2(0.0, 20.0), Align2::CENTER_TOP)
        };
        let size = painter
            .layout_no_wrap(label.clone(), egui::FontId::proportional(12.0), color)
            .size();
        if let Some(position) = label_position(area, at + offset, align, size, &occupied) {
            occupied.push(text_shadow(&painter, position, align, &label, 12.0, color));
        }
    }
}

fn label_position(area: Rect, desired: Pos2, align: Align2, size: Vec2, occupied: &[Rect]) -> Option<Pos2> {
    [0.0, 20.0, -20.0, 40.0, -40.0]
        .into_iter()
        .map(|dy| desired + vec2(0.0, dy))
        .find(|position| {
            let rect = align.anchor_size(*position, size);
            area.contains_rect(rect) && !occupied.iter().any(|taken| taken.expand(4.0).intersects(rect))
        })
}

#[allow(clippy::too_many_arguments)]
fn paint_world(
    painter: &egui::Painter,
    at: Pos2,
    ping: &Ping,
    distance: f32,
    color: Color32,
    cfg: &Settings,
    world: &World,
    count: usize,
    aim: Option<Pos2>,
    occupied: &[Rect],
) -> Rect {
    let mut painter = painter.clone();
    painter.set_opacity(aim.map_or(1.0, |aim| aim_opacity(at.distance(aim))));
    paint_icon(&painter, at, cfg.size, &cfg.icons[ping.kind.min(3)], color);
    let mut label = super::ping_ui::label(ping, world, cfg, distance);
    if count > 1 {
        label.push_str(&format!("  +{}", count - 1));
    }
    let desired = at + vec2(0.0, cfg.size + 4.0);
    let size = painter
        .layout_no_wrap(label.clone(), egui::FontId::proportional(13.0), color)
        .size();
    let area = crate::ui::layout::work_area(painter.ctx());
    if let Some(position) = label_position(area, desired, Align2::CENTER_TOP, size, occupied) {
        text_shadow(&painter, position, Align2::CENTER_TOP, &label, 13.0, color)
    } else {
        Rect::from_center_size(at, Vec2::splat(cfg.size * 2.0))
    }
}

/// Smoothstep avoids an opacity jump while aiming. The 35% floor keeps the
/// ping recognisable and targetable even directly under the crosshair.
fn aim_opacity(distance: f32) -> f32 {
    if !distance.is_finite() {
        return 1.0;
    }
    let t = (distance / 120.0).clamp(0.0, 1.0);
    0.35 + 0.65 * t * t * (3.0 - 2.0 * t)
}

/// Reticle/cursor targeting is independent of names and does not raycast the
/// simulator. Only a live, on-screen icon within 22 logical pixels is eligible.
pub(super) fn aimed(ctx: &egui::Context, projector: &Projector, world: &World, cfg: &Settings, mouselook: bool) -> Option<uuid::Uuid> {
    let aim = if mouselook {
        ctx.content_rect().center()
    } else {
        ctx.input(|i| i.pointer.hover_pos())?
    };
    if !mouselook && crate::ui::widgets::pointer_over_ui(ctx, aim) {
        return None;
    }
    let (ox, oy) = world.main_origin().unwrap_or_default();
    let mut nearest = None;
    let mut best = 22.0_f32;
    let mut displayed = Vec::new();
    for ping in &cfg.pings {
        if ping.at.elapsed().as_secs_f32() >= cfg.seconds.clamp(1.0, 60.0) {
            continue;
        }
        let position = Vec3::new(
            (ping.marker.position[0] - ox as f64) as f32,
            (ping.marker.position[1] - oy as f64) as f32,
            ping.marker.position[2] as f32,
        );
        if let Some((at, _)) = projector.project(position) {
            if cfg.offscreen_pings {
                insert_world(&mut displayed, ping, at, 0.0);
            } else {
                displayed.push(WorldGuide {
                    ping,
                    at,
                    distance: 0.0,
                    count: 1,
                });
            }
        }
    }
    for group in displayed {
        let ping = group.ping;
        if !ping.own() && (!ping.compatible || !cfg.share || !cfg.send_enabled) {
            continue;
        }
        let distance = group.at.distance(aim);
        if distance < best {
            nearest = Some(ping.marker.id);
            best = distance;
        }
    }
    nearest
}

#[cfg(test)]
mod tests {
    use super::*;
    fn projector() -> Projector {
        Projector {
            view_proj: glam::camera::rh::proj::directx::perspective_infinite_reverse(1.0, 2.0, 0.1),
            width: 1200.0,
            height: 600.0,
            ppp: 1.0,
        }
    }
    #[test]
    fn mouselook_ping_fade_is_smooth_bounded_and_never_hides_the_target() {
        assert_eq!(aim_opacity(0.0), 0.35);
        assert_eq!(aim_opacity(120.0), 1.0);
        assert_eq!(aim_opacity(1000.0), 1.0);
        assert_eq!(aim_opacity(f32::NAN), 1.0);
        let mut previous = 0.35;
        for distance in 0..=120 {
            let opacity = aim_opacity(distance as f32);
            assert!((0.35..=1.0).contains(&opacity));
            assert!(opacity >= previous);
            previous = opacity;
        }
    }

    #[test]
    fn acknowledged_ping_stays_targeted_so_another_tap_cannot_place_through_it() {
        let world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        let mut cfg = Settings {
            pings_enabled: true,
            share: true,
            ..Default::default()
        };
        let mut ping = Ping::new(
            super::super::tactical::Marker {
                id: uuid::Uuid::new_v4(),
                position: [0.0, 0.0, -20.0],
                ..Default::default()
            },
            1,
            Instant::now(),
        );
        ping.sender = uuid::Uuid::from_u128(1);
        ping.acknowledged = true;
        let id = ping.marker.id;
        cfg.pings.push(ping);
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 600.0))),
                ..Default::default()
            },
            |ui| {
                assert_eq!(aimed(ui.ctx(), &projector(), &world, &cfg, true), Some(id));
                assert!(!cfg.respond(id, Instant::now()));
                assert!(cfg.last_ping.is_none());
                assert_eq!(cfg.pings.len(), 1);
            },
        );
    }
    #[test]
    fn combat_navigation_handles_front_sides_rear_and_dpi_without_reversed_bearings() {
        let mut p = projector();
        let safe = Rect::from_min_max(pos2(56.0, 56.0), pos2(1144.0, 544.0));
        assert!(matches!(locate(&p, Vec3::new(0.0, 0.0, -20.0), safe), Some(Location::World(_))));
        for depth in [-1.0, 1.0] {
            let Some(Location::Edge { direction, .. }) = locate(&p, Vec3::new(20.0, 0.0, depth), safe) else {
                panic!("edge");
            };
            assert!(direction.x > 0.0);
        }
        let Some(Location::Edge { sector, direction }) = locate(&p, Vec3::new(0.0, 0.0, 20.0), safe) else {
            panic!("rear");
        };
        assert_eq!(sector, 2);
        assert!(direction.y > 0.99);
        p.ppp = 2.0;
        let small = Rect::from_min_max(pos2(56.0, 56.0), pos2(544.0, 244.0));
        let Some(Location::World(point)) = locate(&p, Vec3::new(0.0, 0.0, -20.0), small) else {
            panic!("scaled projection");
        };
        assert_eq!(point, pos2(300.0, 150.0));
        assert!(locate(&p, Vec3::NAN, safe).is_none());
        p.ppp = 0.0;
        assert!(locate(&p, Vec3::ZERO, safe).is_none());
    }
    #[test]
    fn combat_navigation_groups_same_direction_and_bounds_the_display_without_losing_pings() {
        let now = Instant::now();
        let pings: Vec<_> = (0..64)
            .map(|i| Ping::new(Default::default(), i % 4, now + std::time::Duration::from_millis(i as u64)))
            .collect();
        let mut groups = std::array::from_fn(|_| Vec::new());
        for (index, ping) in pings.iter().enumerate() {
            insert(&mut groups, ping, index % 8, Vec2::X, 20.0);
        }
        assert_eq!(groups.iter().flatten().map(|g| g.count).sum::<usize>(), 64);
        let visible = guides(groups);
        assert_eq!(visible.len(), 4);
        assert_eq!(visible[0].ping.at, pings[63].at);
        assert_eq!(pings.len(), 64);
    }

    #[test]
    fn ping_groups_preserve_distinct_senders_at_the_same_bearing_and_position() {
        let now = Instant::now();
        let mut alex = Ping::new(Default::default(), 1, now);
        let mut mika = Ping::new(Default::default(), 1, now);
        alex.sender = uuid::Uuid::from_u128(1);
        mika.sender = uuid::Uuid::from_u128(2);
        let mut groups = std::array::from_fn(|_| Vec::new());
        insert(&mut groups, &alex, 0, Vec2::X, 20.0);
        insert(&mut groups, &mika, 0, Vec2::X, 20.0);
        assert_eq!(guides(groups).len(), 2);
        let mut screen = Vec::new();
        insert_world(&mut screen, &alex, pos2(600.0, 300.0), 20.0);
        insert_world(&mut screen, &mika, pos2(600.0, 300.0), 20.0);
        assert_eq!(screen.len(), 2);
        assert!(screen.iter().all(|g| g.count == 1));
    }

    #[test]
    fn combat_navigation_uses_camera_orientation_and_keeps_help_above_newer_default_pings() {
        let mut p = projector();
        let eye = Vec3::new(120.0, 80.0, 30.0);
        p.view_proj *= glam::camera::rh::view::look_at_mat4(eye, eye + Vec3::Y, Vec3::Z);
        let safe = Rect::from_min_max(pos2(56.0, 56.0), pos2(1144.0, 544.0));
        assert!(matches!(locate(&p, eye + Vec3::Y * 20.0, safe), Some(Location::World(_))));
        let Some(Location::Edge { direction, .. }) = locate(&p, eye + Vec3::X * 100.0, safe) else {
            panic!("right bearing");
        };
        assert!(direction.x > 0.0);
        let now = Instant::now();
        let help = Ping::new(Default::default(), 3, now);
        let ordinary = Ping::new(Default::default(), 0, now + std::time::Duration::from_secs(1));
        let mut groups = std::array::from_fn(|_| Vec::new());
        insert(&mut groups, &help, 0, Vec2::X, 40.0);
        insert(&mut groups, &ordinary, 0, Vec2::X, 30.0);
        let visible = guides(groups);
        assert_eq!(visible[0].ping.kind, 3);
        assert_eq!(visible[0].count, 2);
        let mut screen_groups = Vec::new();
        insert_world(&mut screen_groups, &ordinary, pos2(600.0, 250.0), 30.0);
        insert_world(&mut screen_groups, &help, pos2(610.0, 254.0), 40.0);
        assert_eq!(screen_groups.len(), 1);
        assert_eq!(screen_groups[0].ping.kind, 3);
        assert_eq!(screen_groups[0].count, 2);
        let cfg: Settings = serde_json::from_str(r#"{"pings_enabled":true}"#).expect("legacy settings");
        assert!(cfg.offscreen_pings);
        let area = Rect::from_min_max(Pos2::ZERO, pos2(1200.0, 800.0));
        let desired = pos2(1000.0, 400.0);
        let size = vec2(150.0, 16.0);
        let occupied = [Align2::RIGHT_CENTER.anchor_size(desired, size)];
        let placed = label_position(area, desired, Align2::RIGHT_CENTER, size, &occupied).expect("free label position");
        assert!(!Align2::RIGHT_CENTER.anchor_size(placed, size).intersects(occupied[0]));
        assert!(label_position(area, desired, Align2::RIGHT_CENTER, size, &[area]).is_none());
    }
}
