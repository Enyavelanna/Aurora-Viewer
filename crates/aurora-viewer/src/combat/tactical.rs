//! Persistent global map annotations and short-lived, explicitly shared pings.
use crate::{
    theme::Palette,
    ui::{
        minimap::{View, text_shadow},
        widgets,
    },
    world::World,
};
use aurora_i18n::tr;
use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Instant};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum Shape {
    #[default]
    Point,
    Circle,
    Square,
    Line,
    Rectangle,
    Freehand,
}
impl Shape {
    pub const ALL: [Self; 4] = [Self::Circle, Self::Rectangle, Self::Line, Self::Freehand];
    pub fn tool(self) -> Self {
        match self {
            Self::Point => Self::Circle,
            Self::Square => Self::Rectangle,
            other => other,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Point | Self::Circle => tr!("combat-editor-round"),
            Self::Square => tr!("combat-editor-rectangle"),
            Self::Line => tr!("tactical-line"),
            Self::Rectangle => tr!("combat-editor-rectangle"),
            Self::Freehand => tr!("combat-editor-freehand"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Marker {
    pub id: Uuid,
    pub name: String,
    pub position: [f64; 3],
    pub radius: f32,
    pub color: [u8; 4],
    pub selected: bool,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub end: Option<[f64; 3]>,
    #[serde(default)]
    pub path: Vec<[f64; 3]>,
    #[serde(default)]
    pub hatched: bool,
}

impl Marker {
    pub fn shape(&self) -> Shape {
        if self.shape == Shape::Point && self.radius > 0.0 {
            Shape::Circle
        } else {
            self.shape
        }
    }
    pub fn valid(&self) -> bool {
        let coordinate = |p: &[f64; 3]| p.iter().all(|n| n.is_finite() && n.abs() < 1e10);
        !self.id.is_nil()
            && coordinate(&self.position)
            && self.end.as_ref().is_none_or(coordinate)
            && self.radius.is_finite()
            && (0.0..=1024.0).contains(&self.radius)
            && self.name.len() <= 128
            && !self.name.chars().any(char::is_control)
            && self.path.len() <= 128
            && self.path.iter().all(coordinate)
            && (self.shape != Shape::Freehand || self.path.len() >= 2)
            && (!matches!(self.shape(), Shape::Square | Shape::Line | Shape::Rectangle)
                || self
                    .end
                    .is_some_and(|end| (end[0] - self.position[0]).abs() <= 1024.0 && (end[1] - self.position[1]).abs() <= 1024.0))
    }
    pub fn drawn(shape: Shape, position: [f64; 3], mut end: [f64; 3], color: [u8; 4]) -> Option<Self> {
        let dx = end[0] - position[0];
        let dy = end[1] - position[1];
        let size = dx.abs().max(dy.abs());
        if !size.is_finite() || size < 0.1 || size > 1024.0 {
            return None;
        }
        if shape == Shape::Square {
            end[0] = position[0] + if dx < 0.0 { -size } else { size };
            end[1] = position[1] + if dy < 0.0 { -size } else { size };
        }
        let m = Self {
            id: Uuid::new_v4(),
            name: shape.label().into(),
            position,
            radius: 0.0,
            color,
            selected: false,
            shape,
            end: Some(end),
            ..Self::default()
        };
        m.valid().then_some(m)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ping {
    pub marker: Marker,
    pub kind: usize,
    pub at: Instant,
    pub sender: Uuid,
    pub compatible: bool,
    pub responders: std::collections::BTreeSet<Uuid>,
    pub recipients: std::collections::BTreeSet<Uuid>,
    pub acknowledged: bool,
    pub sharing: super::pings::ShareState,
}

/// A resident the viewer knows the place of (Sharing, avatar colours).
pub(crate) struct Nearby {
    pub id: Uuid,
    pub name: String,
    /// Metres from us; None outside a region.
    pub distance: Option<f32>,
    /// Only the coarse location is known (no height).
    pub approximate: bool,
}

/// The active group, and its members once their list has arrived.
pub(crate) struct ActiveGroup {
    pub name: String,
    pub members: Option<std::collections::BTreeSet<Uuid>>,
}

/// Read-only world data for Preferences; no live world borrow crosses a row.
#[derive(Default)]
pub(crate) struct Context {
    pub position: Option<[f64; 3]>,
    pub groups: Vec<(Uuid, String)>,
    pub identities: BTreeMap<Uuid, String>,
    /// The residents around us, nearest first.
    pub nearby: Vec<Nearby>,
    pub active_group: Option<ActiveGroup>,
    /// Names of the teammates, coloured residents and senders not around.
    pub names: BTreeMap<Uuid, String>,
}
impl Context {
    pub fn from_world(world: &World, cfg: &Settings) -> Self {
        let (x, y) = crate::ui::minimap::to_global(world, world.agent.position);
        let me = [x, y, world.agent.position.z as f64];
        let avatars = crate::ui::minimap::avatars(world);
        let mut nearby: Vec<Nearby> = avatars
            .iter()
            .map(|(id, pos, known)| {
                let (gx, gy) = crate::ui::minimap::to_global(world, *pos);
                let flat = (gx - me[0]).hypot(gy - me[1]);
                let distance = if *known { flat.hypot(pos.z as f64 - me[2]) } else { flat };
                Nearby {
                    id: *id,
                    name: world.social.display_name_of(id),
                    distance: world.main_region.map(|_| distance as f32),
                    approximate: !known,
                }
            })
            .collect();
        nearby.sort_by(|a, b| a.distance.unwrap_or(f32::MAX).total_cmp(&b.distance.unwrap_or(f32::MAX)));
        let active = world.groups.active;
        let active_group = world
            .groups
            .groups
            .iter()
            .find(|g| g.id == active && !active.is_nil())
            .map(|g| ActiveGroup {
                name: g.name.clone(),
                members: world
                    .group_data
                    .get(&g.id)
                    .filter(|d| d.members_complete)
                    .map(|d| d.members.keys().copied().collect()),
            });
        let names = cfg
            .recipients
            .keys()
            .chain(cfg.player_colors.keys())
            .chain(cfg.inbox.senders.values())
            .map(|id| (*id, world.social.display_name_of(id)))
            .collect();
        Self {
            position: world.main_region.map(|_| me),
            groups: world.groups.groups.iter().map(|g| (g.id, g.name.clone())).collect(),
            identities: cfg
                .pings
                .iter()
                .flat_map(|p| std::iter::once(p.sender).chain(p.responders.iter().copied()))
                .map(|id| (id, world.social.name_of(&id)))
                .collect(),
            nearby,
            active_group,
            names,
        }
    }
    /// A resident's name, around us or not.
    pub fn name_of(&self, id: &Uuid) -> String {
        self.nearby
            .iter()
            .find(|n| n.id == *id)
            .map(|n| n.name.clone())
            .or_else(|| self.names.get(id).cloned())
            .unwrap_or_else(|| "…".into())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub library: super::sets::Library,
    pub current_region: Option<super::sets::RegionKey>,
    pub enabled: bool,
    pub tools_enabled: bool,
    #[serde(skip)]
    pub combat_enabled: bool,
    pub rotate_avatar: bool,
    pub pings_enabled: bool,
    pub offscreen_pings: bool,
    pub labels: bool,
    pub distances: bool,
    pub size: f32,
    pub dot_scale: f32,
    pub dots_enabled: bool,
    pub outline: [u8; 4],
    pub outline_width: f32,
    pub player_colors: BTreeMap<Uuid, [u8; 4]>,
    pub markers: Vec<Marker>,
    pub colors: [[u8; 4]; 4],
    pub icons: [String; 4],
    pub seconds: f32,
    pub key: String,
    #[serde(skip)]
    pub capture_key: bool,
    pub hold_ms: f32,
    pub auto_contact: bool,
    pub double_tap: bool,
    pub aim_fade: bool,
    pub share: bool,
    pub send_enabled: bool,
    pub receive_enabled: bool,
    pub ping_audience: u8,
    #[serde(skip)]
    pub set_outgoing: std::collections::VecDeque<String>,
    /// Parts queued since the queue was last empty (Sharing's progress).
    #[serde(skip)]
    pub set_total: usize,
    #[serde(skip)]
    pub last_set_send: Option<Instant>,
    #[serde(skip)]
    pub inbox: super::transfer::Inbox,
    pub share_group: bool,
    pub group_colors: BTreeMap<Uuid, [u8; 4]>,
    pub recipients: BTreeMap<Uuid, bool>,
    pub ping_sound: bool,
    pub sound_asset: String,
    pub sound_volume: f32,
    #[serde(skip)]
    pub pings: Vec<Ping>,
    #[serde(skip)]
    pub outgoing: Vec<(Marker, Option<usize>)>,
    #[serde(skip)]
    pub sound_pending: bool,
    pub fixed_center: bool,
    #[serde(skip)]
    pub draw: Option<Shape>,
    #[serde(skip)]
    pub editing: bool,
    #[serde(skip)]
    pub edit_drag: Option<(Marker, [f64; 2], bool)>,
    #[serde(skip)]
    pub drawing_start: Option<[f64; 3]>,
    #[serde(skip)]
    pub last_ping: Option<Instant>,
    #[serde(skip)]
    pub sender_pings: BTreeMap<Uuid, Instant>,
    #[serde(skip)]
    pub seen_messages: BTreeMap<Uuid, Instant>,
    #[serde(skip)]
    pub share_error: bool,
    #[serde(skip)]
    pub ping_controls: std::collections::VecDeque<super::pings::Outbound>,
    #[serde(skip)]
    pub cancelled_pings: BTreeMap<(Uuid, Uuid), Instant>,
    #[serde(skip)]
    pub aimed_ping: Option<Uuid>,
    #[serde(skip)]
    pub last_tap: Option<(Uuid, Instant)>,
    #[serde(skip)]
    pub pending_contacts: BTreeMap<(Uuid, Uuid), Instant>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            library: Default::default(),
            current_region: None,
            enabled: false,
            tools_enabled: true,
            combat_enabled: true,
            rotate_avatar: true,
            pings_enabled: false,
            offscreen_pings: true,
            labels: true,
            distances: true,
            size: 8.0,
            dot_scale: 1.0,
            dots_enabled: false,
            outline: [0, 0, 0, 230],
            outline_width: 1.5,
            player_colors: BTreeMap::new(),
            markers: Vec::new(),
            colors: [[90, 180, 255, 255], [255, 70, 70, 255], [255, 200, 60, 255], [80, 240, 160, 255]],
            icons: ["map-pin-filled", "user-filled", "map-pin-filled", "users-group"].map(str::to_owned),
            seconds: 8.0,
            key: "KeyB".into(),
            capture_key: false,
            hold_ms: 250.0,
            auto_contact: true,
            double_tap: true,
            aim_fade: true,
            share: false,
            send_enabled: true,
            receive_enabled: true,
            ping_audience: 0,
            set_outgoing: Default::default(),
            set_total: 0,
            last_set_send: None,
            inbox: Default::default(),
            share_group: false,
            group_colors: BTreeMap::new(),
            recipients: BTreeMap::new(),
            ping_sound: false,
            sound_asset: String::new(),
            sound_volume: 0.7,
            pings: Vec::new(),
            outgoing: Vec::new(),
            sound_pending: false,
            fixed_center: false,
            draw: None,
            editing: false,
            edit_drag: None,
            drawing_start: None,
            last_ping: None,
            sender_pings: BTreeMap::new(),
            seen_messages: BTreeMap::new(),
            share_error: false,
            ping_controls: Default::default(),
            cancelled_pings: Default::default(),
            aimed_ping: None,
            last_tap: None,
            pending_contacts: Default::default(),
        }
    }
}
pub(crate) fn kind_name(kind: usize) -> &'static str {
    match kind {
        1 => tr!("tactical-enemy"),
        2 => tr!("tactical-objective"),
        3 => tr!("tactical-help"),
        _ => tr!("tactical-ping"),
    }
}
pub(crate) fn color(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

fn icon_name(icon: &str) -> &'static str {
    match icon {
        "user-filled" => tr!("tactical-icon-person"),
        "users-group" => tr!("tactical-icon-team"),
        "alert-triangle-filled" => tr!("tactical-icon-danger"),
        "flag-filled" => tr!("tactical-icon-objective"),
        _ => tr!("tactical-icon-pin"),
    }
}

pub(crate) fn wheel_selection(ctx: &egui::Context, center: egui::Pos2) -> Option<usize> {
    wheel_choice(ctx.input(|i| i.pointer.hover_pos()).unwrap_or(center) - center)
}
fn wheel_choice(delta: egui::Vec2) -> Option<usize> {
    if !delta.is_finite() || delta.length() < 52.0 {
        return None;
    }
    Some(
        // f32 rem_euclid can round a tiny negative angle up to TAU at the
        // seam. Wrap the resulting index too, so it never escapes 0..4.
        (((delta.y.atan2(delta.x) + std::f32::consts::FRAC_PI_4).rem_euclid(std::f32::consts::TAU)) / std::f32::consts::FRAC_PI_2) as usize
            % 4,
    )
}
pub(crate) fn world_overlay(
    ctx: &egui::Context,
    projector: &crate::ui::hud::Projector,
    world: &World,
    cfg: &mut Settings,
    mouselook: bool,
) {
    cfg.aimed_ping = None;
    if !cfg.available() || !cfg.pings_enabled {
        return;
    }
    super::navigation::show(ctx, projector, world, cfg, mouselook);
    cfg.aimed_ping = super::navigation::aimed(ctx, projector, world, cfg, mouselook);
}
impl Settings {
    pub fn available(&self) -> bool {
        self.tools_enabled && self.combat_enabled
    }
    /// The active set's name in this region.
    pub fn active_set_name(&self) -> String {
        self.current_region
            .as_ref()
            .and_then(|key| self.library.regions.iter().find(|r| &r.key == key))
            .and_then(|r| r.selected())
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }
    /// Queues the active set's markers (`selected_only`: the selected ones)
    /// for the recipients; `share_error` when it cannot.
    pub fn send_markers(&mut self, selected_only: bool) {
        if !self.available() || !self.share || !self.send_enabled {
            return;
        }
        let markers = self.markers.iter().filter(|m| !selected_only || m.selected).cloned().collect();
        match super::transfer::encode(self.active_set_name(), markers) {
            Ok(chunks) if self.set_outgoing.len() + chunks.len() <= 512 => {
                if self.set_outgoing.is_empty() {
                    self.set_total = 0;
                }
                self.set_total += chunks.len();
                self.set_outgoing.extend(chunks);
                self.share_error = false;
            }
            _ => self.share_error = true,
        }
    }
    pub fn enter_region(&mut self, key: super::sets::RegionKey, name: &str) {
        if self.current_region.as_ref() == Some(&key) {
            return;
        }
        if let Some((original, _, _)) = self.edit_drag.take()
            && let Some(marker) = self.markers.iter_mut().find(|m| m.id == original.id)
        {
            *marker = original;
        }
        self.editing = false;
        if let Some(previous) = self.current_region.take() {
            self.library.region_mut(&previous, "").save(&self.markers);
        } else if !self.markers.is_empty() {
            self.library.unassigned.append(&mut self.markers);
        }
        let region = self.library.region_mut(&key, name);
        if region.sets.is_empty() {
            region.add(tr!("tactical-set-default").into(), &[]);
        }
        self.markers = region.selected().map(|s| s.markers.clone()).unwrap_or_default();
        self.current_region = Some(key);
        self.draw = None;
        self.drawing_start = None;
        self.pings.clear();
        self.ping_controls.clear();
        self.cancelled_pings.clear();
        self.aimed_ping = None;
        self.last_tap = None;
        self.pending_contacts.clear();
        self.outgoing.clear();
        self.set_outgoing.clear();
        self.inbox = Default::default();
    }

    pub fn save_set(&mut self) {
        if self.edit_drag.is_some() {
            return;
        }
        if let Some(key) = &self.current_region {
            let region = self.library.region_mut(key, "");
            if region.active.is_none() && !self.markers.is_empty() {
                region.add(tr!("tactical-set-default").into(), &self.markers);
            }
            region.save(&self.markers);
        }
    }

    pub fn add_marker(&mut self, position: [f64; 3]) {
        if self.markers.len() < 256 {
            self.markers.push(Marker {
                id: Uuid::new_v4(),
                name: tr!("tactical-objective").into(),
                position,
                radius: 0.0,
                color: self.colors[2],
                selected: false,
                shape: Shape::Point,
                end: None,
                ..Marker::default()
            });
        }
    }
    pub fn ping(&mut self, position: [f64; 3], kind: usize) {
        self.ping_at(position, kind, Instant::now());
    }
    pub fn ping_at(&mut self, position: [f64; 3], kind: usize, now: Instant) -> bool {
        if !self.available()
            || !self.pings_enabled
            || kind >= 4
            || !position.iter().all(|v| v.is_finite() && v.abs() < 1e10)
            || self
                .last_ping
                .is_some_and(|at| now.saturating_duration_since(at).as_secs_f32() < 3.0)
        {
            return false;
        }
        self.last_ping = Some(now);
        self.sound_pending = true;
        let marker = Marker {
            id: Uuid::new_v4(),
            name: kind_name(kind).into(),
            position,
            radius: 0.0,
            color: self.colors[kind],
            selected: false,
            shape: Shape::Point,
            end: None,
            ..Marker::default()
        };
        if self.share && self.send_enabled && self.ping_audience != 0 && self.outgoing.len() < 256 {
            self.outgoing.push((marker.clone(), Some(kind)));
        }
        if self.pings.len() >= 64 {
            self.pings.remove(0);
        }
        let mut ping = Ping::new(marker, kind, now);
        if self.outgoing.iter().any(|(m, _)| m.id == ping.marker.id) {
            ping.sharing = super::pings::ShareState::Queued;
        }
        self.pings.push(ping);
        true
    }
    pub fn accept_shared(&mut self, sender: Uuid, body: Shared, now: Instant) -> bool {
        if sender.is_nil()
            || body.version != 2
            || !body.marker.valid()
            || self.seen_messages.contains_key(&body.marker.id)
            || body.kind.is_some_and(|k| k >= 4)
        {
            return false;
        }
        if let Some(kind) = body.kind {
            if !self.pings_enabled
                || self
                    .sender_pings
                    .get(&sender)
                    .is_some_and(|at| now.saturating_duration_since(*at).as_secs_f32() < 3.0)
            {
                return false;
            }
            if self.sender_pings.len() >= 512 && !self.sender_pings.contains_key(&sender) {
                return false;
            }
            self.sender_pings.insert(sender, now);
            if self.pings.len() >= 64 {
                self.pings.remove(0);
            }
            let mut ping = Ping::new(body.marker.clone(), kind, now);
            ping.sender = sender;
            ping.compatible = false;
            self.pings.push(ping);
            self.sound_pending = true;
        } else {
            if !self.enabled || self.markers.len() >= 256 || self.markers.iter().any(|m| m.id == body.marker.id) {
                return false;
            }
            let mut marker = body.marker.clone();
            marker.selected = false;
            self.markers.push(marker);
        }
        self.seen_messages.insert(body.marker.id, now);
        true
    }
    pub fn prune(&mut self) {
        self.share_error |= self.inbox.expire(Instant::now());
        self.pings.retain(|p| p.at.elapsed().as_secs_f32() < self.seconds.clamp(1.0, 60.0));
        self.sender_pings.retain(|_, at| at.elapsed().as_secs() < 60);
        self.seen_messages.retain(|_, at| at.elapsed().as_secs() < 120);
        self.cancelled_pings.retain(|_, at| at.elapsed().as_secs() < 120);
        self.pending_contacts.retain(|_, at| at.elapsed().as_secs() < 120);
        self.ping_controls.retain(|p| p.at.elapsed().as_secs() < 60);
    }
    pub fn map(&self, painter: &egui::Painter, view: &View, world: &World) {
        if !self.available() {
            return;
        }
        if self.enabled {
            for marker in &self.markers {
                self.paint_map(painter, view, marker);
            }
        }
        if self.pings_enabled {
            for ping in &self.pings {
                let mut marker = ping.marker.clone();
                marker.name = format!("{} · {}", super::ping_ui::short_name(ping, world), kind_name(ping.kind));
                if ping.acknowledged || !ping.responders.is_empty() {
                    marker.name.push_str("  ✓");
                }
                self.paint_map(painter, view, &marker);
                paint_icon(
                    painter,
                    view.to_screen(ping.marker.position[0], ping.marker.position[1]),
                    self.size,
                    &self.icons[ping.kind.min(3)],
                    color(ping.marker.color),
                );
            }
        }
    }
    pub fn paint_map(&self, painter: &egui::Painter, view: &View, m: &Marker) {
        let at = view.to_screen(m.position[0], m.position[1]);
        let c = color(m.color);
        if m.hatched {
            let polygon = match m.shape() {
                Shape::Circle => (0..32)
                    .map(|n| {
                        let angle = n as f32 * std::f32::consts::TAU / 32.0;
                        at + egui::vec2(angle.cos(), angle.sin()) * m.radius * view.ppm
                    })
                    .collect(),
                Shape::Square | Shape::Rectangle => m
                    .end
                    .map(|end| {
                        vec![
                            at,
                            view.to_screen(end[0], m.position[1]),
                            view.to_screen(end[0], end[1]),
                            view.to_screen(m.position[0], end[1]),
                        ]
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            hatch(painter, &polygon, c);
        }
        match m.shape() {
            Shape::Circle => {
                painter.circle(at, m.radius.min(1024.0) * view.ppm, c.gamma_multiply(0.15), Stroke::new(1.5, c));
            }
            Shape::Square | Shape::Rectangle => {
                if let Some(end) = m.end {
                    let corners = [
                        at,
                        view.to_screen(end[0], m.position[1]),
                        view.to_screen(end[0], end[1]),
                        view.to_screen(m.position[0], end[1]),
                    ];
                    painter.add(egui::Shape::convex_polygon(
                        corners.to_vec(),
                        c.gamma_multiply(0.15),
                        Stroke::new(1.5, c),
                    ));
                }
            }
            Shape::Line => {
                if let Some(end) = m.end {
                    painter.line_segment([at, view.to_screen(end[0], end[1])], Stroke::new(2.0, c));
                }
            }
            Shape::Point => {}
            Shape::Freehand => {
                painter.add(egui::Shape::line(
                    m.path.iter().map(|p| view.to_screen(p[0], p[1])).collect(),
                    Stroke::new(2.0, c),
                ));
            }
        }
        painter.circle_stroke(at, self.size.clamp(3.0, 32.0), Stroke::new(2.0, c));
        if self.labels {
            text_shadow(
                painter,
                at + egui::vec2(0.0, self.size + 3.0),
                egui::Align2::CENTER_TOP,
                &m.name,
                11.0,
                c,
            );
        }
    }
}
/// Bounded diagonal hatch clipped to a convex zone, including rotated minimaps.
pub(crate) fn hatch(painter: &egui::Painter, polygon: &[egui::Pos2], color: Color32) {
    if polygon.len() < 3 {
        return;
    }
    let min = polygon.iter().map(|p| p.x + p.y).fold(f32::INFINITY, f32::min);
    let max = polygon.iter().map(|p| p.x + p.y).fold(f32::NEG_INFINITY, f32::max);
    let step = ((max - min) / 128.0).max(10.0);
    for n in 0..=((max - min) / step).ceil() as usize {
        let value = min + n as f32 * step;
        let mut points = Vec::new();
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            let da = a.x + a.y;
            let db = b.x + b.y;
            if (db - da).abs() > 0.001 {
                let t = (value - da) / (db - da);
                if (0.0..1.0).contains(&t) {
                    points.push(a + (b - a) * t);
                }
            }
        }
        points.sort_by(|a, b| a.x.total_cmp(&b.x));
        if let (Some(a), Some(b)) = (points.first(), points.last()) {
            painter.line_segment([*a, *b], Stroke::new(1.0, color.gamma_multiply(0.5)));
        }
    }
}

/// The colour and icon of one ping kind (Pings tab).
pub(crate) fn kind_style(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, kind: usize) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= widgets::color_button(ui, p, ("ping-kind", kind), &mut cfg.colors[kind], false).changed();
        egui::ComboBox::from_id_salt(("ping-icon", kind))
            .selected_text(icon_name(&cfg.icons[kind]))
            .show_ui(ui, |ui| {
                for icon in [
                    "map-pin-filled",
                    "user-filled",
                    "users-group",
                    "alert-triangle-filled",
                    "flag-filled",
                ] {
                    if crate::ui::icons::embedded(icon) {
                        ui.horizontal(|ui| {
                            if let Some(picture) = crate::ui::icons::global(icon) {
                                ui.image((picture.id(), egui::Vec2::splat(16.0)));
                            }
                            changed |= ui
                                .selectable_value(&mut cfg.icons[kind], icon.to_owned(), icon_name(icon))
                                .changed();
                        });
                    }
                }
            });
    });
    changed
}

/// One line of the avatar colours: its colour (or a button to give it
/// one), its name and detail, and a button that takes the colour away.
fn color_line(ui: &mut egui::Ui, p: &Palette, id: Uuid, name: &str, detail: &str, map: &mut BTreeMap<Uuid, [u8; 4]>, default: [u8; 4]) {
    ui.horizontal(|ui| {
        match map.get_mut(&id) {
            Some(c) => {
                widgets::color_button(ui, p, ("avatar-color", id), c, false);
            }
            None => {
                if widgets::flat_button(ui, p, tr!("tactical-color-set")).clicked() {
                    map.insert(id, default);
                }
            }
        }
        ui.label(name);
        if !detail.is_empty() {
            ui.label(egui::RichText::new(detail).size(11.5).color(p.muted));
        }
        if map.contains_key(&id) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, p, "x", tr!("tactical-color-remove"), true) {
                    map.remove(&id);
                }
            });
        }
    });
}

/// The avatar colours of the Mini-map tab: the groups, then the residents
/// with a colour (around or not), then the others around, nearest first.
pub(crate) fn avatar_colors(ui: &mut egui::Ui, p: &Palette, context: &Context, cfg: &mut Settings) -> bool {
    let before = (cfg.player_colors.clone(), cfg.group_colors.clone());
    let default = cfg.colors[0];
    ui.label(egui::RichText::new(tr!("tactical-colors-groups")).size(11.5).color(p.muted));
    if context.groups.is_empty() {
        ui.label(egui::RichText::new(tr!("tactical-colors-no-groups")).color(p.muted));
    }
    for (id, name) in &context.groups {
        color_line(ui, p, *id, name, "", &mut cfg.group_colors, default);
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tr!("tactical-colors-residents")).size(11.5).color(p.muted));
    let coloured: Vec<Uuid> = cfg.player_colors.keys().copied().collect();
    for id in &coloured {
        let around = context.nearby.iter().find(|n| n.id == *id);
        let detail = around.map_or_else(|| tr!("sharing-away").to_owned(), distance_text);
        color_line(ui, p, *id, &context.name_of(id), &detail, &mut cfg.player_colors, default);
    }
    for n in context.nearby.iter().filter(|n| !coloured.contains(&n.id)) {
        color_line(ui, p, n.id, &n.name, &distance_text(n), &mut cfg.player_colors, default);
    }
    if coloured.is_empty() && context.nearby.is_empty() {
        ui.label(egui::RichText::new(tr!("tactical-colors-no-residents")).color(p.muted));
    }
    before != (cfg.player_colors.clone(), cfg.group_colors.clone())
}

/// "24 m", or "~120 m" from a coarse location.
pub(crate) fn distance_text(n: &Nearby) -> String {
    match n.distance {
        Some(d) if n.approximate => format!("~{d:.0} m"),
        Some(d) => format!("{d:.0} m"),
        None => String::new(),
    }
}

/// The markers' look on the map and the drawing tools (Marker sets tab).
pub(crate) fn marker_display(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) -> bool {
    let before = (cfg.editing, cfg.enabled, cfg.labels, cfg.size);
    crate::ui::combat_cards::switch_row(ui, p, &mut cfg.enabled, tr!("tactical-markers"), "");
    crate::ui::combat_cards::switch_row(ui, p, &mut cfg.labels, tr!("tactical-labels"), "");
    crate::ui::combat_cards::switch_row(ui, p, &mut cfg.editing, tr!("tactical-edit"), tr!("tactical-edit-help"));
    crate::ui::combat_cards::slider_row(ui, tr!("combat-size"), "", &mut cfg.size, 3.0..=32.0, "");
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for shape in [Shape::Rectangle, Shape::Line] {
            if widgets::flat_button(ui, p, shape.label()).clicked() {
                cfg.enabled = true;
                cfg.draw = Some(shape);
                cfg.drawing_start = None;
            }
        }
    });
    before != (cfg.editing, cfg.enabled, cfg.labels, cfg.size)
}

/// Every marker of the active set, one framed block each (Marker sets tab).
pub(crate) fn marker_list(ui: &mut egui::Ui, p: &Palette, context: &Context, cfg: &mut Settings) {
    {
        if ui
            .add_enabled(context.position.is_some(), egui::Button::new(tr!("tactical-add")))
            .clicked()
            && let Some(position) = context.position
        {
            cfg.add_marker(position);
        }
        let mut remove = None;
        for (i, m) in cfg.markers.iter_mut().enumerate() {
            ui.add_space(6.0);
            ui.push_id(m.id, |ui| {
                widgets::surface(ui, p, |ui| {
                    ui.set_width(ui.available_width());
                    let logical = m.shape().tool();
                    let mut shape = logical;
                    egui::ComboBox::from_id_salt("shape")
                        .selected_text(shape.label())
                        .show_ui(ui, |ui| {
                            for value in Shape::ALL {
                                ui.selectable_value(&mut shape, value, value.label());
                            }
                        });
                    if shape != logical {
                        m.shape = shape;
                        m.path.clear();
                        if shape == Shape::Freehand {
                            m.path = vec![m.position, [m.position[0] + 8.0, m.position[1] + 8.0, m.position[2]]];
                        }
                        if matches!(shape, Shape::Point | Shape::Circle) {
                            m.radius = 0.0;
                            m.end = None;
                        } else {
                            m.radius = 0.0;
                            m.end = Some([m.position[0] + 8.0, m.position[1] + 8.0, m.position[2]]);
                        }
                    }
                    ui.horizontal(|ui| {
                        widgets::switch(ui, p, &mut m.selected);
                        ui.text_edit_singleline(&mut m.name);
                        if ui.button(tr!("tactical-remove")).clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.horizontal(|ui| {
                        widgets::color_button(ui, p, "color", &mut m.color, false);
                        if m.shape().tool() == Shape::Circle {
                            ui.add(egui::Slider::new(&mut m.radius, 0.0..=1024.0).text(tr!("combat-editor-radius")));
                        }
                        if m.shape() == Shape::Square
                            && let Some(end) = m.end
                        {
                            let dx = end[0] - m.position[0];
                            let dy = end[1] - m.position[1];
                            let mut side = dx.abs().max(dy.abs()).clamp(0.1, 1024.0);
                            if ui
                                .add(
                                    egui::DragValue::new(&mut side)
                                        .range(0.1..=1024.0)
                                        .prefix(tr!("tactical-square-side")),
                                )
                                .changed()
                            {
                                m.end = Some([
                                    m.position[0] + side * if dx < 0.0 { -1.0 } else { 1.0 },
                                    m.position[1] + side * if dy < 0.0 { -1.0 } else { 1.0 },
                                    end[2],
                                ]);
                            }
                        }
                    });
                    let previous_position = m.position;
                    ui.label(tr!("tactical-position"));
                    ui.horizontal(|ui| {
                        let width = ((ui.available_width() - ui.spacing().item_spacing.x * 2.0) / 3.0).max(48.0);
                        for coordinate in &mut m.position {
                            ui.add_sized([width, 24.0], egui::DragValue::new(coordinate).speed(0.1).max_decimals(2));
                        }
                    });
                    if previous_position != m.position
                        && let Some(end) = &mut m.end
                    {
                        for i in 0..3 {
                            end[i] += m.position[i] - previous_position[i];
                        }
                    }
                    if previous_position != m.position {
                        for point in &mut m.path {
                            for axis in 0..3 {
                                point[axis] += m.position[axis] - previous_position[axis];
                            }
                        }
                    }
                    if m.shape() != Shape::Square
                        && let Some(end) = &mut m.end
                    {
                        ui.label(tr!("tactical-end"));
                        ui.horizontal(|ui| {
                            for value in end {
                                ui.add(egui::DragValue::new(value).speed(0.1));
                            }
                        });
                    }
                })
            });
        }
        if let Some(i) = remove {
            cfg.markers.remove(i);
        }
        cfg.save_set();
    }
}

pub(crate) fn sets_panel(ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) {
    let Some(key) = cfg.current_region.clone() else {
        ui.label(tr!("tactical-set-region-required"));
        return;
    };
    cfg.save_set();
    let region = cfg.library.region_mut(&key, "");
    let previous = region.active;
    egui::ComboBox::from_id_salt("combat-set")
        .selected_text(region.selected().map(|s| s.name.as_str()).unwrap_or_default())
        .show_ui(ui, |ui| {
            for set in &region.sets {
                ui.selectable_value(&mut region.active, Some(set.id), &set.name);
            }
        });
    if let Some(set) = region.sets.iter_mut().find(|s| Some(s.id) == region.active) {
        ui.text_edit_singleline(&mut set.name);
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr!("tactical-set-new")).clicked() {
            region.add(tr!("tactical-set-default").into(), &[]);
        }
        if ui.button(tr!("tactical-set-duplicate")).clicked() {
            let name = region.selected().map(|s| s.name.clone()).unwrap_or_default();
            region.add(name, &cfg.markers);
        }
        if ui.button(tr!("tactical-remove")).clicked() {
            region.remove_active();
        }
        if ui.button(tr!("tactical-set-save")).clicked() {
            region.save(&cfg.markers);
        }
    });
    if region.active != previous {
        cfg.markers = region.selected().map(|s| s.markers.clone()).unwrap_or_default();
        cfg.draw = None;
        cfg.drawing_start = None;
    }
    if !cfg.library.unassigned.is_empty() {
        widgets::surface(ui, p, |ui| {
            ui.label(tr!("tactical-set-unassigned"));
            if ui.button(tr!("tactical-set-assign")).clicked() {
                let markers = std::mem::take(&mut cfg.library.unassigned);
                let region = cfg.library.region_mut(&key, "");
                region.add(tr!("tactical-set-unassigned").into(), &markers);
                cfg.markers = region.selected().map(|s| s.markers.clone()).unwrap_or_default();
            }
        });
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Shared {
    pub version: u8,
    pub marker: Marker,
    pub kind: Option<usize>,
}

pub(super) fn paint_icon(painter: &egui::Painter, at: egui::Pos2, size: f32, name: &str, c: Color32) {
    if let Some(icon) = crate::ui::icons::global(name) {
        painter.image(
            icon.id(),
            egui::Rect::from_center_size(at, egui::Vec2::splat(size * 2.0)),
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            c,
        );
    } else {
        painter.circle_stroke(at, size, Stroke::new(2.0, c));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_tactical_cooldown_is_shared_and_invalid_positions_do_not_consume_it() {
        let mut cfg = Settings {
            pings_enabled: true,
            offscreen_pings: true,
            ..Default::default()
        };
        let now = Instant::now();
        assert!(!cfg.ping_at([f64::NAN, 0.0, 0.0], 0, now));
        assert!(cfg.ping_at([1.0, 2.0, 3.0], 0, now));
        for kind in 0..4 {
            assert!(!cfg.ping_at([1.0, 2.0, 3.0], kind, now + std::time::Duration::from_millis(2999)));
        }
        assert!(cfg.ping_at([1.0, 2.0, 3.0], 3, now + std::time::Duration::from_secs(3)));
        assert_eq!(cfg.pings.len(), 2);
    }
    #[test]
    fn combat_tactical_peer_cooldowns_do_not_block_other_peers_or_markers() {
        let mut cfg = Settings {
            pings_enabled: true,
            enabled: true,
            ..Default::default()
        };
        let now = Instant::now();
        let make = |kind| Shared {
            version: 2,
            marker: Marker {
                id: Uuid::new_v4(),
                name: "objective".into(),
                position: [100.0, 200.0, 30.0],
                radius: 0.0,
                color: [255; 4],
                selected: false,
                shape: Shape::Point,
                end: None,
                ..Marker::default()
            },
            kind,
        };
        assert!(cfg.accept_shared(Uuid::from_u128(1), make(Some(0)), now));
        assert!(!cfg.accept_shared(Uuid::from_u128(1), make(Some(1)), now));
        assert!(cfg.accept_shared(Uuid::from_u128(2), make(Some(1)), now));
        assert!(cfg.accept_shared(Uuid::from_u128(1), make(None), now));
        assert_eq!((cfg.pings.len(), cfg.markers.len()), (2, 1));
    }
    #[test]
    fn combat_tactical_old_circles_and_new_shapes_round_trip_without_session_state() {
        let old = serde_json::json!({"id":Uuid::from_u128(1),"name":"old zone","position":[100.0,200.0,30.0],"radius":5.0,"color":[255,0,0,255],"selected":true});
        let marker: Marker = serde_json::from_value(old).expect("old marker");
        assert_eq!(marker.shape(), Shape::Circle);
        let mut cfg = Settings {
            pings_enabled: true,
            ..Default::default()
        };
        cfg.markers.push(marker);
        for shape in [Shape::Square, Shape::Line] {
            let m = Marker::drawn(shape, [100.0, 200.0, 30.0], [110.0, 204.0, 30.0], [255; 4]).expect("geometry");
            assert!(m.valid());
            let bytes = serde_json::to_vec(&Shared {
                version: 2,
                marker: m.clone(),
                kind: None,
            })
            .expect("wire");
            assert!(bytes.len() <= 480);
            cfg.markers.push(m);
        }
        cfg.ping([1.0, 2.0, 3.0], 0);
        cfg.draw = Some(Shape::Line);
        let reloaded: Settings = serde_json::from_str(&serde_json::to_string(&cfg).expect("settings")).expect("reload");
        assert_eq!(reloaded.markers, cfg.markers);
        assert!(reloaded.pings.is_empty() && reloaded.draw.is_none() && reloaded.last_ping.is_none());
        assert!(Marker::drawn(Shape::Square, [0.0; 3], [0.0; 3], [255; 4]).is_none());
    }

    #[test]
    fn wheel_cancels_at_its_origin_and_selects_the_four_directions() {
        assert_eq!(wheel_choice(egui::Vec2::ZERO), None);
        assert_eq!(wheel_choice(egui::vec2(23.0, 0.0)), None);
        // This exact near-diagonal can round rem_euclid to TAU, formerly
        // producing sector 4 and indexing past the four configured colours.
        assert!(wheel_choice(egui::vec2(100.0, -100.00001)).is_some_and(|kind| kind < 4));
        for step in 0..3600 {
            let angle = step as f32 * std::f32::consts::TAU / 3600.0;
            assert!(wheel_choice(egui::vec2(angle.cos(), angle.sin()) * 100.0).is_some_and(|kind| kind < 4));
        }
        for (direction, kind) in [((100.0, 0.0), 0), ((0.0, 100.0), 1), ((-100.0, 0.0), 2), ((0.0, -100.0), 3)] {
            assert_eq!(wheel_choice(egui::vec2(direction.0, direction.1)), Some(kind));
        }
        let ctx = egui::Context::default();
        let origin = egui::pos2(120.0, 260.0);
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(origin)],
                ..Default::default()
            },
            |ui| assert_eq!(wheel_selection(ui.ctx(), origin), None),
        );
    }
}
