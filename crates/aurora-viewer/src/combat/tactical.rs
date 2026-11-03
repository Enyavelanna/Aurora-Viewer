//! Persistent global map annotations and short-lived, explicitly shared pings.
use crate::{
    theme::Palette,
    ui::{minimap::View, widgets},
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
    /// A closed area of three or more corners (`path`).
    Polygon,
    /// A line from `position` to `end` with an arrowhead.
    Arrow,
    /// A path with an arrowhead at its last point.
    Route,
    /// Free text: the marker's name, at `position`.
    Text,
    /// A numbered or lettered pin with an icon.
    Pin,
}
impl Shape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Point | Self::Circle => tr!("combat-editor-round"),
            Self::Square => tr!("combat-editor-rectangle"),
            Self::Line => tr!("tactical-line"),
            Self::Rectangle => tr!("combat-editor-rectangle"),
            Self::Freehand => tr!("combat-editor-freehand"),
            Self::Polygon => tr!("combat-editor-polygon"),
            Self::Arrow => tr!("combat-editor-arrow"),
            Self::Route => tr!("combat-editor-route"),
            Self::Text => tr!("combat-editor-text"),
            Self::Pin => tr!("combat-editor-pin"),
        }
    }
    /// Closed areas: filled, hatched, picked by their inside.
    pub fn is_area(self) -> bool {
        matches!(self, Self::Circle | Self::Point | Self::Square | Self::Rectangle | Self::Polygon)
    }
}

/// How a line is drawn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// The look of a marker, beyond its colour. Every field has a default that
/// reads as the markers of the first version, and a default style is not
/// written, so an old file loads unchanged and a set stays small to send.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Style {
    /// Opacity of the fill, percent of the colour's own opacity.
    pub fill: u8,
    /// Stroke width, pixels.
    pub width: f32,
    pub dash: Dash,
    /// Text size, pixels.
    pub text_size: f32,
    /// The name is drawn on the map.
    pub label: bool,
    /// Tabler icon of a pin.
    pub icon: String,
    /// One to three characters on a pin (A, B, 1, 2...).
    pub badge: String,
    /// Rotation of a rectangle about its centre, degrees.
    pub angle: f32,
    /// Index in the set's layers.
    pub layer: u8,
    /// Hidden from the map (its own eye, besides the layer's).
    pub hidden: bool,
    /// Cannot be picked or moved (its own lock, besides the layer's).
    pub locked: bool,
    /// Where the name sits, metres from where it would by default.
    pub label_dx: f32,
    pub label_dy: f32,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            fill: 15,
            width: 2.0,
            dash: Dash::Solid,
            text_size: 12.0,
            label: true,
            icon: String::new(),
            badge: String::new(),
            angle: 0.0,
            layer: 0,
            hidden: false,
            locked: false,
            label_dx: 0.0,
            label_dy: 0.0,
        }
    }
}
impl Style {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn valid(&self) -> bool {
        self.fill <= 100
            && self.width.is_finite()
            && (0.5..=12.0).contains(&self.width)
            && self.text_size.is_finite()
            && (6.0..=64.0).contains(&self.text_size)
            && self.angle.is_finite()
            && self.angle.abs() <= 360.0
            && usize::from(self.layer) < MAX_LAYERS
            && self.label_dx.is_finite()
            && self.label_dy.is_finite()
            && self.label_dx.abs() <= 512.0
            && self.label_dy.abs() <= 512.0
            && self.badge.chars().count() <= 3
            && !self.badge.chars().any(char::is_control)
            && self.icon.len() <= 40
            && self.icon.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }
}

/// What undo brings back: the markers and the layers of the active set.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    pub markers: Vec<Marker>,
    pub layers: Vec<Layer>,
}

/// Undo and redo, for the set under edit. It is not part of the settings'
/// identity: a copy of the settings starts without one, and two settings
/// compare equal whatever their histories.
#[derive(Debug, Default)]
pub(crate) struct History {
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
    /// The region and set the history belongs to.
    pub of: Option<(super::sets::RegionKey, Option<Uuid>)>,
}
impl Clone for History {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl PartialEq for History {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
/// How many steps undo keeps.
const UNDO_STEPS: usize = 200;

/// A drag on the minimap while editing: what moves, and the state to go
/// back to (undo, or Escape).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EditGesture {
    Move {
        start: [f64; 2],
        originals: Vec<Marker>,
        before: Snapshot,
    },
    Handle {
        handle: crate::ui::combat_annotate::Handle,
        original: Marker,
        before: Snapshot,
    },
}
impl EditGesture {
    pub fn before(&self) -> &Snapshot {
        match self {
            Self::Move { before, .. } | Self::Handle { before, .. } => before,
        }
    }
}

/// How many layers a set may hold.
pub(crate) const MAX_LAYERS: usize = 8;

/// A named group of markers that can be hidden or locked together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
}
impl Default for Layer {
    fn default() -> Self {
        Self {
            name: String::new(),
            visible: true,
            locked: false,
        }
    }
}
impl Layer {
    /// The layers a new set starts with.
    pub fn defaults() -> Vec<Layer> {
        [tr!("combat-layer-zones"), tr!("combat-layer-routes"), tr!("combat-layer-callouts")]
            .into_iter()
            .map(|name| Layer {
                name: name.into(),
                ..Layer::default()
            })
            .collect()
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
    #[serde(default, skip_serializing_if = "Style::is_default")]
    pub style: Style,
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
            && (self.shape != Shape::Route || (2..=128).contains(&self.path.len()))
            && (self.shape != Shape::Polygon || (3..=64).contains(&self.path.len()))
            && (!matches!(self.shape(), Shape::Square | Shape::Line | Shape::Rectangle | Shape::Arrow)
                || self
                    .end
                    .is_some_and(|end| (end[0] - self.position[0]).abs() <= 1024.0 && (end[1] - self.position[1]).abs() <= 1024.0))
            && (!matches!(self.shape, Shape::Polygon | Shape::Route | Shape::Freehand)
                || crate::ui::combat_annotate::extent(&self.path) <= 1024.0)
            && self.style.valid()
    }
    /// A marker of `shape` from a drag: `Rectangle`, `Square`, `Line`, `Arrow`.
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
            name: String::new(),
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
    /// The group title on their name tag; unknown beyond the draw distance.
    pub title: Option<String>,
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
    /// The group you wear, when you wear one.
    pub worn_group: Option<Uuid>,
    /// Names of the teammates, coloured residents and senders not around.
    pub names: BTreeMap<Uuid, String>,
    /// Everyone who counts as an ally now (Allies tab).
    pub allies: std::collections::BTreeSet<Uuid>,
    /// The worn group is allied but its member list has not arrived.
    pub allies_loading: bool,
}
impl Context {
    pub fn from_world(world: &World, cfg: &Settings) -> Self {
        let (x, y) = crate::ui::minimap::to_global(world, world.agent.position);
        let me = [x, y, world.agent.position.z as f64];
        let avatars = crate::ui::minimap::avatars(world);
        let titles: BTreeMap<Uuid, String> = world
            .objects
            .avatars()
            .filter_map(|(_, o)| o.group_title().map(|t| (o.full_id, t)))
            .collect();
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
                    title: titles.get(id).cloned(),
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
            worn_group: (!active.is_nil()).then_some(active),
            names,
            allies: cfg.allies.resolve(world, &cfg.recipients),
            allies_loading: cfg.allies.group_loading(world),
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

/// The ground under the markers on the editor's map and in the picture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum Ground {
    /// Height-shaded terrain (and the map tiles in the editor).
    #[default]
    Shaded,
    /// One plain colour.
    Flat,
}

/// What the editor's map shows under the drawing, and what the picture of
/// the map is made of. It is kept with the combat settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MapView {
    pub ground: Ground,
    pub objects: bool,
    pub parcels: bool,
    pub grid: bool,
    /// How bright the ground is, 0.2 to 1.
    pub dim: f32,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            ground: Ground::Shaded,
            objects: true,
            parcels: true,
            grid: false,
            dim: 1.0,
        }
    }
}

impl MapView {
    pub const PRESETS: usize = 3;

    /// 0 tactical (soft terrain, parcel lines), 1 detailed (everything),
    /// 2 clean (a plain ground, nothing else).
    pub fn preset(n: usize, grid: bool) -> Self {
        match n {
            0 => Self {
                ground: Ground::Shaded,
                objects: false,
                parcels: true,
                grid,
                dim: 0.6,
            },
            1 => Self { grid, ..Self::default() },
            _ => Self {
                ground: Ground::Flat,
                objects: false,
                parcels: false,
                grid,
                dim: 1.0,
            },
        }
    }

    /// The preset this view is, grid apart.
    pub fn preset_of(&self) -> Option<usize> {
        (0..Self::PRESETS).find(|n| {
            let v = Self::preset(*n, self.grid);
            v.ground == self.ground && v.objects == self.objects && v.parcels == self.parcels && (v.dim - self.dim).abs() < 0.005
        })
    }

    /// Within bounds, whatever a file said.
    pub fn sane(mut self) -> Self {
        self.dim = if self.dim.is_finite() { self.dim.clamp(0.2, 1.0) } else { 1.0 };
        self
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
    /// The active set's layers (kept with the set, like `markers`).
    #[serde(skip)]
    pub layers: Vec<Layer>,
    /// Undo and redo of the active set, shared by the editor and the minimap.
    #[serde(skip)]
    pub history: History,
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
    /// A colour per group title worn around you: the group of a resident is only known by it.
    pub title_colors: Vec<(String, [u8; 4])>,
    pub recipients: BTreeMap<Uuid, bool>,
    /// The Allies tab: the teammates above, the worn group and group titles.
    pub allies: super::allies::Allies,
    pub ping_sound: bool,
    pub sound_asset: String,
    pub sound_volume: f32,
    #[serde(skip)]
    pub pings: Vec<Ping>,
    #[serde(skip)]
    pub outgoing: Vec<(Marker, usize)>,
    #[serde(skip)]
    pub sound_pending: bool,
    pub fixed_center: bool,
    pub map_view: MapView,
    #[serde(skip)]
    pub draw: Option<Shape>,
    #[serde(skip)]
    pub editing: bool,
    #[serde(skip)]
    pub edit_drag: Option<EditGesture>,
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
            layers: Layer::defaults(),
            history: History::default(),
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
            title_colors: Vec::new(),
            recipients: BTreeMap::new(),
            allies: Default::default(),
            ping_sound: false,
            sound_asset: String::new(),
            sound_volume: 0.7,
            pings: Vec::new(),
            outgoing: Vec::new(),
            sound_pending: false,
            fixed_center: false,
            map_view: MapView::default(),
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
    pub fn layer_visible(&self, m: &Marker) -> bool {
        self.layers.get(usize::from(m.style.layer)).is_none_or(|l| l.visible)
    }
    pub fn layer_locked(&self, m: &Marker) -> bool {
        self.layers.get(usize::from(m.style.layer)).is_some_and(|l| l.locked)
    }
    /// Drops a drag under way, putting the markers back as they were.
    pub fn cancel_gesture(&mut self) {
        if let Some(gesture) = self.edit_drag.take() {
            let before = gesture.before().clone();
            (self.markers, self.layers) = (before.markers, before.layers);
        }
    }
    /// The next unused letter, then number, for a pin's badge.
    pub fn next_badge(&self) -> String {
        let used: std::collections::BTreeSet<&str> = self
            .markers
            .iter()
            .filter(|m| m.shape == Shape::Pin)
            .map(|m| m.style.badge.as_str())
            .collect();
        ('A'..='Z')
            .map(|c| c.to_string())
            .chain((1..=99).map(|n| n.to_string()))
            .find(|b| !used.contains(b.as_str()))
            .unwrap_or_default()
    }
    /// Puts a nameless marker (`Point` or `Pin`) on the map, selected, as one
    /// undo step: the minimap's quick placement.
    pub fn place_marker(&mut self, shape: Shape, position: [f64; 3]) {
        if self.markers.len() >= 256 {
            return;
        }
        self.sync_history();
        let before = self.snapshot();
        let mut m = Marker {
            id: Uuid::new_v4(),
            position,
            color: self.colors[2],
            shape,
            selected: true,
            ..Marker::default()
        };
        if shape == Shape::Pin {
            m.style.badge = self.next_badge();
            m.style.icon = "map-pin-filled".into();
        }
        for other in &mut self.markers {
            other.selected = false;
        }
        self.markers.push(m);
        self.push_undo(before);
    }
    /// On the map: its layer and itself are not hidden.
    pub fn shown(&self, m: &Marker) -> bool {
        self.layer_visible(m) && !m.style.hidden
    }
    /// Fixed in place: its layer or itself is locked.
    pub fn frozen(&self, m: &Marker) -> bool {
        self.layer_locked(m) || m.style.locked
    }
    /// The markers to draw, back to front: by layer, then in the set's order.
    pub fn draw_order(&self) -> Vec<&Marker> {
        let top = self.layers.len().saturating_sub(1);
        let mut shown: Vec<&Marker> = self.markers.iter().filter(|m| self.shown(m)).collect();
        shown.sort_by_key(|m| usize::from(m.style.layer).min(top));
        shown
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            markers: self.markers.clone(),
            layers: self.layers.clone(),
        }
    }
    /// Starts a history for the set now active; an older one is dropped.
    pub fn sync_history(&mut self) {
        let of = self.current_region.clone().map(|key| {
            let active = self.library.regions.iter().find(|r| r.key == key).and_then(|r| r.active);
            (key, active)
        });
        if self.history.of != of {
            self.history = History { of, ..History::default() };
        }
    }
    /// Remembers `before`, the state a change started from.
    pub fn push_undo(&mut self, before: Snapshot) {
        if self.history.undo.len() == UNDO_STEPS {
            self.history.undo.remove(0);
        }
        self.history.undo.push(before);
        self.history.redo.clear();
    }
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.history.undo.pop() else {
            return false;
        };
        let now = self.snapshot();
        self.history.redo.push(now);
        (self.markers, self.layers) = (previous.markers, previous.layers);
        true
    }
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.history.redo.pop() else {
            return false;
        };
        let now = self.snapshot();
        self.history.undo.push(now);
        (self.markers, self.layers) = (next.markers, next.layers);
        true
    }
    /// Reads the active set of `region` into the working copy.
    pub(crate) fn adopt(&mut self, set: Option<&super::sets::MarkerSet>) {
        self.markers = set.map(|s| s.markers.clone()).unwrap_or_default();
        self.layers = set
            .map(|s| s.layers.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(Layer::defaults);
    }
    /// Adds a set read from a file as a new set of this region, which becomes
    /// the active one; false when there is no region to hold it.
    pub fn import_set(&mut self, file: super::sets::SetFile) -> bool {
        let Some(key) = self.current_region.clone() else {
            return false;
        };
        self.save_set();
        if let Some(view) = file.view {
            self.map_view = view.sane();
        }
        let region = self.library.region_mut(&key, "");
        let name = if file.name.is_empty() {
            tr!("tactical-set-default").to_owned()
        } else {
            file.name
        };
        region.add(name, &file.markers);
        region.save_layers(&file.layers);
        let set = region.selected().cloned();
        self.adopt(set.as_ref());
        self.sync_history();
        true
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
        let file = super::sets::SetFile::new(self.active_set_name(), self.layers.clone(), markers);
        match super::transfer::encode(&file) {
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
        self.cancel_gesture();
        self.editing = false;
        if let Some(previous) = self.current_region.take() {
            let region = self.library.region_mut(&previous, "");
            region.save(&self.markers);
            region.save_layers(&self.layers);
        } else if !self.markers.is_empty() {
            self.library.unassigned.append(&mut self.markers);
        }
        let region = self.library.region_mut(&key, name);
        if region.sets.is_empty() {
            region.add(tr!("tactical-set-default").into(), &[]);
        }
        let set = region.selected().cloned();
        self.adopt(set.as_ref());
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
            region.save_layers(&self.layers);
        }
    }

    pub fn add_marker(&mut self, position: [f64; 3]) {
        if self.markers.len() < 256 {
            self.markers.push(Marker {
                id: Uuid::new_v4(),
                name: String::new(),
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
            self.outgoing.push((marker.clone(), kind));
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
    /// A ping a peer placed: kept for its few seconds, at most one per peer
    /// every three seconds.
    pub fn accept_place(&mut self, sender: Uuid, marker: Marker, kind: usize, now: Instant) -> bool {
        if sender.is_nil() || kind >= 4 || !marker.valid() || self.seen_messages.contains_key(&marker.id) || !self.pings_enabled {
            return false;
        }
        if self
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
        let mut ping = Ping::new(marker.clone(), kind, now);
        ping.sender = sender;
        self.pings.push(ping);
        self.sound_pending = true;
        self.seen_messages.insert(marker.id, now);
        true
    }
    /// The colour given to a group title, whatever its case or spacing.
    pub fn title_color(&self, title: &str) -> Option<[u8; 4]> {
        let key = super::allies::clean_tag(title).to_lowercase();
        self.title_colors
            .iter()
            .find(|(t, _)| super::allies::clean_tag(t).to_lowercase() == key)
            .map(|(_, c)| *c)
    }
    /// Gives a group title a colour (the one it has already is replaced).
    pub fn set_title_color(&mut self, title: &str, color: [u8; 4]) {
        let title = super::allies::clean_tag(title);
        if title.is_empty() {
            return;
        }
        let key = title.to_lowercase();
        if let Some(entry) = self.title_colors.iter_mut().find(|(t, _)| t.to_lowercase() == key) {
            entry.1 = color;
        } else if self.title_colors.len() < 32 {
            self.title_colors.push((title, color));
        }
    }
    pub fn remove_title_color(&mut self, title: &str) {
        let key = super::allies::clean_tag(title).to_lowercase();
        self.title_colors.retain(|(t, _)| t.to_lowercase() != key);
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
            for marker in self.draw_order() {
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
        crate::ui::combat_annotate::paint(
            painter,
            view,
            m,
            &crate::ui::combat_annotate::Look {
                labels: self.labels,
                point_size: self.size,
                ..Default::default()
            },
        );
    }
}
/// A map of callouts for the offline demo, around `at` (global metres): a
/// zone, a turned cover, a route, an arrow, two pins, a text and a hatched
/// circle, on the three default layers.
pub(crate) fn demo_callouts(at: [f64; 2], z: f64) -> Vec<Marker> {
    let pt = |dx: f64, dy: f64| [at[0] + dx, at[1] + dy, z];
    let base = |shape: Shape, name: &str, color: [u8; 4], layer: u8| Marker {
        id: Uuid::new_v4(),
        name: name.into(),
        shape,
        color,
        position: pt(0.0, 0.0),
        style: Style { layer, ..Style::default() },
        ..Marker::default()
    };
    let mut out = Vec::new();
    let mut bridge = base(Shape::Polygon, "Bridge", [239, 68, 68, 255], 0);
    bridge.path = vec![
        pt(-60.0, 30.0),
        pt(-30.0, 50.0),
        pt(10.0, 44.0),
        pt(22.0, 20.0),
        pt(-8.0, 12.0),
        pt(-40.0, 8.0),
    ];
    bridge.position = bridge.path[0];
    bridge.style.fill = 28;
    bridge.style.dash = Dash::Dashed;
    bridge.style.width = 2.5;
    bridge.selected = true;
    out.push(bridge);
    let mut cover = base(Shape::Rectangle, "Cover", [96, 165, 250, 255], 0);
    cover.position = pt(30.0, -30.0);
    cover.end = Some(pt(62.0, -14.0));
    cover.style.fill = 22;
    cover.style.angle = 28.0;
    out.push(cover);
    let mut yard = base(Shape::Circle, "Yard", [74, 222, 128, 255], 0);
    yard.position = pt(-50.0, -40.0);
    yard.radius = 18.0;
    yard.hatched = true;
    out.push(yard);
    let mut attack = base(Shape::Route, "Attack path", [249, 115, 22, 255], 1);
    attack.path = vec![pt(-80.0, -70.0), pt(-40.0, -62.0), pt(-5.0, -30.0), pt(20.0, 0.0), pt(34.0, 28.0)];
    attack.position = attack.path[0];
    attack.style.dash = Dash::Dashed;
    attack.style.width = 3.0;
    out.push(attack);
    let mut flank = base(Shape::Arrow, "Flank", [74, 222, 128, 255], 1);
    flank.position = pt(70.0, 50.0);
    flank.end = Some(pt(44.0, 12.0));
    flank.style.width = 3.0;
    out.push(flank);
    for (name, badge, icon, dx, dy, color) in [
        ("Spawn", "A", "flag-filled", -78.0, -78.0, [74, 222, 128, 255]),
        ("Objective", "B", "star-filled", 40.0, 36.0, [250, 204, 21, 255]),
    ] {
        let mut pin = base(Shape::Pin, name, color, 2);
        pin.position = pt(dx, dy);
        pin.style.badge = badge.into();
        pin.style.icon = icon.into();
        out.push(pin);
    }
    let mut text = base(Shape::Text, "MID", [232, 237, 251, 255], 2);
    text.position = pt(-10.0, 28.0);
    text.style.text_size = 18.0;
    out.push(text);
    out
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

/// A line of a colour kept for a group title, with how many people wear it around you.
fn title_line(ui: &mut egui::Ui, p: &Palette, title: &str, around: usize, cfg: &mut Settings, default: [u8; 4]) {
    ui.horizontal(|ui| {
        match cfg.title_colors.iter_mut().find(|(t, _)| t.eq_ignore_ascii_case(title)) {
            Some((_, c)) => {
                widgets::color_button(ui, p, ("title-color", title), c, false);
            }
            None => {
                if widgets::flat_button(ui, p, tr!("tactical-color-set")).clicked() {
                    cfg.set_title_color(title, default);
                }
            }
        }
        ui.label(title);
        if around > 0 {
            ui.label(
                egui::RichText::new(tr!("tactical-colors-around", count = around.to_string()))
                    .size(11.5)
                    .color(p.muted),
            );
        }
        if cfg.title_color(title).is_some() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, p, "x", tr!("tactical-color-remove"), true) {
                    cfg.remove_title_color(title);
                }
            });
        }
    });
}

/// The most residents listed at once; a search finds the others.
const RESIDENTS_SHOWN: usize = 30;

/// The group titles worn around you with how many wear each, the most worn first.
pub(crate) fn titles_around(context: &Context) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for n in &context.nearby {
        let Some(title) = n.title.as_deref().map(super::allies::clean_tag).filter(|t| !t.is_empty()) else {
            continue;
        };
        match counts.iter_mut().find(|(t, _)| t.eq_ignore_ascii_case(&title)) {
            Some(entry) => entry.1 += 1,
            None => counts.push((title, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    counts
}

/// The avatar colours of the Mini-map tab. Only what is on the sim: your worn group, the groups
/// of the people around you (by the title over their head) and those people. Colours you gave
/// to others that are not around come last, so they can be taken away.
pub(crate) fn avatar_colors(ui: &mut egui::Ui, p: &Palette, context: &Context, cfg: &mut Settings) -> bool {
    let before = (cfg.player_colors.clone(), cfg.group_colors.clone(), cfg.title_colors.clone());
    let default = cfg.colors[0];
    let sub = |ui: &mut egui::Ui, text: &str| ui.label(egui::RichText::new(text).size(11.5).color(p.muted));

    sub(ui, tr!("tactical-colors-groups"));
    if let Some(group) = context.groups.iter().find(|(id, _)| context.worn_group == Some(*id)) {
        color_line(
            ui,
            p,
            group.0,
            &group.1,
            tr!("tactical-colors-worn"),
            &mut cfg.group_colors,
            default,
        );
    }
    let around = titles_around(context);
    for (title, count) in &around {
        title_line(ui, p, title, *count, cfg, default);
    }
    if around.is_empty() && context.worn_group.is_none() {
        ui.label(egui::RichText::new(tr!("tactical-colors-no-titles")).color(p.muted));
    }
    // Colours kept for groups that are not around any more.
    let gone: Vec<String> = cfg
        .title_colors
        .iter()
        .map(|(t, _)| t.clone())
        .filter(|t| !around.iter().any(|(a, _)| a.eq_ignore_ascii_case(t)))
        .collect();
    let saved: Vec<Uuid> = cfg
        .group_colors
        .keys()
        .copied()
        .filter(|id| context.worn_group != Some(*id))
        .collect();
    if !gone.is_empty() || !saved.is_empty() {
        ui.add_space(4.0);
        sub(ui, tr!("tactical-colors-saved"));
        for title in &gone {
            title_line(ui, p, title, 0, cfg, default);
        }
        for id in &saved {
            let name = context.groups.iter().find(|(g, _)| g == id).map_or("…", |(_, n)| n.as_str());
            color_line(ui, p, *id, name, "", &mut cfg.group_colors, default);
        }
    }

    ui.add_space(8.0);
    sub(ui, tr!("tactical-colors-residents"));
    let search_id = ui.make_persistent_id("tactical-colors-search");
    let mut search: String = ui.data(|d| d.get_temp(search_id)).unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut search)
            .hint_text(tr!("tactical-colors-search"))
            .desired_width(f32::INFINITY),
    );
    let needle = search.trim().to_lowercase();
    ui.data_mut(|d| d.insert_temp(search_id, search));
    let wanted = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);
    let coloured: Vec<Uuid> = cfg.player_colors.keys().copied().collect();
    for id in &coloured {
        let name = context.name_of(id);
        if !wanted(&name) {
            continue;
        }
        let near = context.nearby.iter().find(|n| n.id == *id);
        let detail = near.map_or_else(|| tr!("sharing-away").to_owned(), distance_text);
        color_line(ui, p, *id, &name, &detail, &mut cfg.player_colors, default);
    }
    let others: Vec<&Nearby> = context
        .nearby
        .iter()
        .filter(|n| !coloured.contains(&n.id) && wanted(&n.name))
        .collect();
    for n in others.iter().take(RESIDENTS_SHOWN) {
        color_line(ui, p, n.id, &n.name, &distance_text(n), &mut cfg.player_colors, default);
    }
    if others.len() > RESIDENTS_SHOWN {
        ui.label(
            egui::RichText::new(tr!("tactical-colors-more", count = (others.len() - RESIDENTS_SHOWN).to_string()))
                .size(11.5)
                .color(p.muted),
        );
    }
    if coloured.is_empty() && context.nearby.is_empty() {
        ui.label(egui::RichText::new(tr!("tactical-colors-no-residents")).color(p.muted));
    }
    before != (cfg.player_colors.clone(), cfg.group_colors.clone(), cfg.title_colors.clone())
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
            let icon = crate::ui::combat_editor::shape_icon(shape);
            if widgets::action_button(ui, p, icon, shape.label(), widgets::ActionKind::Flat, egui::vec2(0.0, 26.0)).clicked() {
                cfg.enabled = true;
                cfg.draw = Some(shape);
                cfg.drawing_start = None;
            }
        }
    });
    before != (cfg.editing, cfg.enabled, cfg.labels, cfg.size)
}

/// The markers of the active set, one line each: a tick to send it, its
/// colour, its shape, its name, and a button to remove it. Shaping them is
/// the region editor's job (Marker sets tab).
pub(crate) fn marker_list(ui: &mut egui::Ui, p: &Palette, context: &Context, cfg: &mut Settings) {
    if widgets::action_button_if(
        ui,
        p,
        "map-pin-filled",
        tr!("tactical-add"),
        widgets::ActionKind::Flat,
        egui::vec2(0.0, 26.0),
        context.position.is_some(),
    )
    .clicked()
        && let Some(position) = context.position
    {
        cfg.place_marker(Shape::Pin, position);
    }
    ui.add_space(4.0);
    let mut remove = None;
    for (i, m) in cfg.markers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            widgets::checkbox(ui, p, &mut m.selected, "").on_hover_text(tr!("tactical-send-tick"));
            widgets::color_button(ui, p, ("marker-list-color", m.id), &mut m.color, true);
            if let Some(icon) = crate::ui::icons::global(crate::ui::combat_editor::shape_icon(m.shape())) {
                ui.add(egui::Image::new((icon.id(), egui::Vec2::splat(14.0))).tint(p.muted));
            }
            let room = (ui.available_width() - 34.0).max(60.0);
            let hint = m.shape().label();
            ui.add(
                egui::TextEdit::singleline(&mut m.name)
                    .char_limit(128)
                    .hint_text(hint)
                    .desired_width(room),
            );
            if widgets::icon_button(ui, p, "trash-filled", tr!("tactical-remove"), true) {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        cfg.sync_history();
        let before = cfg.snapshot();
        cfg.markers.remove(i);
        cfg.push_undo(before);
    }
    cfg.save_set();
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
        let size = egui::vec2(0.0, 26.0);
        let flat = widgets::ActionKind::Flat;
        if widgets::action_button(ui, p, "plus", tr!("tactical-set-new"), flat, size).clicked() {
            region.add(tr!("tactical-set-default").into(), &[]);
        }
        if widgets::action_button(ui, p, "copy-filled", tr!("tactical-set-duplicate"), flat, size).clicked() {
            let name = region.selected().map(|s| s.name.clone()).unwrap_or_default();
            region.add(name, &cfg.markers);
            region.save_layers(&cfg.layers);
        }
        if widgets::action_button(ui, p, "trash-filled", tr!("tactical-remove"), widgets::ActionKind::Danger, size).clicked() {
            region.remove_active();
        }
        if widgets::action_button(ui, p, "device-floppy-filled", tr!("tactical-set-save"), flat, size).clicked() {
            region.save(&cfg.markers);
            region.save_layers(&cfg.layers);
        }
    });
    if region.active != previous {
        let set = region.selected().cloned();
        cfg.adopt(set.as_ref());
        cfg.draw = None;
        cfg.drawing_start = None;
    }
    if !cfg.library.unassigned.is_empty() {
        widgets::surface(ui, p, |ui| {
            ui.label(tr!("tactical-set-unassigned"));
            if widgets::action_button(
                ui,
                p,
                "check",
                tr!("tactical-set-assign"),
                widgets::ActionKind::Flat,
                egui::vec2(0.0, 26.0),
            )
            .clicked()
            {
                let markers = std::mem::take(&mut cfg.library.unassigned);
                let region = cfg.library.region_mut(&key, "");
                region.add(tr!("tactical-set-unassigned").into(), &markers);
                let set = region.selected().cloned();
                cfg.adopt(set.as_ref());
            }
        });
    }
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
    fn group_colours_follow_the_titles_worn_around_whatever_their_case() {
        let mut cfg = Settings::default();
        cfg.set_title_color("  Aurora   Builders ", [1, 2, 3, 255]);
        assert_eq!(cfg.title_color("aurora builders"), Some([1, 2, 3, 255]));
        cfg.set_title_color("AURORA BUILDERS", [9, 9, 9, 255]);
        assert_eq!(cfg.title_colors.len(), 1);
        cfg.remove_title_color("Aurora Builders");
        assert!(cfg.title_colors.is_empty());
    }

    #[test]
    fn only_the_titles_worn_around_are_listed_the_most_worn_first() {
        let near = |n: u128, title: Option<&str>| Nearby {
            id: Uuid::from_u128(n),
            name: n.to_string(),
            distance: Some(5.0),
            approximate: false,
            title: title.map(str::to_owned),
        };
        let context = Context {
            nearby: vec![
                near(1, Some("Wolves")),
                near(2, None),
                near(3, Some("Aurora")),
                near(4, Some("wolves")),
                near(5, Some(" ")),
            ],
            ..Context::default()
        };
        assert_eq!(titles_around(&context), vec![("Wolves".to_owned(), 2), ("Aurora".to_owned(), 1)]);
    }

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
        let make = || Marker {
            id: Uuid::new_v4(),
            name: "objective".into(),
            position: [100.0, 200.0, 30.0],
            radius: 0.0,
            color: [255; 4],
            selected: false,
            shape: Shape::Point,
            end: None,
            ..Marker::default()
        };
        assert!(cfg.accept_place(Uuid::from_u128(1), make(), 0, now));
        assert!(
            !cfg.accept_place(Uuid::from_u128(1), make(), 1, now),
            "one ping per peer every three seconds"
        );
        assert!(cfg.accept_place(Uuid::from_u128(2), make(), 1, now), "another peer is not held");
        assert!(!cfg.accept_place(Uuid::from_u128(3), make(), 4, now), "a kind is one of four");
        assert_eq!((cfg.pings.len(), cfg.markers.len()), (2, 0), "a peer cannot add markers to the set");
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
            let bytes = serde_json::to_vec(&m).expect("wire");
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

    #[test]
    fn an_old_marker_loads_with_the_default_style_and_a_default_style_is_not_written() {
        let old = r#"{"id":"2b0f2a4e-0000-4000-8000-000000000001","name":"A","position":[1.0,2.0,3.0],"radius":5.0,"color":[1,2,3,255],"selected":false}"#;
        let m: Marker = serde_json::from_str(old).expect("old marker");
        assert_eq!(m.style, Style::default());
        assert!(m.valid());
        let written = serde_json::to_string(&m).expect("encode");
        assert!(!written.contains("style"), "{written}");
        let mut styled = m.clone();
        styled.style.dash = Dash::Dashed;
        styled.style.layer = 2;
        let back: Marker = serde_json::from_str(&serde_json::to_string(&styled).expect("encode")).expect("decode");
        assert_eq!(back, styled);
    }

    #[test]
    fn the_new_shapes_are_valid_only_with_what_they_need_and_in_bounds() {
        let at = [10.0, 10.0, 0.0];
        let base = |shape: Shape| Marker {
            id: Uuid::new_v4(),
            name: "m".into(),
            position: at,
            shape,
            ..Marker::default()
        };
        let mut polygon = base(Shape::Polygon);
        assert!(!polygon.valid(), "a polygon needs corners");
        polygon.path = vec![at, [20.0, 10.0, 0.0], [20.0, 20.0, 0.0]];
        assert!(polygon.valid());
        polygon.path = vec![at; 65];
        assert!(!polygon.valid(), "at most 64 corners");
        let mut route = base(Shape::Route);
        route.path = vec![at, [30.0, 30.0, 0.0]];
        assert!(route.valid());
        route.path = vec![at, [2000.0, 30.0, 0.0]];
        assert!(!route.valid(), "a route stays within 1024 m");
        let mut arrow = base(Shape::Arrow);
        assert!(!arrow.valid());
        arrow.end = Some([40.0, 40.0, 0.0]);
        assert!(arrow.valid());
        let mut text = base(Shape::Text);
        text.name.clear();
        assert!(text.valid(), "a text may still be empty: it waits for its words");
        let mut pin = base(Shape::Pin);
        pin.style.badge = "ABCD".into();
        assert!(!pin.valid(), "a badge has at most three characters");
        pin.style.badge = "A1".into();
        pin.style.icon = "flag-filled".into();
        assert!(pin.valid());
        pin.style.icon = "../../x".into();
        assert!(!pin.valid(), "an icon is a plain name");
        pin.style.icon.clear();
        pin.style.width = 40.0;
        assert!(!pin.valid(), "a width is bounded");
    }

    #[test]
    fn the_markers_of_a_set_keep_their_layers_with_the_set() {
        let mut cfg = Settings::default();
        let key = super::super::sets::RegionKey {
            grid: "g".into(),
            region: 7,
        };
        cfg.enter_region(key.clone(), "R");
        cfg.layers.push(Layer {
            name: "Mine".into(),
            visible: false,
            locked: true,
        });
        cfg.add_marker([5.0, 5.0, 5.0]);
        cfg.markers[0].style.layer = 3;
        cfg.save_set();
        let other = super::super::sets::RegionKey {
            grid: "g".into(),
            region: 8,
        };
        cfg.enter_region(other, "S");
        assert_eq!(cfg.layers, Layer::defaults(), "another region starts on the default layers");
        cfg.enter_region(key, "R");
        assert_eq!(cfg.layers.len(), 4);
        assert!(!cfg.layers[3].visible && cfg.layers[3].locked);
        assert_eq!(cfg.markers[0].style.layer, 3);
        assert!(!cfg.layer_visible(&cfg.markers[0]) && cfg.layer_locked(&cfg.markers[0]));
    }

    #[test]
    fn the_history_follows_the_set_and_is_not_part_of_the_settings() {
        let mut cfg = Settings::default();
        let key = super::super::sets::RegionKey {
            grid: "g".into(),
            region: 1,
        };
        cfg.enter_region(key, "R");
        cfg.sync_history();
        cfg.push_undo(cfg.snapshot());
        assert_eq!(cfg.history.undo.len(), 1);
        assert_eq!(cfg.clone(), cfg, "a copy compares equal without the history");
        assert!(cfg.clone().history.undo.is_empty());
        cfg.library
            .region_mut(&cfg.current_region.clone().expect("region"), "")
            .add("other".into(), &[]);
        cfg.sync_history();
        assert!(cfg.history.undo.is_empty(), "another active set starts a new history");
    }

    #[test]
    fn markers_come_without_a_name_and_the_marker_flags_hide_and_fix_them() {
        let mut cfg = Settings::default();
        cfg.add_marker([1.0, 2.0, 3.0]);
        assert!(cfg.markers[0].name.is_empty(), "no default name");
        let drawn = Marker::drawn(Shape::Line, [0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [1, 2, 3, 255]).expect("line");
        assert!(drawn.name.is_empty());
        let mut hidden = cfg.markers[0].clone();
        hidden.id = Uuid::new_v4();
        hidden.style.hidden = true;
        let mut fixed = cfg.markers[0].clone();
        fixed.id = Uuid::new_v4();
        fixed.style.locked = true;
        cfg.markers.extend([hidden.clone(), fixed.clone()]);
        assert!(cfg.shown(&cfg.markers[0]) && !cfg.shown(&hidden) && cfg.shown(&fixed));
        assert!(!cfg.frozen(&cfg.markers[0]) && cfg.frozen(&fixed));
        assert_eq!(cfg.draw_order().len(), 2, "a hidden marker is not drawn");
        let mut offset = fixed.clone();
        offset.style.label_dx = 900.0;
        assert!(!offset.valid(), "a name stays within 512 m of its marker");
    }

    #[test]
    fn the_draw_order_runs_layer_by_layer_and_keeps_the_sets_order_inside_one() {
        let mut cfg = Settings::default();
        let make = |name: &str, layer: u8| Marker {
            id: Uuid::new_v4(),
            name: name.into(),
            style: Style { layer, ..Style::default() },
            ..Marker::default()
        };
        cfg.markers = vec![
            make("top-a", 2),
            make("bottom-a", 0),
            make("top-b", 2),
            make("bottom-b", 0),
            make("mid", 1),
        ];
        let order: Vec<&str> = cfg.draw_order().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(order, vec!["bottom-a", "bottom-b", "mid", "top-a", "top-b"]);
    }

    #[test]
    fn quick_placement_adds_a_nameless_pin_with_the_next_letter_as_one_undo_step() {
        let mut cfg = Settings::default();
        cfg.place_marker(Shape::Pin, [10.0, 20.0, 30.0]);
        cfg.place_marker(Shape::Pin, [11.0, 21.0, 30.0]);
        cfg.place_marker(Shape::Point, [12.0, 22.0, 30.0]);
        let badges: Vec<&str> = cfg.markers.iter().map(|m| m.style.badge.as_str()).collect();
        assert_eq!(badges, vec!["A", "B", ""]);
        assert!(cfg.markers.iter().all(|m| m.name.is_empty()));
        assert_eq!(cfg.markers.iter().filter(|m| m.selected).count(), 1, "the new one is the selection");
        assert_eq!(cfg.history.undo.len(), 3);
        assert!(cfg.undo());
        assert_eq!(cfg.markers.len(), 2);
        assert!(cfg.redo());
        assert_eq!(cfg.markers.len(), 3);
    }

    #[test]
    fn a_dragged_gesture_put_back_restores_the_markers_as_they_were() {
        let mut cfg = Settings::default();
        cfg.place_marker(Shape::Pin, [10.0, 20.0, 30.0]);
        let before = cfg.snapshot();
        cfg.edit_drag = Some(EditGesture::Move {
            start: [0.0, 0.0],
            originals: cfg.markers.clone(),
            before: before.clone(),
        });
        cfg.markers[0].position = [99.0, 99.0, 30.0];
        cfg.cancel_gesture();
        assert_eq!(cfg.markers, before.markers);
        assert!(cfg.edit_drag.is_none());
    }

    #[test]
    fn map_view_presets_are_recognised_and_keep_the_grid() {
        for n in 0..MapView::PRESETS {
            assert_eq!(MapView::preset(n, true).preset_of(), Some(n));
            assert!(MapView::preset(n, true).grid);
        }
        let mut view = MapView::preset(1, false);
        view.dim = 0.5;
        assert_eq!(view.preset_of(), None, "a changed piece is no preset any more");
        let wild = MapView {
            dim: f32::NAN,
            ..MapView::default()
        }
        .sane();
        assert_eq!(wild.dim, 1.0);
        assert_eq!(
            MapView {
                dim: 0.0,
                ..MapView::default()
            }
            .sane()
            .dim,
            0.2
        );
    }

    #[test]
    fn the_map_view_is_kept_with_the_settings_and_old_settings_still_load() {
        let cfg = Settings {
            map_view: MapView::preset(2, true),
            ..Settings::default()
        };
        let text = serde_json::to_string(&cfg).expect("json");
        let back: Settings = serde_json::from_str(&text).expect("settings");
        assert_eq!(back.map_view, cfg.map_view);
        let old: Settings = serde_json::from_str("{}").expect("old settings");
        assert_eq!(old.map_view, MapView::default());
    }
}
