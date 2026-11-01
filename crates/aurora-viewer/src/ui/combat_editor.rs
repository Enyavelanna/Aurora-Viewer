//! Region annotation canvas. Edits the same working set as the minimap;
//! undo snapshots stay local to this editor and never cross set identities.
use super::widgets;
use crate::{
    combat::tactical::{Marker, Settings, Shape},
    theme::Palette,
    world::World,
};
use aurora_i18n::tr;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct Editor {
    pub open: bool,
    tool: Option<Shape>,
    selected: Option<Uuid>,
    identity: Option<(crate::combat::sets::RegionKey, Option<Uuid>)>,
    start: Option<[f64; 3]>,
    before: Option<Vec<Marker>>,
    undo: Vec<Vec<Marker>>,
    zoom: f32,
    pan: Vec2,
    pub images: Vec<super::minimap::RegionImage>,
    grid: bool,
    snap: bool,
    spacing: f64,
    erase: bool,
    path: Vec<[f64; 3]>,
    lock_sides: bool,
    resize: bool,
}

struct Map {
    rect: Rect,
    origin: [f64; 2],
    size: Vec2,
}
impl Map {
    fn screen(&self, p: [f64; 3]) -> Pos2 {
        pos2(
            self.rect.left() + ((p[0] - self.origin[0]) / self.size.x as f64) as f32 * self.rect.width(),
            self.rect.bottom() - ((p[1] - self.origin[1]) / self.size.y as f64) as f32 * self.rect.height(),
        )
    }
    fn global(&self, p: Pos2, z: f64) -> [f64; 3] {
        [
            self.origin[0] + ((p.x - self.rect.left()) / self.rect.width()) as f64 * self.size.x as f64,
            self.origin[1] + ((self.rect.bottom() - p.y) / self.rect.height()) as f64 * self.size.y as f64,
            z,
        ]
    }
}

impl Editor {
    fn remember(&mut self, markers: Vec<Marker>) {
        if self.undo.len() == 32 {
            self.undo.remove(0);
        }
        self.undo.push(markers);
    }

    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, cfg: &mut Settings, world: &World) {
        if !self.open {
            return;
        }
        let id = cfg.current_region.clone().map(|key| {
            let active = cfg.library.regions.iter().find(|r| r.key == key).and_then(|r| r.active);
            (key, active)
        });
        if self.identity != id {
            self.identity = id;
            self.undo.clear();
            self.before = None;
            self.start = None;
            self.selected = None;
            self.pan = Vec2::ZERO;
            self.zoom = 1.0;
        }
        let mut open = self.open;
        widgets::Floater::new(
            "combat_marker_editor",
            tr!("combat-editor-title"),
            pos2(40.0, 65.0),
            vec2(1120.0, 680.0),
        )
        .show(ctx, p, &mut open, |ui| {
            ui.horizontal_wrapped(|ui| {
                if widgets::action_button(
                    ui,
                    p,
                    "pointer-filled",
                    tr!("combat-editor-select"),
                    if self.tool.is_none() && !self.erase {
                        widgets::ActionKind::Primary
                    } else {
                        widgets::ActionKind::Flat
                    },
                    vec2(0.0, 28.0),
                )
                .clicked()
                {
                    self.tool = None;
                    self.erase = false;
                    self.start = None;
                }
                for (shape, icon) in [
                    (Shape::Circle, "circle-filled"),
                    (Shape::Line, "minus"),
                    (Shape::Rectangle, "aspect-ratio-filled"),
                    (Shape::Freehand, "pencil-filled"),
                ] {
                    let kind = if self.tool == Some(shape) {
                        widgets::ActionKind::Primary
                    } else {
                        widgets::ActionKind::Flat
                    };
                    if widgets::action_button(ui, p, icon, shape.label(), kind, vec2(0.0, 28.0)).clicked() {
                        self.tool = Some(shape);
                        self.erase = false;
                        self.start = None;
                    }
                }
                if widgets::action_button_if(
                    ui,
                    p,
                    "arrow-back-up",
                    tr!("combat-editor-undo"),
                    widgets::ActionKind::Flat,
                    vec2(0.0, 28.0),
                    !self.undo.is_empty(),
                )
                .clicked()
                    && let Some(markers) = self.undo.pop()
                {
                    cfg.markers = markers;
                    cfg.save_set();
                }
                if ui.button(tr!("combat-editor-fit")).clicked() {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }
                if widgets::action_button(
                    ui,
                    p,
                    "backspace-filled",
                    tr!("combat-editor-erase"),
                    if self.erase {
                        widgets::ActionKind::Primary
                    } else {
                        widgets::ActionKind::Flat
                    },
                    vec2(0.0, 28.0),
                )
                .clicked()
                {
                    self.erase = true;
                    self.tool = None;
                }
            });
            ui.horizontal_wrapped(|ui| {
                widgets::switch_labeled(ui, p, &mut self.grid, tr!("combat-editor-grid"));
                widgets::switch_labeled(ui, p, &mut self.snap, tr!("combat-editor-snap"));
                if self.spacing < 1.0 {
                    self.spacing = 16.0;
                }
                ui.add(egui::DragValue::new(&mut self.spacing).range(1.0..=64.0).suffix(" m"));
                if self.tool == Some(Shape::Rectangle) {
                    widgets::switch_labeled(ui, p, &mut self.lock_sides, tr!("combat-editor-square-lock"));
                }
            });
            ui.label(tr!("combat-editor-help"));
            widgets::divider(ui, p);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(245.0);
                    egui::ScrollArea::vertical().max_height(510.0).show(ui, |ui| {
                        crate::combat::tactical::sets_panel(ui, p, cfg);
                        widgets::divider(ui, p);
                        let property_before = cfg.markers.clone();
                        let undo_count = self.undo.len();
                        if let Some(marker) = cfg.markers.iter_mut().find(|m| Some(m.id) == self.selected) {
                            ui.label(marker.shape().label());
                            ui.add(egui::TextEdit::singleline(&mut marker.name).char_limit(128));
                            while marker.name.len() > 128 {
                                marker.name.pop();
                            }
                            widgets::color_button(ui, p, "editor-marker-color", &mut marker.color, true);
                            if matches!(marker.shape(), Shape::Circle | Shape::Square | Shape::Rectangle) {
                                widgets::switch_labeled(ui, p, &mut marker.hatched, tr!("combat-editor-hatch"));
                            }
                            geometry(ui, p, marker, &mut self.lock_sides);
                            ui.label(tr!("combat-editor-altitude"));
                            ui.add(egui::DragValue::new(&mut marker.position[2]).range(-4096.0..=65536.0));
                            if let Some((ox, oy)) = world.main_origin() {
                                for (axis, origin) in [(0, ox as f64), (1, oy as f64)] {
                                    let mut local = marker.position[axis] - origin;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut local)
                                                .prefix(if axis == 0 { "X " } else { "Y " })
                                                .range(0.0..=4096.0),
                                        )
                                        .changed()
                                    {
                                        let delta = local + origin - marker.position[axis];
                                        marker.position[axis] += delta;
                                        if let Some(end) = &mut marker.end {
                                            end[axis] += delta;
                                        }
                                        for point in &mut marker.path {
                                            point[axis] += delta;
                                        }
                                    }
                                }
                            }
                            if ui.button(tr!("tactical-remove")).clicked() {
                                self.remember(cfg.markers.clone());
                                cfg.markers.retain(|m| Some(m.id) != self.selected);
                                self.selected = None;
                            }
                        }
                        if property_before != cfg.markers && self.undo.len() == undo_count {
                            self.remember(property_before);
                        }
                        widgets::divider(ui, p);
                        for marker in &cfg.markers {
                            if ui.selectable_label(Some(marker.id) == self.selected, &marker.name).clicked() {
                                self.selected = Some(marker.id);
                                self.tool = None;
                            }
                        }
                    });
                });
                ui.vertical(|ui| {
                    self.canvas(ui, p, cfg, world);
                });
            });
        });
        if !open {
            self.start = None;
            self.before = None;
        }
        self.open = open;
        cfg.save_set();
    }

    fn canvas(&mut self, ui: &mut egui::Ui, p: &Palette, cfg: &mut Settings, world: &World) {
        let Some(region) = world.main() else {
            ui.label(tr!("tactical-set-region-required"));
            return;
        };
        let Some((ox, oy)) = world.main_origin() else {
            return;
        };
        let size = vec2(region.heightmap.size_x as f32, region.heightmap.size_y as f32);
        let (canvas, response) = ui.allocate_exact_size(
            vec2(ui.available_width().max(200.0), ui.available_height().clamp(300.0, 560.0)),
            Sense::click_and_drag(),
        );
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(1.0, 8.0);
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.pan += response.drag_delta();
        }
        let fit = (canvas.width() / size.x).min(canvas.height() / size.y) * self.zoom.max(1.0);
        let map = Map {
            rect: Rect::from_center_size(canvas.center() + self.pan, size * fit),
            origin: [ox as f64, oy as f64],
            size,
        };
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 0.0, p.field);
        for image in &self.images {
            let sw = [image.origin.0, image.origin.1, 0.0];
            let ne = [sw[0] + image.size.0, sw[1] + image.size.1, 0.0];
            painter.with_clip_rect(canvas.intersect(map.rect)).image(
                image.texture.id(),
                Rect::from_two_pos(map.screen(sw), map.screen(ne)),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                widgets::PICTURE_TINT,
            );
        }
        if self.grid {
            for n in 0..=((size.x.max(size.y) as f64 / self.spacing.max(1.0)).ceil() as usize).min(4096) {
                let x = map.rect.left() + (n as f64 * self.spacing - (ox as f64 % self.spacing)) as f32 * fit;
                let y = map.rect.bottom() - (n as f64 * self.spacing - (oy as f64 % self.spacing)) as f32 * fit;
                painter.line_segment(
                    [pos2(x, map.rect.top()), pos2(x, map.rect.bottom())],
                    Stroke::new(1.0, p.muted_dim.gamma_multiply(0.3)),
                );
                painter.line_segment(
                    [pos2(map.rect.left(), y), pos2(map.rect.right(), y)],
                    Stroke::new(1.0, p.muted_dim.gamma_multiply(0.3)),
                );
            }
        }
        painter.text(
            canvas.left_top() + vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!("{} · N ↑", region.name),
            egui::FontId::proportional(13.0),
            p.ink,
        );
        // A drag release can stop interacting before the response is inspected.
        // Keep the final cursor location so the stroke can still be committed.
        let pointer = response.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.latest_pos()));
        let z = world.agent.position.z as f64;
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if let Some(before) = self.before.take() {
                cfg.markers = before;
            }
            self.start = None;
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = ui.input(|i| i.pointer.press_origin()).filter(|pos| map.rect.contains(*pos))
        {
            self.start = Some(map.global(pos, z));
            if self.snap
                && let Some(start) = &mut self.start
            {
                snap(start, self.spacing);
            }
            self.path = self.start.into_iter().collect();
            self.before = Some(cfg.markers.clone());
            if self.tool.is_none() {
                self.selected = cfg.markers.iter().rev().find(|m| hit(&map, m, pos)).map(|m| m.id);
                self.resize = cfg
                    .markers
                    .iter()
                    .find(|m| Some(m.id) == self.selected)
                    .and_then(handle)
                    .is_some_and(|point| map.screen(point).distance(pos) < 12.0);
            }
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && self.tool == Some(Shape::Freehand)
            && self.path.len() < 128
            && let Some(pos) = pointer
        {
            let mut point = map.global(map.rect.clamp(pos), z);
            if self.snap {
                snap(&mut point, self.spacing);
            }
            if self
                .path
                .last()
                .is_none_or(|prev| map.screen(*prev).distance(map.screen(point)) >= 3.0)
            {
                self.path.push(point);
            }
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && self.tool.is_none()
            && !self.erase
            && let (Some(start), Some(before), Some(pos)) = (self.start, self.before.as_ref(), pointer)
            && let Some(marker) = cfg.markers.iter_mut().find(|m| Some(m.id) == self.selected)
            && let Some(original) = before.iter().find(|m| m.id == marker.id)
        {
            let mut now = map.global(map.rect.clamp(pos), z);
            if self.snap {
                snap(&mut now, self.spacing);
            }
            if self.resize {
                if original.shape().tool() == Shape::Circle {
                    marker.radius = ((now[0] - original.position[0]).hypot(now[1] - original.position[1]) as f32).clamp(0.0, 1024.0);
                    marker.shape = if marker.radius == 0.0 { Shape::Point } else { Shape::Circle };
                } else if let Some(resized) = drawn(
                    if original.shape().tool() == Shape::Rectangle && self.lock_sides {
                        Shape::Square
                    } else {
                        original.shape().tool()
                    },
                    original.position,
                    now,
                    original.color,
                ) {
                    marker.end = resized.end;
                    marker.shape = resized.shape;
                }
            } else {
                for i in 0..2 {
                    marker.position[i] = original.position[i] + now[i] - start[i];
                }
                marker.end = original.end.map(|mut end| {
                    for i in 0..2 {
                        end[i] += now[i] - start[i];
                    }
                    end
                });
                marker.path = original
                    .path
                    .iter()
                    .map(|point| [point[0] + now[0] - start[0], point[1] + now[1] - start[1], point[2]])
                    .collect();
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if let (Some(shape), Some(start), Some(pos)) = (self.tool, self.start, pointer)
                && cfg.markers.len() < 256
                && let Some(m) = self.draft(shape, start, map.global(map.rect.clamp(pos), z), cfg.colors[2])
            {
                self.selected = Some(m.id);
                cfg.markers.push(m);
            }
            if let Some(before) = self.before.take()
                && before != cfg.markers
            {
                self.remember(before);
            }
            self.start = None;
        }
        if response.clicked()
            && let Some(pos) = pointer.filter(|pos| map.rect.contains(*pos))
        {
            if self.erase {
                if let Some(index) = cfg.markers.iter().rposition(|m| hit(&map, m, pos)) {
                    self.remember(cfg.markers.clone());
                    cfg.markers.remove(index);
                }
            } else if self.tool == Some(Shape::Circle) && cfg.markers.len() < 256 {
                self.remember(cfg.markers.clone());
                let mut point = map.global(pos, z);
                if self.snap {
                    snap(&mut point, self.spacing);
                }
                cfg.add_marker(point);
                self.selected = cfg.markers.last().map(|m| m.id);
            } else if self.tool.is_none() {
                self.selected = cfg.markers.iter().rev().find(|m| hit(&map, m, pos)).map(|m| m.id);
            }
        }
        for marker in &cfg.markers {
            paint(&painter, &map, marker, Some(marker.id) == self.selected, p);
        }
        if let (Some(shape), Some(start), Some(pos)) = (self.tool, self.start, pointer)
            && let Some(m) = self.draft(shape, start, map.global(map.rect.clamp(pos), z), cfg.colors[2])
        {
            paint(&painter, &map, &m, true, p);
        }
    }
    fn draft(&self, shape: Shape, start: [f64; 3], mut end: [f64; 3], color: [u8; 4]) -> Option<Marker> {
        if self.snap {
            snap(&mut end, self.spacing);
        }
        if shape == Shape::Freehand {
            let marker = Marker {
                id: Uuid::new_v4(),
                name: shape.label().into(),
                position: start,
                shape,
                color,
                path: self.path.clone(),
                ..Marker::default()
            };
            return marker.valid().then_some(marker);
        }
        drawn(
            if shape == Shape::Rectangle && self.lock_sides {
                Shape::Square
            } else {
                shape
            },
            start,
            end,
            color,
        )
    }
}

fn snap(point: &mut [f64; 3], spacing: f64) {
    for coordinate in point.iter_mut().take(2) {
        *coordinate = (*coordinate / spacing.max(1.0)).round() * spacing.max(1.0);
    }
}

fn geometry(ui: &mut egui::Ui, p: &Palette, marker: &mut Marker, locked: &mut bool) {
    match marker.shape().tool() {
        Shape::Circle => {
            ui.label(tr!("combat-editor-radius"));
            if ui
                .add(egui::DragValue::new(&mut marker.radius).range(0.0..=1024.0).suffix(" m"))
                .changed()
            {
                marker.shape = if marker.radius == 0.0 { Shape::Point } else { Shape::Circle };
            }
            if marker.radius == 0.0 {
                ui.label(tr!("combat-editor-radius-zero"));
            }
        }
        Shape::Rectangle => {
            if let Some(end) = &mut marker.end {
                let delta = [end[0] - marker.position[0], end[1] - marker.position[1]];
                let mut width = delta[0].abs();
                let mut height = delta[1].abs();
                let lock_changed = widgets::switch_labeled(ui, p, locked, tr!("combat-editor-square-lock")).changed();
                ui.label(tr!("combat-editor-width"));
                let width_changed = ui.add(egui::DragValue::new(&mut width).range(0.1..=1024.0).suffix(" m")).changed();
                ui.label(tr!("combat-editor-height"));
                let height_changed = ui
                    .add_enabled(!*locked, egui::DragValue::new(&mut height).range(0.1..=1024.0).suffix(" m"))
                    .changed();
                if width_changed || height_changed || lock_changed {
                    if *locked {
                        height = width;
                    }
                    end[0] = marker.position[0] + width * if delta[0] < 0.0 { -1.0 } else { 1.0 };
                    end[1] = marker.position[1] + height * if delta[1] < 0.0 { -1.0 } else { 1.0 };
                    marker.shape = Shape::Rectangle;
                }
            }
        }
        Shape::Line => {
            if let Some(end) = &mut marker.end {
                let dx = end[0] - marker.position[0];
                let dy = end[1] - marker.position[1];
                let mut length = dx.hypot(dy);
                let mut angle = dy.atan2(dx).to_degrees();
                ui.label(tr!("combat-editor-length"));
                let changed = ui
                    .add(
                        egui::DragValue::new(&mut length)
                            .range(0.1..=1024.0)
                            .clamp_existing_to_range(false)
                            .suffix(" m"),
                    )
                    .changed();
                ui.label(tr!("combat-editor-angle"));
                if ui.add(egui::DragValue::new(&mut angle).range(-180.0..=180.0).suffix("°")).changed() || changed {
                    length = length.min(1024.0 / angle.to_radians().cos().abs().max(angle.to_radians().sin().abs()));
                    end[0] = marker.position[0] + length * angle.to_radians().cos();
                    end[1] = marker.position[1] + length * angle.to_radians().sin();
                }
            }
        }
        _ => {}
    }
}

fn drawn(shape: Shape, start: [f64; 3], end: [f64; 3], color: [u8; 4]) -> Option<Marker> {
    if shape == Shape::Point {
        return None;
    }
    if shape == Shape::Circle {
        let radius = ((end[0] - start[0]).hypot(end[1] - start[1])) as f32;
        let m = Marker {
            id: Uuid::new_v4(),
            name: shape.label().into(),
            position: start,
            radius,
            color,
            selected: false,
            shape,
            end: None,
            ..Marker::default()
        };
        return (radius > 0.1 && m.valid()).then_some(m);
    }
    Marker::drawn(shape, start, end, color)
}
fn hit(map: &Map, m: &Marker, pos: Pos2) -> bool {
    let a = map.screen(m.position);
    match m.shape() {
        Shape::Point => a.distance(pos) < 12.0,
        Shape::Circle => a.distance(pos) < (m.radius * map.rect.width() / map.size.x + 8.0).max(12.0),
        Shape::Square | Shape::Rectangle => m
            .end
            .is_some_and(|end| Rect::from_two_pos(a, map.screen(end)).expand(5.0).contains(pos)),
        Shape::Line => m.end.is_some_and(|end| {
            let b = map.screen(end);
            let delta = b - a;
            let t = ((pos - a).dot(delta) / delta.length_sq().max(0.001)).clamp(0.0, 1.0);
            pos.distance(a + delta * t) < 10.0
        }),
        Shape::Freehand => m.path.windows(2).any(|pair| {
            let a = map.screen(pair[0]);
            let b = map.screen(pair[1]);
            let d = b - a;
            let t = ((pos - a).dot(d) / d.length_sq().max(0.001)).clamp(0.0, 1.0);
            pos.distance(a + d * t) < 10.0
        }),
    }
}
fn paint(painter: &egui::Painter, map: &Map, m: &Marker, selected: bool, p: &Palette) {
    let a = map.screen(m.position);
    let color = Color32::from_rgba_unmultiplied(m.color[0], m.color[1], m.color[2], m.color[3]);
    let stroke = Stroke::new(if selected { 3.0 } else { 2.0 }, color);
    if m.hatched {
        let polygon = match m.shape() {
            Shape::Circle => (0..32)
                .map(|n| {
                    let angle = n as f32 * std::f32::consts::TAU / 32.0;
                    a + vec2(angle.cos(), angle.sin()) * m.radius * map.rect.width() / map.size.x
                })
                .collect(),
            Shape::Square | Shape::Rectangle => m
                .end
                .map(|end| {
                    let rect = Rect::from_two_pos(a, map.screen(end));
                    vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()]
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        crate::combat::tactical::hatch(painter, &polygon, color);
    }
    match m.shape() {
        Shape::Point => {
            painter.circle_filled(a, 5.0, color);
        }
        Shape::Circle => {
            painter.circle_stroke(a, m.radius * map.rect.width() / map.size.x, stroke);
        }
        Shape::Square | Shape::Rectangle => {
            if let Some(end) = m.end {
                painter.rect_stroke(Rect::from_two_pos(a, map.screen(end)), 0.0, stroke, egui::StrokeKind::Inside);
            }
        }
        Shape::Line => {
            if let Some(end) = m.end {
                painter.line_segment([a, map.screen(end)], stroke);
            }
        }
        Shape::Freehand => {
            painter.add(egui::Shape::line(m.path.iter().map(|point| map.screen(*point)).collect(), stroke));
        }
    }
    if selected {
        painter.circle_stroke(a, 8.0, Stroke::new(1.0, p.ink));
        if let Some(point) = handle(m) {
            painter.rect_filled(Rect::from_center_size(map.screen(point), Vec2::splat(9.0)), 0.0, p.ink);
        }
    }
    painter.text(
        a + vec2(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        &m.name,
        egui::FontId::proportional(12.0),
        color,
    );
}

fn handle(marker: &Marker) -> Option<[f64; 3]> {
    match marker.shape().tool() {
        Shape::Circle if marker.radius > 0.0 => Some([marker.position[0] + marker.radius as f64, marker.position[1], marker.position[2]]),
        Shape::Rectangle | Shape::Line => marker.end,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combat_editor_projection_and_shape_picking_follow_region_coordinates() {
        let map = Map {
            rect: Rect::from_min_size(pos2(40.0, 50.0), vec2(1024.0, 512.0)),
            origin: [1_000_000.0, 2_000_000.0],
            size: vec2(512.0, 256.0),
        };
        let point = [1_000_128.0, 2_000_064.0, 32.0];
        assert_eq!(map.global(map.screen(point), 32.0), point);
        assert_eq!(map.screen([1_000_000.0, 2_000_256.0, 0.0]), map.rect.left_top());
        let end = [point[0] + 60.0, point[1] + 40.0, 32.0];
        let line = drawn(Shape::Line, point, end, [255; 4]).expect("line");
        assert!(hit(&map, &line, map.screen([point[0] + 30.0, point[1] + 20.0, 32.0])));
        assert!(!hit(&map, &line, map.screen([point[0] + 30.0, point[1] + 50.0, 32.0])));
        let circle = drawn(Shape::Circle, point, end, [255; 4]).expect("circle");
        assert!((circle.radius - 72.111).abs() < 0.01);
        assert!(drawn(Shape::Circle, point, point, [255; 4]).is_none());
        let rectangle = drawn(Shape::Rectangle, point, end, [255; 4]).expect("rectangle");
        assert_eq!(rectangle.end, Some(end));
        let freehand = Marker {
            id: Uuid::new_v4(),
            shape: Shape::Freehand,
            position: point,
            path: vec![point, end],
            hatched: true,
            ..Marker::default()
        };
        assert!(freehand.valid());
        let copy: Marker = serde_json::from_str(&serde_json::to_string(&freehand).expect("encode")).expect("decode");
        assert_eq!(copy, freehand);
        assert_eq!(Shape::Point.tool(), Shape::Circle);
        assert_eq!(Shape::Square.tool(), Shape::Rectangle);
        let mut excessive = freehand;
        excessive.path = vec![point; 129];
        assert!(!excessive.valid());
        let mut snapped = [100.5, 111.2, 32.0];
        snap(&mut snapped, 8.0);
        assert_eq!(snapped, [104.0, 112.0, 32.0]);
    }
}
