//! The region marker editor: a canvas over the region to draw callouts,
//! zones, routes and pins, with a tool rail, an inspector (properties,
//! layers, set) and a status bar. It edits the same working set as the
//! minimap, through the shared history, so undo and redo cover both.
//!
//! The colours of the markers and of the map are content, not the theme's.
use super::{
    combat_annotate::{self as annotate, Handle, Look, Project},
    combat_cards as cards,
    minimap::RegionImage,
    widgets,
};
use crate::{
    combat::tactical::{Dash, Ground, Layer, MAX_LAYERS, MapView, Marker, Settings, Shape, Snapshot, Style},
    theme::Palette,
    world::World,
};
use aurora_i18n::tr;
use egui::{Key, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use std::collections::BTreeSet;
use uuid::Uuid;

/// The colours offered for a marker, besides the picker.
const SWATCHES: [[u8; 4]; 10] = [
    [239, 68, 68, 255],
    [249, 115, 22, 255],
    [250, 204, 21, 255],
    [74, 222, 128, 255],
    [45, 212, 191, 255],
    [96, 165, 250, 255],
    [139, 92, 246, 255],
    [244, 114, 182, 255],
    [232, 237, 251, 255],
    [148, 163, 184, 255],
];

/// The icons a pin can carry.
const PIN_ICONS: [&str; 8] = [
    "map-pin-filled",
    "flag-filled",
    "star-filled",
    "bolt-filled",
    "alert-triangle-filled",
    "shield-filled",
    "user-filled",
    "bulb-filled",
];

/// A picture of the annotated map, asked for in the Set tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExportJob {
    /// Picture side, pixels.
    pub side: u32,
    pub title: bool,
    pub legend: bool,
    /// The markers alone, on a transparent picture.
    pub transparent: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Tool {
    #[default]
    Select,
    Pin,
    Circle,
    Rect,
    Polygon,
    Arrow,
    Route,
    Freehand,
    Text,
    Measure,
    Erase,
}

impl Tool {
    const ALL: [Tool; 11] = [
        Tool::Select,
        Tool::Pin,
        Tool::Circle,
        Tool::Rect,
        Tool::Polygon,
        Tool::Arrow,
        Tool::Route,
        Tool::Freehand,
        Tool::Text,
        Tool::Measure,
        Tool::Erase,
    ];
    fn icon(self) -> &'static str {
        match self {
            Tool::Select => "pointer-filled",
            Tool::Pin => "map-pin-filled",
            Tool::Circle => "circle-filled",
            Tool::Rect => "aspect-ratio-filled",
            Tool::Polygon => "polygon",
            Tool::Arrow => "arrow-up-right",
            Tool::Route => "route",
            Tool::Freehand => "pencil-filled",
            Tool::Text => "typography",
            Tool::Measure => "ruler-measure",
            Tool::Erase => "eraser",
        }
    }
    fn key(self) -> Key {
        match self {
            Tool::Select => Key::V,
            Tool::Pin => Key::P,
            Tool::Circle => Key::O,
            Tool::Rect => Key::R,
            Tool::Polygon => Key::G,
            Tool::Arrow => Key::A,
            Tool::Route => Key::L,
            Tool::Freehand => Key::F,
            Tool::Text => Key::T,
            Tool::Measure => Key::M,
            Tool::Erase => Key::E,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Tool::Select => tr!("combat-editor-select"),
            Tool::Pin => tr!("combat-editor-pin"),
            Tool::Circle => tr!("combat-editor-round"),
            Tool::Rect => tr!("combat-editor-rectangle"),
            Tool::Polygon => tr!("combat-editor-polygon"),
            Tool::Arrow => tr!("combat-editor-arrow"),
            Tool::Route => tr!("combat-editor-route"),
            Tool::Freehand => tr!("combat-editor-freehand"),
            Tool::Text => tr!("combat-editor-text"),
            Tool::Measure => tr!("combat-editor-measure"),
            Tool::Erase => tr!("combat-editor-erase"),
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Tool::Select => tr!("combat-editor-hint-select"),
            Tool::Pin => tr!("combat-editor-hint-pin"),
            Tool::Circle => tr!("combat-editor-hint-circle"),
            Tool::Rect => tr!("combat-editor-hint-rect"),
            Tool::Polygon => tr!("combat-editor-hint-polygon"),
            Tool::Arrow => tr!("combat-editor-hint-arrow"),
            Tool::Route => tr!("combat-editor-hint-route"),
            Tool::Freehand => tr!("combat-editor-hint-freehand"),
            Tool::Text => tr!("combat-editor-hint-text"),
            Tool::Measure => tr!("combat-editor-hint-measure"),
            Tool::Erase => tr!("combat-editor-hint-erase"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Panel {
    #[default]
    Properties,
    Layers,
    Map,
    Set,
}

/// What the primary button is doing on the canvas.
#[derive(Default)]
enum Drag {
    #[default]
    None,
    Band {
        from: Pos2,
    },
    Move {
        start: [f64; 2],
        originals: Vec<Marker>,
    },
    Handle {
        handle: Handle,
        original: Marker,
    },
    Draw {
        start: [f64; 3],
    },
    /// The name of a marker, dragged to where it reads best.
    Label {
        original: Marker,
        start: [f64; 2],
    },
}

/// The look new markers are given.
#[derive(Clone)]
struct Pen {
    color: [u8; 4],
    fill: u8,
    width: f32,
    dash: Dash,
    icon: &'static str,
}

impl Default for Pen {
    fn default() -> Self {
        Self {
            color: SWATCHES[2],
            fill: 15,
            width: 2.0,
            dash: Dash::Solid,
            icon: PIN_ICONS[0],
        }
    }
}

/// A style to start from in one click.
struct Preset {
    name: &'static str,
    tool: Tool,
    color: [u8; 4],
    fill: u8,
    width: f32,
    dash: Dash,
    icon: &'static str,
}

fn presets() -> [Preset; 6] {
    [
        Preset {
            name: tr!("combat-preset-spawn"),
            tool: Tool::Pin,
            color: SWATCHES[3],
            fill: 15,
            width: 2.0,
            dash: Dash::Solid,
            icon: "flag-filled",
        },
        Preset {
            name: tr!("combat-preset-objective"),
            tool: Tool::Pin,
            color: SWATCHES[2],
            fill: 15,
            width: 2.0,
            dash: Dash::Solid,
            icon: "star-filled",
        },
        Preset {
            name: tr!("combat-preset-danger"),
            tool: Tool::Polygon,
            color: SWATCHES[0],
            fill: 28,
            width: 2.5,
            dash: Dash::Dashed,
            icon: "alert-triangle-filled",
        },
        Preset {
            name: tr!("combat-preset-cover"),
            tool: Tool::Rect,
            color: SWATCHES[5],
            fill: 22,
            width: 2.0,
            dash: Dash::Solid,
            icon: "shield-filled",
        },
        Preset {
            name: tr!("combat-preset-attack"),
            tool: Tool::Route,
            color: SWATCHES[1],
            fill: 0,
            width: 3.0,
            dash: Dash::Dashed,
            icon: "bolt-filled",
        },
        Preset {
            name: tr!("combat-preset-callout"),
            tool: Tool::Text,
            color: SWATCHES[8],
            fill: 0,
            width: 2.0,
            dash: Dash::Solid,
            icon: "map-pin-filled",
        },
    ]
}

pub(crate) struct Editor {
    pub open: bool,
    pub images: Vec<RegionImage>,
    tool: Tool,
    panel: Panel,
    zoom: f32,
    pan: Vec2,
    drag: Drag,
    /// Points of a freehand stroke, a polygon or a route under way.
    draft: Vec<[f64; 3]>,
    /// Points of the ruler.
    ruler: Vec<[f64; 2]>,
    pen: Pen,
    /// The layer new markers go to.
    layer: usize,
    snap_grid: bool,
    snap_objects: bool,
    spacing: f64,
    clipboard: Vec<Marker>,
    identity: Option<(crate::combat::sets::RegionKey, Option<Uuid>)>,
    /// The state before the change being made, pushed to the history once
    /// the change has rested.
    pending: Option<Snapshot>,
    last_change: f64,
    focus_name: bool,
    /// The marker being renamed in the layer list, and whether its field
    /// still has to take the keyboard.
    renaming: Option<Uuid>,
    focus_rename: bool,
    /// The layer list row under the pointer, lit on the canvas next frame.
    row_hover: Option<Uuid>,
    row_hover_prev: Option<Uuid>,
    /// Layers folded in the list.
    collapsed: BTreeSet<usize>,
    /// The export asked for, and the choices behind it.
    export_job: Option<ExportJob>,
    export_size: usize,
    export_title: bool,
    export_legend: bool,
    export_transparent: bool,
    /// The file keeps how the map looked (the Map tab).
    file_with_view: bool,
    /// What the last file action said, and whether it worked.
    file_note: Option<(String, bool)>,
    /// The pointer on the map, in global metres.
    cursor: Option<[f64; 2]>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            open: false,
            images: Vec::new(),
            tool: Tool::Select,
            panel: Panel::Properties,
            zoom: 1.0,
            pan: Vec2::ZERO,
            drag: Drag::None,
            draft: Vec::new(),
            ruler: Vec::new(),
            pen: Pen::default(),
            layer: 0,
            snap_grid: false,
            snap_objects: true,
            spacing: 8.0,
            clipboard: Vec::new(),
            identity: None,
            pending: None,
            last_change: 0.0,
            focus_name: false,
            renaming: None,
            focus_rename: false,
            row_hover: None,
            row_hover_prev: None,
            collapsed: BTreeSet::new(),
            export_job: None,
            export_size: 1,
            export_title: true,
            export_legend: true,
            export_transparent: false,
            file_with_view: true,
            file_note: None,
            cursor: None,
        }
    }
}

/// The region on the canvas: global metres to screen points.
struct Map {
    rect: Rect,
    origin: [f64; 2],
    size: Vec2,
}

impl Map {
    fn screen(&self, p: [f64; 3]) -> Pos2 {
        self.at(p[0], p[1])
    }
    fn global(&self, p: Pos2, z: f64) -> [f64; 3] {
        [
            self.origin[0] + f64::from((p.x - self.rect.left()) / self.rect.width()) * f64::from(self.size.x),
            self.origin[1] + f64::from((self.rect.bottom() - p.y) / self.rect.height()) * f64::from(self.size.y),
            z,
        ]
    }
}

impl Project for Map {
    fn at(&self, x: f64, y: f64) -> Pos2 {
        pos2(
            self.rect.left() + ((x - self.origin[0]) / f64::from(self.size.x)) as f32 * self.rect.width(),
            self.rect.bottom() - ((y - self.origin[1]) / f64::from(self.size.y)) as f32 * self.rect.height(),
        )
    }
    fn ppm(&self) -> f32 {
        self.rect.width() / self.size.x
    }
}

// ---- selection helpers -------------------------------------------------------

fn selected(cfg: &Settings) -> Vec<usize> {
    cfg.markers.iter().enumerate().filter(|(_, m)| m.selected).map(|(i, _)| i).collect()
}

fn select_only(cfg: &mut Settings, id: Uuid) {
    for m in &mut cfg.markers {
        m.selected = m.id == id;
    }
}

fn select_none(cfg: &mut Settings) {
    for m in &mut cfg.markers {
        m.selected = false;
    }
}

/// Markers that can be touched: on a visible, unlocked layer.
fn editable(cfg: &Settings, m: &Marker) -> bool {
    cfg.shown(m) && !cfg.frozen(m)
}

/// The next unused letter, then number, for a pin's badge.
fn next_badge(cfg: &Settings) -> String {
    cfg.next_badge()
}

/// A marker with the pen's look, on the active layer.
fn fresh(cfg: &Settings, pen: &Pen, layer: usize, shape: Shape, name: String, position: [f64; 3]) -> Marker {
    Marker {
        id: Uuid::new_v4(),
        name,
        position,
        color: pen.color,
        shape,
        style: Style {
            fill: pen.fill,
            width: pen.width,
            dash: pen.dash,
            layer: layer.min(cfg.session.layers.len().saturating_sub(1)) as u8,
            ..Style::default()
        },
        ..Marker::default()
    }
}

/// Ramer-Douglas-Peucker: fewer points along the same stroke.
fn simplify(points: &[[f64; 3]], epsilon: f64) -> Vec<[f64; 3]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dy).max(1e-9);
    let (index, far) = points[1..points.len() - 1]
        .iter()
        .enumerate()
        .map(|(i, p)| (i + 1, ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / length))
        .fold((0, 0.0), |best, c| if c.1 > best.1 { c } else { best });
    if far > epsilon {
        let mut left = simplify(&points[..=index], epsilon);
        left.pop();
        left.extend(simplify(&points[index..], epsilon));
        left
    } else {
        vec![a, b]
    }
}

/// A round length for the scale bar.
fn nice_length(max_metres: f64) -> f64 {
    [1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0]
        .into_iter()
        .rev()
        .find(|v| *v <= max_metres)
        .unwrap_or(1.0)
}

impl Editor {
    /// Closes the change being made: it becomes one undo step.
    fn flush(&mut self, cfg: &mut Settings) {
        if let Some(before) = self.pending.take() {
            cfg.push_undo(before);
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, cfg: &mut Settings, world: &World) {
        if !self.open {
            return;
        }
        cfg.sync_history();
        let id = cfg.session.history.of.clone();
        if self.identity != id {
            self.identity = id;
            self.pending = None;
            self.drag = Drag::None;
            self.draft.clear();
            self.ruler.clear();
            self.pan = Vec2::ZERO;
            self.zoom = 1.0;
            self.layer = 0;
        }
        let mut open = self.open;
        let before = cfg.snapshot();
        let mut skip_history = false;
        widgets::Floater::new(
            "combat_marker_editor",
            tr!("combat-editor-title"),
            pos2(30.0, 55.0),
            vec2(1260.0, 760.0),
        )
        .show(ctx, p, &mut open, |ui| {
            skip_history = self.body(ui, p, cfg, world);
        });
        let now = ctx.input(|i| i.time);
        if !skip_history && (cfg.markers != before.markers || cfg.session.layers != before.layers) {
            if self.pending.is_none() {
                self.pending = Some(before);
            }
            self.last_change = now;
        }
        let gesture = !matches!(self.drag, Drag::None);
        if self.pending.is_some() {
            if !gesture && now - self.last_change > 0.35 {
                self.flush(cfg);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if !open {
            self.flush(cfg);
            self.drag = Drag::None;
            self.draft.clear();
        }
        self.open = open;
        cfg.save_set();
    }

    /// The window's content. True when undo or redo ran this frame.
    fn body(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, world: &World) -> bool {
        let Some(region) = world.main() else {
            ui.label(tr!("tactical-set-region-required"));
            return false;
        };
        let Some((ox, oy)) = world.main_origin() else {
            return false;
        };
        let size = vec2(region.heightmap.size_x as f32, region.heightmap.size_y as f32);
        let name = region.name.clone();
        let mut history_moved = false;
        self.row_hover_prev = self.row_hover.take();
        if cfg.session.layers.is_empty() {
            cfg.session.layers = Layer::defaults();
        }
        self.layer = self.layer.min(cfg.session.layers.len() - 1);
        if self.pen.fill == 0 && self.pen.width == 0.0 {
            self.pen = Pen::default();
        }

        // the viewer's own keys (walking) must not pick tools: only with the
        // pointer on the editor, or its window holding the keyboard
        let keyboard = !ui.ctx().egui_wants_keyboard_input()
            && (ui.ui_contains_pointer() || widgets::floater_holds_keyboard(ui.ctx(), "combat_marker_editor"));
        if keyboard {
            history_moved = self.shortcuts(ui, cfg);
        }
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(40.0);
                self.rail(ui, p, cfg);
            });
            let inspector = 290.0;
            let center = (ui.available_width() - inspector - ui.spacing().item_spacing.x).max(300.0);
            ui.vertical(|ui| {
                ui.set_width(center);
                history_moved |= self.bar(ui, p, cfg);
                self.canvas(ui, p, cfg, world, &name, [ox as f64, oy as f64], size, center);
            });
            ui.vertical(|ui| {
                ui.set_width(inspector);
                self.inspector(ui, p, cfg, world, [ox as f64, oy as f64]);
            });
        });
        history_moved
    }

    // ---- keyboard ------------------------------------------------------------

    /// Tool keys, undo, copy, delete, nudge. True when undo or redo ran.
    fn shortcuts(&mut self, ui: &mut egui::Ui, cfg: &mut Settings) -> bool {
        let (command, shift, alt) = ui.input(|i| (i.modifiers.command, i.modifiers.shift, i.modifiers.alt));
        let pressed = |ui: &egui::Ui, key: Key| ui.input(|i| i.key_pressed(key));
        let mut moved = false;
        if command {
            if pressed(ui, Key::Z) && !shift || pressed(ui, Key::Y) {
                let redo = pressed(ui, Key::Y);
                self.flush(cfg);
                moved = if redo { cfg.redo() } else { cfg.undo() };
            } else if pressed(ui, Key::Z) && shift {
                self.flush(cfg);
                moved = cfg.redo();
            } else if pressed(ui, Key::D) {
                self.duplicate(cfg, None);
            } else if pressed(ui, Key::C) || pressed(ui, Key::X) {
                self.clipboard = selected(cfg).into_iter().map(|i| cfg.markers[i].clone()).collect();
                if pressed(ui, Key::X) {
                    cfg.markers.retain(|m| !m.selected);
                }
            } else if pressed(ui, Key::V) && !self.clipboard.is_empty() {
                let at = self.cursor;
                let clip = self.clipboard.clone();
                self.paste(cfg, clip, at);
            } else if pressed(ui, Key::A) {
                for m in &mut cfg.markers {
                    m.selected = cfg
                        .session
                        .layers
                        .get(usize::from(m.style.layer))
                        .is_none_or(|l| l.visible && !l.locked);
                }
            }
            return moved;
        }
        for tool in Tool::ALL {
            if pressed(ui, tool.key()) {
                self.set_tool(tool);
            }
        }
        if pressed(ui, Key::Escape) {
            if !self.draft.is_empty() || !self.ruler.is_empty() {
                self.draft.clear();
                self.ruler.clear();
            } else if !matches!(self.drag, Drag::None) {
                self.drag = Drag::None;
            } else if self.tool != Tool::Select {
                self.tool = Tool::Select;
            } else {
                select_none(cfg);
            }
        }
        if pressed(ui, Key::Backspace) && !self.draft.is_empty() {
            self.draft.pop();
        } else if pressed(ui, Key::Delete) || pressed(ui, Key::Backspace) {
            let before = cfg.markers.len();
            let editable_ids: BTreeSet<Uuid> = cfg
                .markers
                .iter()
                .filter(|m| m.selected && editable(cfg, m))
                .map(|m| m.id)
                .collect();
            cfg.markers.retain(|m| !editable_ids.contains(&m.id));
            let _ = before;
        }
        if pressed(ui, Key::Enter) {
            self.finish_draft(cfg);
        }
        if pressed(ui, Key::Home) {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
        }
        let step = if shift {
            self.spacing
        } else if alt {
            0.1
        } else {
            1.0
        };
        let (dx, dy) = ui.input(|i| {
            (
                f64::from(i32::from(i.key_pressed(Key::ArrowRight)) - i32::from(i.key_pressed(Key::ArrowLeft))),
                f64::from(i32::from(i.key_pressed(Key::ArrowUp)) - i32::from(i.key_pressed(Key::ArrowDown))),
            )
        });
        if dx != 0.0 || dy != 0.0 {
            let ids: Vec<usize> = selected(cfg).into_iter().filter(|i| editable(cfg, &cfg.markers[*i])).collect();
            for i in ids {
                annotate::translate(&mut cfg.markers[i], dx * step, dy * step);
            }
        }
        moved
    }

    pub fn take_export(&mut self) -> Option<ExportJob> {
        self.export_job.take()
    }

    /// Opens on the layer list (the demo's `combat/editor-layers`).
    pub fn show_layers(&mut self) {
        self.panel = Panel::Layers;
    }

    /// Opens on the map tab (the demo's `combat/editor-map`).
    pub fn show_map(&mut self) {
        self.panel = Panel::Map;
    }

    /// Opens on the set tab (the demo's `combat/editor-set`).
    pub fn show_set(&mut self) {
        self.panel = Panel::Set;
    }

    fn set_tool(&mut self, tool: Tool) {
        if self.tool != tool {
            self.draft.clear();
            self.ruler.clear();
            self.drag = Drag::None;
        }
        self.tool = tool;
    }

    fn duplicate(&mut self, cfg: &mut Settings, at: Option<[f64; 2]>) {
        let clip: Vec<Marker> = selected(cfg).into_iter().map(|i| cfg.markers[i].clone()).collect();
        self.paste(cfg, clip, at);
    }

    /// Adds copies of `markers` (new identities), centred on `at` or shifted
    /// by one grid step, and selects them.
    fn paste(&mut self, cfg: &mut Settings, markers: Vec<Marker>, at: Option<[f64; 2]>) {
        if markers.is_empty() {
            return;
        }
        let boxes: Vec<([f64; 2], [f64; 2])> = markers.iter().filter_map(annotate::bounds).collect();
        let (lo, hi) = boxes.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), (a, b)| {
            ([lo[0].min(a[0]), lo[1].min(a[1])], [hi[0].max(b[0]), hi[1].max(b[1])])
        });
        let (dx, dy) = match at {
            Some(c) if lo[0] <= hi[0] => (c[0] - (lo[0] + hi[0]) * 0.5, c[1] - (lo[1] + hi[1]) * 0.5),
            _ => (self.spacing, -self.spacing),
        };
        select_none(cfg);
        for mut m in markers {
            if cfg.markers.len() >= 256 {
                break;
            }
            m.id = Uuid::new_v4();
            m.selected = true;
            annotate::translate(&mut m, dx, dy);
            cfg.markers.push(m);
        }
    }

    // ---- left rail and top bar --------------------------------------------------

    fn rail(&mut self, ui: &mut egui::Ui, p: &Palette, _cfg: &mut Settings) {
        for tool in Tool::ALL {
            let tip = format!("{}  ({})", tool.name(), tool.key().name());
            if widgets::icon_toggle_sized(ui, p, tool.icon(), &tip, self.tool == tool, 34.0) {
                self.set_tool(tool);
            }
            ui.add_space(2.0);
        }
    }

    /// Undo, redo, zoom and the grid. True when undo or redo ran.
    fn bar(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) -> bool {
        let mut moved = false;
        ui.horizontal_wrapped(|ui| {
            if widgets::icon_button(
                ui,
                p,
                "arrow-back-up",
                tr!("combat-editor-undo"),
                !cfg.session.history.undo.is_empty() || self.pending.is_some(),
            ) {
                self.flush(cfg);
                moved |= cfg.undo();
            }
            if widgets::icon_button(
                ui,
                p,
                "arrow-forward-up",
                tr!("combat-editor-redo"),
                !cfg.session.history.redo.is_empty(),
            ) {
                self.flush(cfg);
                moved |= cfg.redo();
            }
            ui.add_space(8.0);
            if widgets::icon_button(ui, p, "minus", tr!("combat-editor-zoom-out"), true) {
                self.zoom = (self.zoom / 1.4).max(0.5);
            }
            ui.label(format!("{:.0}%", self.zoom * 100.0));
            if widgets::icon_button(ui, p, "plus", tr!("combat-editor-zoom-in"), true) {
                self.zoom = (self.zoom * 1.4).min(40.0);
            }
            if widgets::icon_button(ui, p, "arrows-maximize", tr!("combat-editor-fit"), true) {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            ui.add_space(8.0);
            widgets::checkbox(ui, p, &mut cfg.map_view.objects, tr!("combat-editor-objects"));
            widgets::checkbox(ui, p, &mut cfg.map_view.parcels, tr!("combat-editor-parcels"));
            widgets::checkbox(ui, p, &mut cfg.map_view.grid, tr!("combat-editor-grid"));
            widgets::checkbox(ui, p, &mut self.snap_grid, tr!("combat-editor-snap"));
            widgets::checkbox(ui, p, &mut self.snap_objects, tr!("combat-editor-snap-objects"));
            if self.spacing < 1.0 {
                self.spacing = 8.0;
            }
            ui.add(egui::DragValue::new(&mut self.spacing).range(1.0..=64.0).suffix(" m"));
        });
        moved
    }

    // ---- the canvas ----------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn canvas(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        cfg: &mut Settings,
        world: &World,
        region: &str,
        origin: [f64; 2],
        size: Vec2,
        width: f32,
    ) {
        let room = vec2(width, (ui.available_height() - 26.0).max(240.0));
        let (canvas, response) = ui.allocate_exact_size(room, Sense::click_and_drag());
        let make = |zoom: f32, pan: Vec2| {
            let fit = (canvas.width() / size.x).min(canvas.height() / size.y) * zoom;
            Map {
                rect: Rect::from_center_size(canvas.center() + pan, size * fit),
                origin,
                size,
            }
        };
        let mut map = make(self.zoom, self.pan);
        let z = f64::from(world.agent.position.z);
        let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
        // wheel: zoom about the pointer
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(at) = response.hover_pos()
            {
                let world_at = map.global(at, z);
                self.zoom = (self.zoom * (scroll * 0.0015).exp()).clamp(0.5, 40.0);
                let after = make(self.zoom, self.pan);
                let moved = after.screen(world_at);
                self.pan += at - moved;
                map = make(self.zoom, self.pan);
            }
        }
        let space = ui.input(|i| i.key_down(Key::Space));
        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
            || (space && response.dragged_by(egui::PointerButton::Primary))
        {
            self.pan += response.drag_delta();
            map = make(self.zoom, self.pan);
        }
        if response.hovered() && (space || response.dragged_by(egui::PointerButton::Middle)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        } else if response.hovered() && self.tool != Tool::Select {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        self.cursor = pointer.filter(|at| canvas.contains(*at)).map(|at| {
            let g = map.global(at, z);
            [g[0], g[1]]
        });

        let snap = |this: &Self, cfg: &Settings, at: Pos2, skip: Option<Uuid>| -> [f64; 3] {
            let mut g = map.global(at, z);
            if this.snap_objects {
                let near = cfg
                    .markers
                    .iter()
                    .filter(|m| Some(m.id) != skip && cfg.layer_visible(m))
                    .flat_map(|m| {
                        let mut pts = vec![[m.position[0], m.position[1]]];
                        pts.extend(m.end.map(|e| [e[0], e[1]]));
                        pts.extend(m.path.iter().map(|q| [q[0], q[1]]));
                        pts.extend(annotate::outline(m));
                        pts
                    })
                    .map(|q| (map.at(q[0], q[1]).distance(at), q))
                    .filter(|(d, _)| *d <= 10.0)
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, q)) = near {
                    g[0] = q[0];
                    g[1] = q[1];
                    return g;
                }
            }
            if this.snap_grid {
                let s = this.spacing.max(1.0);
                g[0] = (g[0] / s).round() * s;
                g[1] = (g[1] / s).round() * s;
            }
            g
        };

        // ---- painting: map, grid, region edge
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 0.0, p.field);
        painter.rect_filled(map.rect, 0.0, p.panel);
        let view = cfg.map_view;
        for image in &self.images {
            // a flat ground leaves the terrain and the tiles out, a dim one darkens them
            if image.ground && view.ground == Ground::Flat {
                continue;
            }
            let tint = if image.ground {
                widgets::PICTURE_TINT.gamma_multiply(view.dim)
            } else {
                widgets::PICTURE_TINT
            };
            let sw = [image.origin.0, image.origin.1, 0.0];
            let ne = [sw[0] + image.size.0, sw[1] + image.size.1, 0.0];
            painter.with_clip_rect(canvas.intersect(map.rect)).image(
                image.texture.id(),
                Rect::from_two_pos(map.screen(sw), map.screen(ne)),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                tint,
            );
        }
        let ppm = map.ppm();
        if view.grid {
            let s = self.spacing.max(1.0);
            let clip = painter.with_clip_rect(canvas.intersect(map.rect));
            let steps = ((f64::from(size.x.max(size.y)) / s).ceil() as usize).min(4096);
            for n in 0..=steps {
                let x = map.rect.left() + ((n as f64 * s - (origin[0] % s)) as f32) * ppm;
                let y = map.rect.bottom() - ((n as f64 * s - (origin[1] % s)) as f32) * ppm;
                clip.line_segment(
                    [pos2(x, map.rect.top()), pos2(x, map.rect.bottom())],
                    Stroke::new(1.0, p.muted_dim.gamma_multiply(0.3)),
                );
                clip.line_segment(
                    [pos2(map.rect.left(), y), pos2(map.rect.right(), y)],
                    Stroke::new(1.0, p.muted_dim.gamma_multiply(0.3)),
                );
            }
        }
        painter.rect_stroke(map.rect, 0.0, Stroke::new(1.0, p.muted_dim), egui::StrokeKind::Outside);
        for n in 0..=4 {
            let m = f64::from(n) * f64::from(size.x) / 4.0;
            let x = map.rect.left() + m as f32 * ppm;
            painter.text(
                pos2(x, map.rect.bottom() + 2.0),
                egui::Align2::CENTER_TOP,
                format!("{m:.0}"),
                egui::FontId::proportional(10.0),
                p.muted_dim,
            );
            let y = map.rect.bottom() - (f64::from(n) * f64::from(size.y) / 4.0) as f32 * ppm;
            painter.text(
                pos2(map.rect.left() - 3.0, y),
                egui::Align2::RIGHT_CENTER,
                format!("{:.0}", f64::from(n) * f64::from(size.y) / 4.0),
                egui::FontId::proportional(10.0),
                p.muted_dim,
            );
        }
        painter.text(
            canvas.left_top() + vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!("{region} · N ↑"),
            egui::FontId::proportional(13.0),
            p.ink,
        );

        // ---- interaction
        let press_at = ui.input(|i| i.pointer.press_origin()).filter(|at| canvas.contains(*at)).or(pointer);
        let shift = ui.input(|i| i.modifiers.shift);
        let alt = ui.input(|i| i.modifiers.alt);
        let double = response.double_clicked_by(egui::PointerButton::Primary);
        let draw_click = response.clicked_by(egui::PointerButton::Primary) && !space;

        match self.tool {
            Tool::Select => {
                if response.drag_started_by(egui::PointerButton::Primary)
                    && !space
                    && let Some(at) = press_at
                {
                    self.select_press(cfg, &map, at, shift, alt, z);
                } else if response.clicked_by(egui::PointerButton::Primary)
                    && !space
                    && let Some(at) = press_at
                {
                    self.select_press(cfg, &map, at, shift, alt, z);
                    // a click leaves the drag at once
                    if matches!(self.drag, Drag::Move { .. } | Drag::Handle { .. } | Drag::Band { .. }) {
                        self.drag = Drag::None;
                    }
                }
                if double && self.pick(cfg, &map, press_at).is_some() {
                    self.panel = Panel::Properties;
                    self.focus_name = true;
                }
                self.select_drag(cfg, &map, &response, pointer, z, &snap, shift);
            }
            Tool::Circle | Tool::Rect | Tool::Arrow | Tool::Freehand => {
                if self.tool == Tool::Circle
                    && draw_click
                    && let Some(at) = press_at
                {
                    // a click is a point; a drag from the centre is a circle
                    let g = snap(self, cfg, at, None);
                    self.place_point(cfg, g);
                }
                if response.drag_started_by(egui::PointerButton::Primary)
                    && !space
                    && let Some(at) = press_at
                {
                    self.drag = Drag::Draw {
                        start: snap(self, cfg, at, None),
                    };
                    self.draft = if self.tool == Tool::Freehand {
                        vec![snap(self, cfg, at, None)]
                    } else {
                        Vec::new()
                    };
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && self.tool == Tool::Freehand
                    && let Some(at) = pointer
                {
                    let g = snap(self, cfg, map.rect.clamp(at), None);
                    if self.draft.len() < 400 && self.draft.last().is_none_or(|q| map.screen(*q).distance(at) >= 3.0) {
                        self.draft.push(g);
                    }
                }
                if response.drag_stopped_by(egui::PointerButton::Primary)
                    && let Drag::Draw { start } = self.drag
                {
                    let end = pointer.map(|at| snap(self, cfg, map.rect.clamp(at), None)).unwrap_or(start);
                    self.finish_drag_shape(cfg, start, end, shift);
                    self.drag = Drag::None;
                }
            }
            Tool::Polygon | Tool::Route => {
                if draw_click && let Some(at) = press_at {
                    let g = snap(self, cfg, at, None);
                    let closes = self.tool == Tool::Polygon && self.draft.len() >= 3 && map.screen(self.draft[0]).distance(at) <= 10.0;
                    if closes {
                        self.finish_draft(cfg);
                    } else if self.draft.len() < 64 {
                        self.draft.push(g);
                    }
                }
                if double {
                    // the double click's two clicks each added the same point
                    if self.draft.len() >= 2 && self.draft[self.draft.len() - 1] == self.draft[self.draft.len() - 2] {
                        self.draft.pop();
                    }
                    self.finish_draft(cfg);
                }
            }
            Tool::Pin | Tool::Text => {
                if draw_click && let Some(at) = press_at {
                    let g = snap(self, cfg, at, None);
                    if cfg.markers.len() < 256 {
                        let mut m = if self.tool == Tool::Pin {
                            let badge = next_badge(cfg);
                            let name = String::new();
                            let mut m = fresh(cfg, &self.pen, self.layer, Shape::Pin, name, g);
                            m.style.badge = badge;
                            m.style.icon = self.pen.icon.to_owned();
                            m
                        } else {
                            let mut m = fresh(cfg, &self.pen, self.layer, Shape::Text, String::new(), g);
                            m.style.text_size = 16.0;
                            m
                        };
                        m.selected = true;
                        select_none(cfg);
                        cfg.markers.push(m);
                        if self.tool == Tool::Text {
                            self.tool = Tool::Select;
                            self.panel = Panel::Properties;
                            self.focus_name = true;
                        }
                    }
                }
            }
            Tool::Measure => {
                if draw_click && let Some(at) = press_at {
                    if self.ruler.len() >= 16 {
                        self.ruler.clear();
                    }
                    let g = snap(self, cfg, at, None);
                    self.ruler.push([g[0], g[1]]);
                }
                if response.secondary_clicked() {
                    self.ruler.clear();
                }
            }
            Tool::Erase => {
                if (response.clicked_by(egui::PointerButton::Primary) || response.dragged_by(egui::PointerButton::Primary))
                    && !space
                    && let Some(at) = pointer
                {
                    let hit = annotate::pick(&map, cfg.markers.iter().filter(|m| editable(cfg, m)), at);
                    if let Some(id) = hit {
                        cfg.markers.retain(|m| m.id != id);
                    }
                }
            }
        }

        // ---- markers
        let hover = if self.tool == Tool::Select && matches!(self.drag, Drag::None) {
            pointer.and_then(|at| self.pick(cfg, &map, Some(at)))
        } else {
            None
        };
        let clip = painter.with_clip_rect(canvas);
        let lit = self.row_hover_prev;
        for m in cfg.draw_order() {
            annotate::paint(
                &clip,
                &map,
                m,
                &Look {
                    emphasis: m.selected || Some(m.id) == hover || Some(m.id) == lit,
                    labels: cfg.labels,
                    alpha: if cfg.frozen(m) { 0.8 } else { 1.0 },
                    point_size: 8.0,
                    placeholder: tr!("combat-editor-text-placeholder"),
                },
            );
        }
        self.paint_selection(&clip, p, cfg, &map);
        self.paint_drafts(&clip, p, cfg, &map, pointer, z, &snap);
        // the agent
        let me = super::minimap::to_global(world, world.agent.position);
        let at = map.at(me.0, me.1);
        if canvas.contains(at) {
            clip.circle_stroke(at, 6.0, Stroke::new(2.0, p.teal));
            clip.circle_filled(at, 2.5, p.teal);
        }
        // scale bar
        let metres = nice_length(f64::from(canvas.width() * 0.2 / ppm));
        let bar = canvas.left_bottom() + vec2(12.0, -14.0);
        let length = metres as f32 * ppm;
        clip.line_segment([bar, bar + vec2(length, 0.0)], Stroke::new(2.0, p.ink));
        clip.line_segment([bar + vec2(0.0, -4.0), bar + vec2(0.0, 4.0)], Stroke::new(2.0, p.ink));
        clip.line_segment([bar + vec2(length, -4.0), bar + vec2(length, 4.0)], Stroke::new(2.0, p.ink));
        annotate::halo_text(
            &clip,
            bar + vec2(length * 0.5, -6.0),
            egui::Align2::CENTER_BOTTOM,
            &format!("{metres:.0} m"),
            11.0,
            p.ink,
        );

        // ---- status bar
        ui.horizontal(|ui| {
            let readout = match self.cursor {
                Some(c) => format!("X {:.1}   Y {:.1}", c[0] - origin[0], c[1] - origin[1]),
                None => String::new(),
            };
            ui.label(egui::RichText::new(readout).monospace().color(p.muted));
            ui.label(egui::RichText::new(self.tool.hint()).color(p.muted));
            let n = cfg.markers.iter().filter(|m| m.selected).count();
            if n > 0 {
                ui.label(egui::RichText::new(tr!("combat-editor-selected", count = n.to_string())).color(p.violet_light));
            }
        });
    }

    /// The editable marker under `at`.
    fn pick(&self, cfg: &Settings, map: &Map, at: Option<Pos2>) -> Option<Uuid> {
        let at = at?;
        annotate::pick(map, cfg.markers.iter().filter(|m| editable(cfg, m)), at)
    }

    /// A press with the select tool: a handle, a marker, or the start of a band.
    fn select_press(&mut self, cfg: &mut Settings, map: &Map, at: Pos2, shift: bool, alt: bool, z: f64) {
        // a handle of the only selected marker
        let picked = selected(cfg);
        if let [i] = picked.as_slice()
            && editable(cfg, &cfg.markers[*i])
            && let Some(handle) = annotate::handle_at(map, &cfg.markers[*i], at)
        {
            let i = *i;
            match handle {
                Handle::Vertex(v) if alt => {
                    let min = if cfg.markers[i].shape == Shape::Polygon { 3 } else { 2 };
                    if cfg.markers[i].path.len() > min {
                        cfg.markers[i].path.remove(v);
                        if let Some(first) = cfg.markers[i].path.first().copied() {
                            cfg.markers[i].position = first;
                        }
                    }
                    return;
                }
                Handle::Edge(e) => {
                    let m = &mut cfg.markers[i];
                    let n = m.path.len();
                    let (a, b) = (m.path[e], m.path[(e + 1) % n]);
                    if n < 64 {
                        m.path.insert(e + 1, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, a[2]]);
                        self.drag = Drag::Handle {
                            handle: Handle::Vertex(e + 1),
                            original: m.clone(),
                        };
                    }
                    return;
                }
                _ => {
                    self.drag = Drag::Handle {
                        handle,
                        original: cfg.markers[i].clone(),
                    };
                    return;
                }
            }
        }
        // the name of a selected marker is dragged to where it reads best
        if let Some(m) = cfg
            .markers
            .iter()
            .find(|m| m.selected && editable(cfg, m) && annotate::label_hit(map, m, at, 8.0))
        {
            let g = map.global(at, z);
            self.drag = Drag::Label {
                original: m.clone(),
                start: [g[0], g[1]],
            };
            return;
        }
        let g = map.global(at, z);
        match self.pick(cfg, map, Some(at)) {
            Some(id) => {
                let was = cfg.markers.iter().find(|m| m.id == id).is_some_and(|m| m.selected);
                if shift {
                    if let Some(m) = cfg.markers.iter_mut().find(|m| m.id == id) {
                        m.selected = !was;
                    }
                } else if !was {
                    select_only(cfg, id);
                }
                let originals: Vec<Marker> = cfg.markers.iter().filter(|m| m.selected && editable(cfg, m)).cloned().collect();
                if !originals.is_empty() {
                    self.drag = Drag::Move {
                        start: [g[0], g[1]],
                        originals,
                    };
                }
            }
            None => {
                if !shift {
                    select_none(cfg);
                }
                self.drag = Drag::Band { from: at };
            }
        }
    }

    /// The drag of the select tool: move, reshape or band.
    #[allow(clippy::too_many_arguments)]
    fn select_drag(
        &mut self,
        cfg: &mut Settings,
        map: &Map,
        response: &egui::Response,
        pointer: Option<Pos2>,
        z: f64,
        snap: &dyn Fn(&Self, &Settings, Pos2, Option<Uuid>) -> [f64; 3],
        shift: bool,
    ) {
        let Some(at) = pointer else { return };
        if response.dragged_by(egui::PointerButton::Primary) {
            match &self.drag {
                Drag::Move { start, originals } => {
                    let (start, originals) = (*start, originals.clone());
                    let first = originals[0].id;
                    let now = snap(self, cfg, map.rect.clamp(at), Some(first));
                    let mut delta = [now[0] - start[0], now[1] - start[1]];
                    if self.snap_grid
                        && !self.snap_objects
                        && let Some(o) = originals.first()
                    {
                        let s = self.spacing.max(1.0);
                        let target = [
                            ((o.position[0] + delta[0]) / s).round() * s,
                            ((o.position[1] + delta[1]) / s).round() * s,
                        ];
                        delta = [target[0] - o.position[0], target[1] - o.position[1]];
                    }
                    for original in &originals {
                        if let Some(m) = cfg.markers.iter_mut().find(|m| m.id == original.id) {
                            *m = original.clone();
                            annotate::translate(m, delta[0], delta[1]);
                        }
                    }
                }
                Drag::Handle { handle, original } => {
                    let (handle, original) = (*handle, original.clone());
                    let now = snap(self, cfg, map.rect.clamp(at), Some(original.id));
                    let keep_square = shift;
                    if let Some(m) = cfg.markers.iter_mut().find(|m| m.id == original.id) {
                        annotate::drag_handle(m, &original, handle, [now[0], now[1]], keep_square, shift);
                    }
                }
                Drag::Label { original, start } => {
                    let (original, start) = (original.clone(), *start);
                    let g = map.global(map.rect.clamp(at), z);
                    if let Some(m) = cfg.markers.iter_mut().find(|m| m.id == original.id) {
                        m.style.label_dx = (f64::from(original.style.label_dx) + g[0] - start[0]).clamp(-512.0, 512.0) as f32;
                        m.style.label_dy = (f64::from(original.style.label_dy) + g[1] - start[1]).clamp(-512.0, 512.0) as f32;
                    }
                }
                Drag::Band { .. } => {}
                _ => {}
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if let Drag::Band { from } = self.drag {
                let band = Rect::from_two_pos(from, at);
                if band.width() > 3.0 || band.height() > 3.0 {
                    let ids: Vec<Uuid> = cfg
                        .markers
                        .iter()
                        .filter(|m| editable(cfg, m))
                        .filter(|m| {
                            annotate::bounds(m).is_some_and(|(lo, hi)| {
                                band.intersects(Rect::from_two_pos(map.at(lo[0], lo[1]), map.at(hi[0], hi[1])).expand(4.0))
                            })
                        })
                        .map(|m| m.id)
                        .collect();
                    for m in &mut cfg.markers {
                        if ids.contains(&m.id) {
                            m.selected = true;
                        }
                    }
                }
            }
            self.drag = Drag::None;
        }
    }

    /// A point, with no name: the circle tool's click.
    fn place_point(&mut self, cfg: &mut Settings, at: [f64; 3]) {
        if cfg.markers.len() >= 256 {
            return;
        }
        let mut m = fresh(cfg, &self.pen, self.layer, Shape::Point, String::new(), at);
        m.selected = true;
        select_none(cfg);
        cfg.markers.push(m);
    }

    /// A shape dragged out on the canvas becomes a marker.
    fn finish_drag_shape(&mut self, cfg: &mut Settings, start: [f64; 3], end: [f64; 3], square: bool) {
        if cfg.markers.len() >= 256 {
            self.draft.clear();
            return;
        }
        let pen = self.pen.clone();
        let (layer, tool) = (self.layer, self.tool);
        let made = match tool {
            Tool::Circle => {
                let radius = (end[0] - start[0]).hypot(end[1] - start[1]) as f32;
                (radius >= 0.5).then(|| {
                    let mut m = fresh(cfg, &pen, layer, Shape::Circle, String::new(), start);
                    m.radius = radius.min(1024.0);
                    m
                })
            }
            Tool::Rect => Marker::drawn(if square { Shape::Square } else { Shape::Rectangle }, start, end, pen.color).map(|r| {
                let mut m = fresh(cfg, &pen, layer, r.shape, String::new(), start);
                m.end = r.end;
                m
            }),
            Tool::Arrow => ((end[0] - start[0]).hypot(end[1] - start[1]) >= 1.0).then(|| {
                let mut m = fresh(cfg, &pen, layer, Shape::Arrow, String::new(), start);
                m.end = Some(end);
                m
            }),
            Tool::Freehand => {
                let path = simplify(&self.draft, 0.6);
                (path.len() >= 2).then(|| {
                    let mut m = fresh(cfg, &pen, layer, Shape::Freehand, String::new(), start);
                    m.path = path.into_iter().take(128).collect();
                    m
                })
            }
            _ => None,
        };
        self.draft.clear();
        if let Some(mut m) = made
            && m.valid()
        {
            select_none(cfg);
            m.selected = true;
            cfg.markers.push(m);
        }
    }

    /// Closes the polygon or route being clicked out.
    fn finish_draft(&mut self, cfg: &mut Settings) {
        let polygon = self.tool == Tool::Polygon;
        let route = self.tool == Tool::Route;
        if !(polygon || route) {
            return;
        }
        let enough = if polygon { 3 } else { 2 };
        if self.draft.len() >= enough && cfg.markers.len() < 256 {
            let shape = if polygon { Shape::Polygon } else { Shape::Route };
            let mut m = fresh(cfg, &self.pen, self.layer, shape, String::new(), self.draft[0]);
            m.path = self.draft.clone();
            if m.valid() {
                select_none(cfg);
                m.selected = true;
                cfg.markers.push(m);
            }
        }
        self.draft.clear();
    }

    fn paint_selection(&self, painter: &egui::Painter, p: &Palette, cfg: &Settings, map: &Map) {
        let picked = selected(cfg);
        for i in &picked {
            let m = &cfg.markers[*i];
            if let Some((lo, hi)) = annotate::bounds(m) {
                let r = Rect::from_two_pos(map.at(lo[0], lo[1]), map.at(hi[0], hi[1])).expand(7.0);
                painter.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(1.0, p.violet_light.gamma_multiply(0.7)),
                    egui::StrokeKind::Outside,
                );
            }
        }
        if let [i] = picked.as_slice() {
            let m = &cfg.markers[*i];
            if editable(cfg, m) {
                for (handle, at) in annotate::handles(map, m) {
                    match handle {
                        Handle::Rotate => {
                            painter.circle_filled(at, 5.0, p.violet);
                            painter.circle_stroke(at, 5.0, Stroke::new(1.0, p.ink));
                        }
                        Handle::Edge(_) => {
                            painter.circle_stroke(at, 3.5, Stroke::new(1.0, p.violet_light));
                        }
                        _ => {
                            let r = Rect::from_center_size(at, Vec2::splat(9.0));
                            painter.rect_filled(r, 0.0, p.ink);
                            painter.rect_stroke(r, 0.0, Stroke::new(1.5, p.violet), egui::StrokeKind::Outside);
                        }
                    }
                }
            }
        }
        if let Drag::Band { from } = self.drag
            && let Some(to) = painter.ctx().input(|i| i.pointer.latest_pos())
        {
            let band = Rect::from_two_pos(from, to);
            painter.rect_filled(band, 0.0, p.violet.gamma_multiply(0.15));
            painter.rect_stroke(band, 0.0, Stroke::new(1.0, p.violet_light), egui::StrokeKind::Inside);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_drafts(
        &self,
        painter: &egui::Painter,
        p: &Palette,
        cfg: &Settings,
        map: &Map,
        pointer: Option<Pos2>,
        z: f64,
        snap: &dyn Fn(&Self, &Settings, Pos2, Option<Uuid>) -> [f64; 3],
    ) {
        let pen_color = crate::combat::tactical::color(self.pen.color);
        let now = pointer.map(|at| snap(self, cfg, at, None));
        match (self.tool, &self.drag) {
            (Tool::Circle | Tool::Rect | Tool::Arrow, Drag::Draw { start }) => {
                if let Some(end) = now {
                    let shape = match self.tool {
                        Tool::Circle => Shape::Circle,
                        Tool::Rect => Shape::Rectangle,
                        _ => Shape::Arrow,
                    };
                    let mut preview = fresh(cfg, &self.pen, self.layer, shape, String::new(), *start);
                    match shape {
                        Shape::Circle => preview.radius = ((end[0] - start[0]).hypot(end[1] - start[1]) as f32).min(1024.0),
                        _ => preview.end = Some(end),
                    }
                    annotate::paint(
                        painter,
                        map,
                        &preview,
                        &Look {
                            alpha: 0.85,
                            labels: false,
                            ..Look::default()
                        },
                    );
                    let metres = (end[0] - start[0]).hypot(end[1] - start[1]);
                    annotate::halo_text(
                        painter,
                        map.screen(end) + vec2(12.0, -12.0),
                        egui::Align2::LEFT_BOTTOM,
                        &format!("{metres:.1} m"),
                        11.0,
                        p.ink,
                    );
                }
            }
            (Tool::Freehand, Drag::Draw { .. }) if self.draft.len() >= 2 => {
                let pts: Vec<Pos2> = self.draft.iter().map(|q| map.screen(*q)).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(self.pen.width, pen_color)));
            }
            _ => {}
        }
        if matches!(self.tool, Tool::Polygon | Tool::Route) && !self.draft.is_empty() {
            let mut pts: Vec<Pos2> = self.draft.iter().map(|q| map.screen(*q)).collect();
            for q in &pts {
                painter.circle_filled(*q, 3.5, pen_color);
            }
            if let Some(end) = now {
                pts.push(map.screen(end));
            }
            painter.add(egui::Shape::line(pts.clone(), Stroke::new(self.pen.width, pen_color)));
            if self.tool == Tool::Polygon && self.draft.len() >= 3 {
                let close = map.screen(self.draft[0]);
                painter.circle_stroke(close, 7.0, Stroke::new(1.5, p.ink));
                if let Some(last) = pts.last() {
                    painter.line_segment([*last, close], Stroke::new(1.0, pen_color.gamma_multiply(0.5)));
                }
            }
        }
        if self.tool == Tool::Measure && !self.ruler.is_empty() {
            let pts: Vec<Pos2> = self.ruler.iter().map(|q| map.at(q[0], q[1])).collect();
            let mut total = 0.0;
            for pair in self.ruler.windows(2) {
                total += (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
            }
            let mut line = pts.clone();
            let mut live = 0.0;
            if let (Some(end), Some(last)) = (now, self.ruler.last()) {
                line.push(map.screen(end));
                live = (end[0] - last[0]).hypot(end[1] - last[1]);
            }
            painter.add(egui::Shape::line(line.clone(), Stroke::new(2.0, p.violet_light)));
            for q in &pts {
                painter.circle_filled(*q, 3.5, p.violet_light);
            }
            if let Some(tip) = line.last() {
                annotate::halo_text(
                    painter,
                    *tip + vec2(12.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    &tr!("combat-editor-measure-total", total = format!("{:.1}", total + live)),
                    12.0,
                    p.ink,
                );
            }
        }
        let _ = z;
    }

    // ---- the inspector -----------------------------------------------------------------

    fn inspector(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, world: &World, origin: [f64; 2]) {
        let mut tab = match self.panel {
            Panel::Properties => 0,
            Panel::Layers => 1,
            Panel::Map => 2,
            Panel::Set => 3,
        };
        let marker_count = cfg.markers.len();
        let sets = cfg
            .current_region
            .as_ref()
            .and_then(|key| cfg.library.regions.iter().find(|r| &r.key == key))
            .map_or(0, |r| r.sets.len());
        if widgets::count_tabs(
            ui,
            p,
            "combat-editor-panel",
            &mut tab,
            &[
                (tr!("combat-editor-properties"), selected(cfg).len()),
                (tr!("combat-editor-layers"), marker_count),
                (tr!("combat-editor-map"), 0),
                (tr!("combat-editor-set"), sets),
            ],
        ) {
            self.panel = [Panel::Properties, Panel::Layers, Panel::Map, Panel::Set][tab.min(3)];
        }
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("combat-editor-inspector")
            .show(ui, |ui| match self.panel {
                Panel::Properties => self.properties(ui, p, cfg, world, origin),
                Panel::Layers => self.layers_panel(ui, p, cfg),
                Panel::Map => self.map_panel(ui, p, cfg),
                Panel::Set => self.set_panel(ui, p, cfg),
            });
    }

    fn swatches(ui: &mut egui::Ui, p: &Palette, id: &str, color: &mut [u8; 4]) -> bool {
        let before = *color;
        ui.horizontal_wrapped(|ui| {
            for swatch in SWATCHES {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                ui.painter()
                    .rect_filled(rect, ui.visuals().window_corner_radius, crate::combat::tactical::color(swatch));
                if *color == swatch {
                    ui.painter().rect_stroke(
                        rect.expand(1.5),
                        ui.visuals().window_corner_radius,
                        Stroke::new(2.0, p.violet_light),
                        egui::StrokeKind::Outside,
                    );
                }
                if resp.clicked() {
                    *color = swatch;
                }
            }
            widgets::color_button(ui, p, id, color, true);
        });
        *color != before
    }

    fn properties(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, _world: &World, origin: [f64; 2]) {
        let picked = selected(cfg);
        if picked.is_empty() {
            self.new_marker_style(ui, p, cfg);
            return;
        }
        let single = picked.len() == 1;
        let first = cfg.markers[picked[0]].clone();
        let mut edited = first.clone();
        let title = if single {
            first.shape.label().to_owned()
        } else {
            tr!("combat-editor-selected", count = picked.len().to_string()).to_owned()
        };
        ui.label(egui::RichText::new(title).strong().size(14.0).color(p.ink));
        ui.add_space(4.0);
        if single {
            let response = ui.add(
                egui::TextEdit::singleline(&mut edited.name)
                    .char_limit(128)
                    .hint_text(tr!("combat-editor-name-hint"))
                    .desired_width(f32::INFINITY),
            );
            if self.focus_name {
                response.request_focus();
                self.focus_name = false;
            }
            if (edited.style.label_dx != 0.0 || edited.style.label_dy != 0.0)
                && widgets::flat_button(ui, p, tr!("combat-editor-label-reset")).clicked()
            {
                edited.style.label_dx = 0.0;
                edited.style.label_dy = 0.0;
            }
        }
        ui.add_space(4.0);
        Self::swatches(ui, p, "combat-editor-color", &mut edited.color);
        ui.add_space(6.0);
        let all_areas = picked.iter().all(|i| cfg.markers[*i].shape().is_area());
        let any_text = picked.iter().any(|i| cfg.markers[*i].shape == Shape::Text);
        let all_pins = picked.iter().all(|i| cfg.markers[*i].shape == Shape::Pin);
        if all_areas {
            let mut fill = f32::from(edited.style.fill);
            if cards::slider_row(ui, tr!("combat-editor-fill"), "", &mut fill, 0.0..=100.0, " %") {
                edited.style.fill = fill as u8;
            }
            cards::switch_row(ui, p, &mut edited.hatched, tr!("combat-editor-hatch"), "");
        }
        if !any_text && !all_pins {
            cards::slider_row(ui, tr!("combat-editor-width"), "", &mut edited.style.width, 1.0..=8.0, " px");
            let mut dash = match edited.style.dash {
                Dash::Solid => 0,
                Dash::Dashed => 1,
                Dash::Dotted => 2,
            };
            if widgets::segmented(
                ui,
                p,
                &mut dash,
                &[
                    (tr!("combat-editor-solid"), false),
                    (tr!("combat-editor-dashed"), false),
                    (tr!("combat-editor-dotted"), false),
                ],
            ) {
                edited.style.dash = [Dash::Solid, Dash::Dashed, Dash::Dotted][dash.min(2)];
            }
        }
        if any_text || !all_pins {
            cards::slider_row(
                ui,
                tr!("combat-editor-text-size"),
                "",
                &mut edited.style.text_size,
                8.0..=40.0,
                " px",
            );
        }
        cards::switch_row(ui, p, &mut edited.style.label, tr!("combat-editor-show-name"), "");
        if single {
            self.shape_fields(ui, p, &mut edited, origin);
        }
        if cfg.session.layers.len() > 1 {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr!("combat-editor-layer")).size(11.5).color(p.muted));
            let labels: Vec<(&str, bool)> = cfg.session.layers.iter().map(|l| (l.name.as_str(), false)).collect();
            let mut layer = usize::from(edited.style.layer).min(cfg.session.layers.len() - 1);
            if widgets::segmented(ui, p, &mut layer, &labels) {
                edited.style.layer = layer as u8;
            }
        }
        // apply what changed to every selected marker
        if edited != first {
            for i in &picked {
                let m = &mut cfg.markers[*i];
                if single {
                    *m = edited.clone();
                    m.selected = true;
                    continue;
                }
                if edited.color != first.color {
                    m.color = edited.color;
                }
                if edited.hatched != first.hatched && m.shape().is_area() {
                    m.hatched = edited.hatched;
                }
                let (a, b) = (&edited.style, &first.style);
                if a.fill != b.fill {
                    m.style.fill = a.fill;
                }
                if a.width != b.width {
                    m.style.width = a.width;
                }
                if a.dash != b.dash {
                    m.style.dash = a.dash;
                }
                if a.text_size != b.text_size {
                    m.style.text_size = a.text_size;
                }
                if a.label != b.label {
                    m.style.label = a.label;
                }
                if a.layer != b.layer {
                    m.style.layer = a.layer;
                }
            }
        }
        ui.add_space(8.0);
        if picked.len() > 1 {
            self.align_buttons(ui, p, cfg);
        }
        ui.horizontal_wrapped(|ui| {
            if widgets::action_button(
                ui,
                p,
                "copy-filled",
                tr!("combat-editor-duplicate"),
                widgets::ActionKind::Flat,
                vec2(0.0, 28.0),
            )
            .clicked()
            {
                self.duplicate(cfg, None);
            }
            if widgets::action_button(
                ui,
                p,
                "trash-filled",
                tr!("tactical-remove"),
                widgets::ActionKind::Danger,
                vec2(0.0, 28.0),
            )
            .clicked()
            {
                cfg.markers.retain(|m| !(m.selected && editable_ref(&cfg.session.layers, m)));
            }
        });
    }

    /// The fields that depend on the shape: size, angle, position, pin look.
    fn shape_fields(&mut self, ui: &mut egui::Ui, p: &Palette, m: &mut Marker, origin: [f64; 2]) {
        match m.shape() {
            Shape::Circle | Shape::Point => {
                let mut radius = m.radius;
                if cards::slider_row(
                    ui,
                    tr!("combat-editor-radius"),
                    tr!("combat-editor-radius-zero"),
                    &mut radius,
                    0.0..=256.0,
                    " m",
                ) {
                    m.radius = radius;
                    m.shape = if radius == 0.0 { Shape::Point } else { Shape::Circle };
                }
            }
            Shape::Square | Shape::Rectangle => {
                if let Some(end) = &mut m.end {
                    let mut w = (end[0] - m.position[0]).abs();
                    let mut h = (end[1] - m.position[1]).abs();
                    let c = [(m.position[0] + end[0]) * 0.5, (m.position[1] + end[1]) * 0.5];
                    let changed_w = drag_row(ui, tr!("combat-editor-width"), &mut w, 1.0..=1024.0, " m");
                    let changed_h = drag_row(ui, tr!("combat-editor-height"), &mut h, 1.0..=1024.0, " m");
                    if changed_w || changed_h {
                        // a square is only made by Shift on the canvas: its sides are free here
                        m.shape = Shape::Rectangle;
                        m.position = [c[0] - w * 0.5, c[1] - h * 0.5, m.position[2]];
                        *end = [c[0] + w * 0.5, c[1] + h * 0.5, m.position[2]];
                    }
                    cards::slider_row(ui, tr!("combat-editor-angle"), "", &mut m.style.angle, -180.0..=180.0, "°");
                }
            }
            Shape::Pin => {
                ui.label(egui::RichText::new(tr!("combat-editor-badge")).size(11.5).color(p.muted));
                ui.add(egui::TextEdit::singleline(&mut m.style.badge).char_limit(3).desired_width(60.0));
                ui.label(egui::RichText::new(tr!("combat-editor-icon")).size(11.5).color(p.muted));
                ui.horizontal_wrapped(|ui| {
                    for icon in PIN_ICONS {
                        if widgets::icon_toggle_sized(ui, p, icon, "", m.style.icon == icon, 28.0) {
                            m.style.icon = icon.to_owned();
                        }
                    }
                });
            }
            _ => {}
        }
        // where it is, in the region's own metres
        let mut local = [m.position[0] - origin[0], m.position[1] - origin[1]];
        let mut moved = false;
        ui.horizontal(|ui| {
            for (axis, label) in [(0, "X"), (1, "Y")] {
                moved |= ui
                    .add(
                        egui::DragValue::new(&mut local[axis])
                            .prefix(format!("{label} "))
                            .speed(0.1)
                            .max_decimals(2),
                    )
                    .changed();
            }
            moved |= ui
                .add(
                    egui::DragValue::new(&mut m.position[2])
                        .prefix("Z ")
                        .speed(0.1)
                        .max_decimals(1)
                        .range(-4096.0..=65536.0),
                )
                .changed();
        });
        if moved {
            let (dx, dy) = (local[0] + origin[0] - m.position[0], local[1] + origin[1] - m.position[1]);
            let z = m.position[2];
            annotate::translate(m, dx, dy);
            m.position[2] = z;
        }
    }

    fn align_buttons(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) {
        let ids: Vec<usize> = selected(cfg).into_iter().filter(|i| editable(cfg, &cfg.markers[*i])).collect();
        let boxes: Vec<(usize, [f64; 2], [f64; 2])> = ids
            .iter()
            .filter_map(|i| annotate::bounds(&cfg.markers[*i]).map(|(lo, hi)| (*i, lo, hi)))
            .collect();
        if boxes.len() < 2 {
            return;
        }
        let lo = boxes.iter().fold([f64::MAX; 2], |a, b| [a[0].min(b.1[0]), a[1].min(b.1[1])]);
        let hi = boxes.iter().fold([f64::MIN; 2], |a, b| [a[0].max(b.2[0]), a[1].max(b.2[1])]);
        ui.label(egui::RichText::new(tr!("combat-editor-align")).size(11.5).color(p.muted));
        ui.horizontal_wrapped(|ui| {
            for (icon, tip, axis, edge) in [
                ("layout-align-left-filled", tr!("combat-editor-align-left"), 0, 0),
                ("layout-align-center-filled", tr!("combat-editor-align-center"), 0, 1),
                ("layout-align-right-filled", tr!("combat-editor-align-right"), 0, 2),
                ("layout-align-top-filled", tr!("combat-editor-align-top"), 1, 2),
                ("layout-align-middle-filled", tr!("combat-editor-align-middle"), 1, 1),
                ("layout-align-bottom-filled", tr!("combat-editor-align-bottom"), 1, 0),
            ] {
                if widgets::icon_button(ui, p, icon, tip, true) {
                    for (i, a, b) in &boxes {
                        let target = match edge {
                            0 => lo[axis],
                            1 => (lo[axis] + hi[axis]) * 0.5,
                            _ => hi[axis],
                        };
                        let here = match edge {
                            0 => a[axis],
                            1 => (a[axis] + b[axis]) * 0.5,
                            _ => b[axis],
                        };
                        let d = target - here;
                        let (dx, dy) = if axis == 0 { (d, 0.0) } else { (0.0, d) };
                        annotate::translate(&mut cfg.markers[*i], dx, dy);
                    }
                }
            }
        });
    }

    /// With nothing selected: the look the next marker gets, and the presets.
    fn new_marker_style(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) {
        ui.label(
            egui::RichText::new(tr!("combat-editor-new-markers"))
                .strong()
                .size(14.0)
                .color(p.ink),
        );
        cards::muted(ui, p, self.tool.hint());
        ui.add_space(4.0);
        Self::swatches(ui, p, "combat-editor-pen-color", &mut self.pen.color);
        let mut fill = f32::from(self.pen.fill);
        if cards::slider_row(ui, tr!("combat-editor-fill"), "", &mut fill, 0.0..=100.0, " %") {
            self.pen.fill = fill as u8;
        }
        cards::slider_row(ui, tr!("combat-editor-width"), "", &mut self.pen.width, 1.0..=8.0, " px");
        let mut dash = match self.pen.dash {
            Dash::Solid => 0,
            Dash::Dashed => 1,
            Dash::Dotted => 2,
        };
        if widgets::segmented(
            ui,
            p,
            &mut dash,
            &[
                (tr!("combat-editor-solid"), false),
                (tr!("combat-editor-dashed"), false),
                (tr!("combat-editor-dotted"), false),
            ],
        ) {
            self.pen.dash = [Dash::Solid, Dash::Dashed, Dash::Dotted][dash.min(2)];
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("combat-editor-presets")).size(11.5).color(p.muted));
        ui.horizontal_wrapped(|ui| {
            for preset in presets() {
                if widgets::action_button(ui, p, preset.icon, preset.name, widgets::ActionKind::Flat, vec2(0.0, 28.0)).clicked() {
                    self.pen = Pen {
                        color: preset.color,
                        fill: preset.fill,
                        width: preset.width,
                        dash: preset.dash,
                        icon: PIN_ICONS.iter().find(|i| **i == preset.icon).copied().unwrap_or(PIN_ICONS[0]),
                    };
                    self.set_tool(preset.tool);
                }
            }
        });
        let _ = cfg;
    }

    /// Every marker, front to back, under the layer it belongs to: eye and
    /// lock on each, a click to select, a double click to rename, arrows to
    /// bring forward or send backward.
    fn layers_panel(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) {
        cards::muted(ui, p, tr!("combat-editor-layers-help"));
        ui.add_space(4.0);
        let mut select: Option<(Uuid, bool)> = None;
        let mut order: Option<(Uuid, bool)> = None;
        let mut remove = None;
        let layers = cfg.session.layers.len();
        let counts: Vec<usize> = (0..layers)
            .map(|i| {
                cfg.markers
                    .iter()
                    .filter(|m| usize::from(m.style.layer).min(layers - 1) == i)
                    .count()
            })
            .collect();
        let only_one = selected(cfg).len() == 1;
        for li in (0..layers).rev() {
            let open = !self.collapsed.contains(&li);
            ui.horizontal(|ui| {
                if widgets::icon_button(ui, p, if open { "caret-down-filled" } else { "caret-right-filled" }, "", true) {
                    if open {
                        self.collapsed.insert(li);
                    } else {
                        self.collapsed.remove(&li);
                    }
                }
                let layer = &mut cfg.session.layers[li];
                if widgets::icon_toggle(
                    ui,
                    p,
                    if layer.visible { "eye-filled" } else { "eye-off" },
                    tr!("combat-editor-layer-visible"),
                    layer.visible,
                ) {
                    layer.visible = !layer.visible;
                }
                if widgets::icon_toggle(
                    ui,
                    p,
                    if layer.locked { "lock-filled" } else { "lock-open" },
                    tr!("combat-editor-layer-locked"),
                    layer.locked,
                ) {
                    layer.locked = !layer.locked;
                }
                let active = self.layer == li;
                if widgets::icon_button(
                    ui,
                    p,
                    if active { "pencil-filled" } else { "pencil-off" },
                    tr!("combat-editor-layer-active"),
                    true,
                ) {
                    self.layer = li;
                }
                ui.add(egui::TextEdit::singleline(&mut layer.name).char_limit(24).desired_width(100.0));
                cards::badge(ui, p, &counts[li].to_string(), p.violet);
                if layers > 1 && widgets::icon_button(ui, p, "trash-filled", tr!("tactical-remove"), true) {
                    remove = Some(li);
                }
            });
            if !open {
                continue;
            }
            let rows: Vec<usize> = (0..cfg.markers.len())
                .rev()
                .filter(|i| usize::from(cfg.markers[*i].style.layer).min(layers - 1) == li)
                .collect();
            for i in rows {
                self.marker_row(ui, p, cfg, i, only_one, &mut select, &mut order);
            }
            ui.add_space(2.0);
        }
        if let Some((id, shift)) = select {
            if shift {
                if let Some(m) = cfg.markers.iter_mut().find(|m| m.id == id) {
                    m.selected = !m.selected;
                }
            } else {
                select_only(cfg, id);
            }
        }
        if let Some((id, up)) = order {
            move_in_layer(cfg, id, up);
        }
        if let Some(index) = remove {
            cfg.session.layers.remove(index);
            self.collapsed.clear();
            for m in &mut cfg.markers {
                let layer = usize::from(m.style.layer);
                m.style.layer = if layer == index {
                    0
                } else if layer > index {
                    (layer - 1) as u8
                } else {
                    m.style.layer
                };
            }
        }
        ui.add_space(6.0);
        if cfg.session.layers.len() < MAX_LAYERS
            && widgets::action_button(
                ui,
                p,
                "plus",
                tr!("combat-editor-layer-add"),
                widgets::ActionKind::Flat,
                vec2(0.0, 28.0),
            )
            .clicked()
        {
            cfg.session.layers.push(Layer {
                name: format!("{} {}", tr!("combat-editor-layer"), cfg.session.layers.len() + 1),
                ..Layer::default()
            });
        }
    }

    /// One marker of the layer list.
    #[allow(clippy::too_many_arguments)]
    fn marker_row(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        cfg: &mut Settings,
        index: usize,
        only_one: bool,
        select: &mut Option<(Uuid, bool)>,
        order: &mut Option<(Uuid, bool)>,
    ) {
        let m = &mut cfg.markers[index];
        let (id, selected) = (m.id, m.selected);
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        let radius = ui.visuals().window_corner_radius;
        if selected {
            ui.painter().rect_filled(rect, radius, p.violet.gamma_multiply(0.35));
        } else if response.hovered() {
            ui.painter().rect_filled(rect, radius, p.raised);
        }
        if response.hovered() {
            self.row_hover = Some(id);
        }
        let buttons = if selected && only_one { 46.0 } else { 0.0 };
        let left = egui::Rect::from_min_max(rect.min + vec2(4.0, 2.0), pos2(rect.max.x - buttons - 2.0, rect.max.y - 2.0));
        let mut inner = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(left)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui_in = &mut inner;
        if widgets::icon_toggle(
            ui_in,
            p,
            if m.style.hidden { "eye-off" } else { "eye-filled" },
            tr!("combat-editor-layer-visible"),
            !m.style.hidden,
        ) {
            m.style.hidden = !m.style.hidden;
        }
        if widgets::icon_toggle(
            ui_in,
            p,
            if m.style.locked { "lock-filled" } else { "lock-open" },
            tr!("combat-editor-layer-locked"),
            m.style.locked,
        ) {
            m.style.locked = !m.style.locked;
        }
        let (chip, _) = ui_in.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
        ui_in
            .painter()
            .rect_filled(chip, ui_in.visuals().window_corner_radius, crate::combat::tactical::color(m.color));
        if let Some(icon) = super::icons::global(shape_icon(m.shape())) {
            ui_in.add(egui::Image::new((icon.id(), Vec2::splat(14.0))).tint(p.muted));
        }
        if self.renaming == Some(id) {
            let field = ui_in.add(
                egui::TextEdit::singleline(&mut m.name)
                    .char_limit(128)
                    .hint_text(tr!("combat-editor-name-hint"))
                    .desired_width(ui_in.available_width()),
            );
            if self.focus_rename {
                field.request_focus();
                self.focus_rename = false;
            } else if field.lost_focus() {
                self.renaming = None;
            }
        } else {
            let (text, color) = if m.name.is_empty() {
                (m.shape().label(), p.muted_dim)
            } else {
                (m.name.as_str(), p.ink)
            };
            ui_in.add(egui::Label::new(egui::RichText::new(text).color(color)).truncate());
        }
        if buttons > 0.0 {
            let right = egui::Rect::from_min_max(
                pos2(rect.max.x - buttons, rect.min.y + 2.0),
                pos2(rect.max.x - 2.0, rect.max.y - 2.0),
            );
            let mut side = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(right)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if widgets::icon_button(&mut side, p, "caret-down-filled", tr!("combat-editor-row-down"), true) {
                *order = Some((id, false));
            }
            if widgets::icon_button(&mut side, p, "caret-up-filled", tr!("combat-editor-row-up"), true) {
                *order = Some((id, true));
            }
        }
        if response.double_clicked() {
            self.renaming = Some(id);
            self.focus_rename = true;
        } else if response.clicked() {
            *select = Some((id, ui.input(|i| i.modifiers.shift)));
        }
    }

    /// What lies under the markers: presets, then each piece on its own.
    fn map_panel(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) {
        ui.label(egui::RichText::new(tr!("combat-map-title")).strong().size(14.0).color(p.ink));
        cards::muted(ui, p, tr!("combat-map-help"));
        ui.add_space(6.0);
        let mut preset = cfg.map_view.preset_of().unwrap_or(usize::MAX);
        if widgets::segmented(
            ui,
            p,
            &mut preset,
            &[
                (tr!("combat-map-preset-tactical"), false),
                (tr!("combat-map-preset-detailed"), false),
                (tr!("combat-map-preset-clean"), false),
            ],
        ) {
            cfg.map_view = MapView::preset(preset, cfg.map_view.grid);
        }
        ui.add_space(8.0);
        let view = &mut cfg.map_view;
        let mut flat = usize::from(view.ground == Ground::Flat);
        ui.label(egui::RichText::new(tr!("combat-map-ground")).color(p.ink));
        if widgets::segmented(
            ui,
            p,
            &mut flat,
            &[(tr!("combat-map-ground-shaded"), false), (tr!("combat-map-ground-flat"), false)],
        ) {
            view.ground = if flat == 1 { Ground::Flat } else { Ground::Shaded };
        }
        ui.add_space(4.0);
        let mut percent = (view.dim * 100.0).round() as u32;
        if cards::slider_row(ui, tr!("combat-map-dim"), tr!("combat-map-dim-help"), &mut percent, 20..=100, "%") {
            view.dim = percent as f32 / 100.0;
        }
        cards::switch_row(
            ui,
            p,
            &mut view.objects,
            tr!("combat-editor-objects"),
            tr!("combat-map-objects-help"),
        );
        cards::switch_row(
            ui,
            p,
            &mut view.parcels,
            tr!("combat-editor-parcels"),
            tr!("combat-map-parcels-help"),
        );
        cards::switch_row(ui, p, &mut view.grid, tr!("combat-editor-grid"), "");
    }

    fn set_panel(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings) {
        crate::combat::tactical::sets_panel(ui, p, cfg);
        ui.add_space(8.0);
        cards::muted(ui, p, tr!("combat-editor-help"));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if widgets::action_button(
                ui,
                p,
                "layers-intersect",
                tr!("combat-editor-select-all"),
                widgets::ActionKind::Flat,
                vec2(0.0, 28.0),
            )
            .clicked()
            {
                for m in &mut cfg.markers {
                    m.selected = true;
                }
            }
            if widgets::action_button_if(
                ui,
                p,
                "trash-filled",
                tr!("combat-editor-clear"),
                widgets::ActionKind::Danger,
                vec2(0.0, 28.0),
                !cfg.markers.is_empty(),
            )
            .clicked()
            {
                cfg.markers.clear();
            }
        });
        ui.add_space(10.0);
        // the picture
        ui.label(
            egui::RichText::new(tr!("combat-editor-export-title"))
                .strong()
                .size(14.0)
                .color(p.ink),
        );
        cards::muted(ui, p, tr!("combat-editor-export-help"));
        ui.add_space(4.0);
        widgets::segmented(ui, p, &mut self.export_size, &[("1024", false), ("2048", false), ("4096", false)]);
        cards::switch_row(ui, p, &mut self.export_title, tr!("combat-editor-export-with-title"), "");
        cards::switch_row(ui, p, &mut self.export_legend, tr!("combat-editor-export-with-legend"), "");
        cards::switch_row(
            ui,
            p,
            &mut self.export_transparent,
            tr!("combat-editor-export-transparent"),
            tr!("combat-editor-export-transparent-help"),
        );
        if widgets::action_button_if(
            ui,
            p,
            "photo-filled",
            tr!("combat-editor-export-image"),
            widgets::ActionKind::Primary,
            vec2(0.0, 28.0),
            true,
        )
        .clicked()
        {
            self.export_job = Some(ExportJob {
                side: [1024, 2048, 4096][self.export_size.min(2)],
                title: self.export_title,
                legend: self.export_legend,
                transparent: self.export_transparent,
            });
        }
        ui.add_space(10.0);
        // the file
        ui.label(
            egui::RichText::new(tr!("combat-editor-file-title"))
                .strong()
                .size(14.0)
                .color(p.ink),
        );
        cards::muted(ui, p, tr!("combat-editor-file-help"));
        ui.add_space(4.0);
        cards::switch_row(
            ui,
            p,
            &mut self.file_with_view,
            tr!("combat-editor-file-with-view"),
            tr!("combat-editor-file-with-view-help"),
        );
        ui.horizontal_wrapped(|ui| {
            if widgets::action_button(
                ui,
                p,
                "file-export",
                tr!("combat-editor-export-set"),
                widgets::ActionKind::Flat,
                vec2(0.0, 28.0),
            )
            .clicked()
            {
                self.file_note = Some(export_set_file(cfg, self.file_with_view));
            }
            if widgets::action_button(
                ui,
                p,
                "file-import",
                tr!("combat-editor-import-set"),
                widgets::ActionKind::Flat,
                vec2(0.0, 28.0),
            )
            .clicked()
            {
                self.file_note = import_set_file(cfg);
            }
        });
        if let Some((text, ok)) = &self.file_note {
            ui.add_space(4.0);
            ui.add(egui::Label::new(egui::RichText::new(text).color(if *ok { p.success } else { p.warn })).wrap());
        }
    }
}

/// Writes the active set to a file the person picks.
fn export_set_file(cfg: &Settings, with_view: bool) -> (String, bool) {
    let name = cfg.active_set_name();
    let mut file = crate::combat::sets::SetFile::new(name.clone(), cfg.session.layers.clone(), cfg.markers.clone());
    if with_view {
        file.view = Some(cfg.map_view);
    }
    let Ok(bytes) = file.to_file_bytes() else {
        return (tr!("combat-editor-file-failed").to_owned(), false);
    };
    let stem = if name.is_empty() {
        tr!("tactical-set-default").to_owned()
    } else {
        name
    };
    let stem: String = stem
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect();
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr!("combat-editor-export-set"))
        .set_file_name(format!("{stem}.json"))
        .add_filter("Aurora marker set", &["json"])
        .save_file()
    else {
        return (String::new(), true);
    };
    match std::fs::write(&path, bytes) {
        Ok(()) => (tr!("combat-editor-file-saved", name = path.display().to_string()).to_owned(), true),
        Err(_) => (tr!("combat-editor-file-failed").to_owned(), false),
    }
}

/// Reads a set from a file the person picks, as a new set of this region.
fn import_set_file(cfg: &mut Settings) -> Option<(String, bool)> {
    let path = rfd::FileDialog::new()
        .set_title(tr!("combat-editor-import-set"))
        .add_filter("Aurora marker set", &["json"])
        .pick_file()?;
    let read = std::fs::metadata(&path)
        .ok()
        .filter(|m| m.len() <= 1_000_000)
        .and_then(|_| std::fs::read(&path).ok())
        .and_then(|bytes| crate::combat::sets::SetFile::from_bytes(&bytes));
    Some(match read {
        Some(file) => {
            let name = file.name.clone();
            if cfg.import_set(file) {
                (tr!("combat-editor-file-imported", name = name).to_owned(), true)
            } else {
                (tr!("tactical-set-region-required").to_owned(), false)
            }
        }
        None => (tr!("combat-editor-file-unreadable").to_owned(), false),
    })
}

/// A label and a number to drag, in metres.
fn drag_row(ui: &mut egui::Ui, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str) -> bool {
    cards::field_row(ui, label, "", |ui| {
        ui.add(egui::DragValue::new(value).range(range).suffix(suffix).speed(0.1)).changed()
    })
}

fn editable_ref(layers: &[Layer], m: &Marker) -> bool {
    layers.get(usize::from(m.style.layer)).is_none_or(|l| l.visible && !l.locked) && !m.style.hidden && !m.style.locked
}

/// The icon of a marker's shape, for the layer list.
pub(crate) fn shape_icon(shape: Shape) -> &'static str {
    match shape {
        Shape::Pin => "map-pin-filled",
        Shape::Circle | Shape::Point => "circle-filled",
        Shape::Square | Shape::Rectangle => "aspect-ratio-filled",
        Shape::Polygon => "polygon",
        Shape::Arrow => "arrow-up-right",
        Shape::Route => "route",
        Shape::Line => "minus",
        Shape::Freehand => "pencil-filled",
        Shape::Text => "typography",
    }
}

/// Moves a marker one step toward the front (`up`) or the back among those
/// of its own layer; false when it is already there.
fn move_in_layer(cfg: &mut Settings, id: Uuid, up: bool) -> bool {
    let Some(i) = cfg.markers.iter().position(|m| m.id == id) else {
        return false;
    };
    let layer = cfg.markers[i].style.layer;
    let neighbour = if up {
        (i + 1..cfg.markers.len()).find(|j| cfg.markers[*j].style.layer == layer)
    } else {
        (0..i).rev().find(|j| cfg.markers[*j].style.layer == layer)
    };
    match neighbour {
        Some(j) => {
            cfg.markers.swap(i, j);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_run_through_the_alphabet_then_numbers() {
        let mut cfg = Settings::default();
        assert_eq!(next_badge(&cfg), "A");
        for badge in ["A", "B", "D"] {
            cfg.markers.push(Marker {
                shape: Shape::Pin,
                style: Style {
                    badge: badge.into(),
                    ..Style::default()
                },
                ..Marker::default()
            });
        }
        assert_eq!(next_badge(&cfg), "C");
    }

    #[test]
    fn a_stroke_loses_its_collinear_points_but_keeps_its_corners() {
        let line: Vec<[f64; 3]> = (0..10).map(|i| [f64::from(i), 0.0, 0.0]).collect();
        assert_eq!(simplify(&line, 0.1).len(), 2);
        let bend: Vec<[f64; 3]> = (0..=10).map(|i| [f64::from(i.min(5)), f64::from((i - 5).max(0)), 0.0]).collect();
        assert_eq!(simplify(&bend, 0.1).len(), 3);
    }

    #[test]
    fn the_scale_bar_takes_a_round_length() {
        assert_eq!(nice_length(37.0), 25.0);
        assert_eq!(nice_length(0.4), 1.0);
        assert_eq!(nice_length(900.0), 250.0);
    }

    #[test]
    fn the_map_projects_region_metres_both_ways() {
        let map = Map {
            rect: Rect::from_min_size(pos2(40.0, 50.0), vec2(1024.0, 512.0)),
            origin: [1_000_000.0, 2_000_000.0],
            size: vec2(512.0, 256.0),
        };
        let point = [1_000_128.0, 2_000_064.0, 32.0];
        assert_eq!(map.global(map.screen(point), 32.0), point);
        assert_eq!(map.screen([1_000_000.0, 2_000_256.0, 0.0]), map.rect.left_top());
        assert!((map.ppm() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn undo_and_redo_step_through_edits_and_a_new_edit_ends_the_redo() {
        let mut cfg = Settings::default();
        cfg.add_marker([1.0, 2.0, 3.0]);
        let one = cfg.snapshot();
        cfg.push_undo(Snapshot {
            markers: Vec::new(),
            layers: cfg.session.layers.clone(),
        });
        cfg.add_marker([4.0, 5.0, 6.0]);
        cfg.push_undo(one);
        assert!(cfg.undo());
        assert_eq!(cfg.markers.len(), 1);
        assert!(cfg.redo());
        assert_eq!(cfg.markers.len(), 2);
        assert!(cfg.undo());
        cfg.push_undo(cfg.snapshot());
        assert!(!cfg.redo(), "a new change drops the redo");
    }

    fn map() -> Map {
        Map {
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(512.0, 512.0)),
            origin: [0.0, 0.0],
            size: vec2(256.0, 256.0),
        }
    }

    fn zone(shape: Shape, name: &str) -> Marker {
        Marker {
            id: Uuid::new_v4(),
            name: name.into(),
            shape,
            ..Marker::default()
        }
    }

    #[test]
    fn clicked_corners_make_a_polygon_on_the_active_layer_and_a_route_needs_two_points() {
        let mut cfg = Settings::default();
        let mut editor = Editor {
            layer: 1,
            tool: Tool::Polygon,
            draft: vec![[10.0, 10.0, 0.0], [40.0, 10.0, 0.0], [40.0, 40.0, 0.0], [10.0, 40.0, 0.0]],
            ..Editor::default()
        };
        editor.finish_draft(&mut cfg);
        assert_eq!(cfg.markers.len(), 1);
        let m = &cfg.markers[0];
        assert_eq!((m.shape, m.path.len(), m.style.layer, m.selected), (Shape::Polygon, 4, 1, true));
        assert!(m.valid() && editor.draft.is_empty());
        editor.tool = Tool::Route;
        editor.draft = vec![[1.0, 1.0, 0.0]];
        editor.finish_draft(&mut cfg);
        assert_eq!(cfg.markers.len(), 1, "one point is not a route");
        editor.draft = vec![[1.0, 1.0, 0.0], [9.0, 9.0, 0.0]];
        editor.finish_draft(&mut cfg);
        assert_eq!(cfg.markers.last().map(|m| m.shape), Some(Shape::Route));
    }

    #[test]
    fn a_dragged_shape_becomes_a_marker_and_shift_makes_a_square() {
        let mut cfg = Settings::default();
        let mut editor = Editor {
            tool: Tool::Rect,
            ..Editor::default()
        };
        editor.finish_drag_shape(&mut cfg, [0.0, 0.0, 0.0], [30.0, 10.0, 0.0], false);
        editor.finish_drag_shape(&mut cfg, [0.0, 0.0, 0.0], [30.0, 10.0, 0.0], true);
        assert_eq!(cfg.markers[0].shape, Shape::Rectangle);
        assert_eq!(cfg.markers[1].shape, Shape::Square);
        editor.tool = Tool::Circle;
        editor.finish_drag_shape(&mut cfg, [50.0, 50.0, 0.0], [50.0, 50.2, 0.0], false);
        assert_eq!(cfg.markers.len(), 2, "a speck of a circle is not made");
        editor.finish_drag_shape(&mut cfg, [50.0, 50.0, 0.0], [60.0, 50.0, 0.0], false);
        assert!((cfg.markers[2].radius - 10.0).abs() < 1e-4);
        editor.tool = Tool::Freehand;
        editor.draft = (0..50).map(|i| [f64::from(i), 0.0, 0.0]).collect();
        editor.finish_drag_shape(&mut cfg, [0.0, 0.0, 0.0], [49.0, 0.0, 0.0], false);
        assert_eq!(cfg.markers[3].path.len(), 2, "a straight stroke keeps its two ends");
    }

    #[test]
    fn a_press_picks_the_small_zone_toggles_with_shift_and_a_miss_clears_and_starts_a_band() {
        let mut cfg = Settings::default();
        let map = map();
        let mut big = zone(Shape::Circle, "big");
        big.position = [100.0, 100.0, 0.0];
        big.radius = 60.0;
        let mut small = zone(Shape::Circle, "small");
        small.position = [100.0, 100.0, 0.0];
        small.radius = 10.0;
        let (big_id, small_id) = (big.id, small.id);
        cfg.markers = vec![big, small];
        let mut editor = Editor::default();
        let centre = map.at(100.0, 100.0);
        editor.select_press(&mut cfg, &map, centre, false, false, 0.0);
        assert!(cfg.markers.iter().find(|m| m.id == small_id).is_some_and(|m| m.selected));
        assert!(matches!(editor.drag, Drag::Move { .. }));
        let inside_big = map.at(140.0, 100.0);
        editor.select_press(&mut cfg, &map, inside_big, true, false, 0.0);
        assert_eq!(selected(&cfg).len(), 2, "shift adds the big one");
        editor.select_press(&mut cfg, &map, inside_big, true, false, 0.0);
        assert!(
            cfg.markers.iter().find(|m| m.id == big_id).is_some_and(|m| !m.selected),
            "shift again takes it out"
        );
        editor.select_press(&mut cfg, &map, map.at(250.0, 250.0), false, false, 0.0);
        assert!(selected(&cfg).is_empty());
        assert!(matches!(editor.drag, Drag::Band { .. }));
    }

    #[test]
    fn a_locked_or_hidden_layer_cannot_be_picked() {
        let mut cfg = Settings::default();
        let map = map();
        let mut zone = zone(Shape::Circle, "z");
        zone.position = [100.0, 100.0, 0.0];
        zone.radius = 10.0;
        cfg.markers = vec![zone];
        let mut editor = Editor::default();
        cfg.session.layers[0].locked = true;
        editor.select_press(&mut cfg, &map, map.at(100.0, 100.0), false, false, 0.0);
        assert!(selected(&cfg).is_empty());
        cfg.session.layers[0].locked = false;
        cfg.session.layers[0].visible = false;
        editor.select_press(&mut cfg, &map, map.at(100.0, 100.0), false, false, 0.0);
        assert!(selected(&cfg).is_empty());
    }

    #[test]
    fn a_handle_press_reshapes_edges_add_a_vertex_and_alt_removes_one() {
        let mut cfg = Settings::default();
        let map = map();
        let mut poly = zone(Shape::Polygon, "p");
        poly.path = vec![[50.0, 50.0, 0.0], [150.0, 50.0, 0.0], [150.0, 150.0, 0.0]];
        poly.position = poly.path[0];
        poly.selected = true;
        cfg.markers = vec![poly];
        let mut editor = Editor::default();
        // the middle of the first edge: a vertex is added and dragged
        editor.select_press(&mut cfg, &map, map.at(100.0, 50.0), false, false, 0.0);
        assert_eq!(cfg.markers[0].path.len(), 4);
        assert!(matches!(
            editor.drag,
            Drag::Handle {
                handle: Handle::Vertex(1),
                ..
            }
        ));
        // alt on a vertex of a triangle removes nothing, but of a quad it does
        editor.drag = Drag::None;
        editor.select_press(&mut cfg, &map, map.at(150.0, 150.0), false, true, 0.0);
        assert_eq!(cfg.markers[0].path.len(), 3);
        editor.select_press(&mut cfg, &map, map.at(150.0, 150.0), false, true, 0.0);
        assert_eq!(cfg.markers[0].path.len(), 3, "a polygon keeps three corners");
    }

    #[test]
    fn pasting_makes_new_identities_next_to_the_originals_and_stops_at_the_cap() {
        let mut cfg = Settings::default();
        let mut editor = Editor::default();
        let mut a = zone(Shape::Circle, "a");
        a.position = [10.0, 10.0, 0.0];
        a.radius = 5.0;
        a.selected = true;
        cfg.markers = vec![a.clone()];
        editor.duplicate(&mut cfg, None);
        assert_eq!(cfg.markers.len(), 2);
        assert_ne!(cfg.markers[0].id, cfg.markers[1].id);
        assert_eq!(selected(&cfg), vec![1], "the copy is the selection");
        assert!((cfg.markers[1].position[0] - 10.0 - editor.spacing).abs() < 1e-9);
        cfg.markers = (0..256).map(|_| zone(Shape::Pin, "pin")).collect();
        editor.paste(&mut cfg, vec![a], None);
        assert_eq!(cfg.markers.len(), 256);
    }

    #[test]
    fn removing_a_layer_moves_its_markers_to_the_first_and_shifts_the_rest() {
        let mut cfg = Settings::default();
        for layer in [0u8, 1, 2] {
            let mut m = zone(Shape::Pin, "p");
            m.style.layer = layer;
            cfg.markers.push(m);
        }
        // what the layers tab does for a removed layer 1
        let index = 1usize;
        cfg.session.layers.remove(index);
        for m in &mut cfg.markers {
            let layer = usize::from(m.style.layer);
            m.style.layer = if layer == index {
                0
            } else if layer > index {
                (layer - 1) as u8
            } else {
                m.style.layer
            };
        }
        assert_eq!(cfg.markers.iter().map(|m| m.style.layer).collect::<Vec<_>>(), vec![0, 0, 1]);
    }

    #[test]
    fn a_click_with_the_circle_tool_places_a_nameless_point_and_a_drag_a_circle() {
        let mut cfg = Settings::default();
        let mut editor = Editor {
            tool: Tool::Circle,
            ..Editor::default()
        };
        editor.place_point(&mut cfg, [40.0, 50.0, 3.0]);
        assert_eq!(cfg.markers.len(), 1);
        let point = &cfg.markers[0];
        assert_eq!(
            (point.shape(), point.radius, point.selected, point.name.is_empty()),
            (Shape::Point, 0.0, true, true)
        );
        assert!(point.valid());
        editor.finish_drag_shape(&mut cfg, [10.0, 10.0, 0.0], [20.0, 10.0, 0.0], false);
        assert_eq!(cfg.markers[1].shape(), Shape::Circle);
        assert!(cfg.markers[1].name.is_empty(), "no default name");
    }

    #[test]
    fn every_tool_makes_unnamed_markers() {
        let mut cfg = Settings::default();
        let mut editor = Editor {
            tool: Tool::Polygon,
            draft: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]],
            ..Editor::default()
        };
        editor.finish_draft(&mut cfg);
        editor.tool = Tool::Rect;
        editor.finish_drag_shape(&mut cfg, [0.0, 0.0, 0.0], [20.0, 10.0, 0.0], false);
        editor.tool = Tool::Arrow;
        editor.finish_drag_shape(&mut cfg, [0.0, 0.0, 0.0], [20.0, 10.0, 0.0], false);
        assert_eq!(cfg.markers.len(), 3);
        assert!(cfg.markers.iter().all(|m| m.name.is_empty()));
    }

    #[test]
    fn the_name_of_a_selected_marker_is_dragged_but_an_unselected_one_is_not() {
        let mut cfg = Settings::default();
        let map = map();
        let mut zone = zone(Shape::Circle, "Bridge");
        zone.position = [100.0, 100.0, 0.0];
        zone.radius = 10.0;
        cfg.markers = vec![zone.clone()];
        let spot = annotate::label_spot(&map, &zone, 8.0).expect("spot").0;
        let mut editor = Editor::default();
        // not selected: the press goes to the marker (the name is over its disc)
        editor.select_press(&mut cfg, &map, spot, false, false, 0.0);
        assert!(!matches!(editor.drag, Drag::Label { .. }));
        editor.drag = Drag::None;
        cfg.markers[0].selected = true;
        editor.select_press(&mut cfg, &map, spot, false, false, 0.0);
        assert!(matches!(editor.drag, Drag::Label { .. }));
    }

    #[test]
    fn a_marker_steps_forward_and_back_among_its_own_layer_only() {
        let mut cfg = Settings::default();
        let make = |layer: u8| {
            let mut m = zone(Shape::Pin, "p");
            m.style.layer = layer;
            m
        };
        cfg.markers = vec![make(0), make(1), make(0), make(1)];
        let ids: Vec<Uuid> = cfg.markers.iter().map(|m| m.id).collect();
        assert!(move_in_layer(&mut cfg, ids[0], true));
        assert_eq!(
            cfg.markers.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![ids[2], ids[1], ids[0], ids[3]]
        );
        assert!(!move_in_layer(&mut cfg, ids[0], true), "already in front of its layer");
        assert!(move_in_layer(&mut cfg, ids[3], false));
        assert!(!move_in_layer(&mut cfg, ids[2], false), "already at the back of its layer");
    }

    #[test]
    fn a_hidden_or_locked_marker_cannot_be_picked_or_deleted() {
        let mut cfg = Settings::default();
        let map = map();
        let mut zone = zone(Shape::Circle, "z");
        zone.position = [100.0, 100.0, 0.0];
        zone.radius = 10.0;
        zone.style.locked = true;
        cfg.markers = vec![zone];
        let mut editor = Editor::default();
        editor.select_press(&mut cfg, &map, map.at(100.0, 100.0), false, false, 0.0);
        assert!(selected(&cfg).is_empty(), "a locked marker is not picked");
        cfg.markers[0].style.locked = false;
        cfg.markers[0].style.hidden = true;
        editor.select_press(&mut cfg, &map, map.at(100.0, 100.0), false, false, 0.0);
        assert!(selected(&cfg).is_empty(), "a hidden marker is not picked");
        assert!(!editable_ref(&cfg.session.layers, &cfg.markers[0]));
    }
}
