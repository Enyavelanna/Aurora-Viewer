//! Drawing, picking and geometry of annotation markers. The region editor,
//! the minimap and the image export all go through here, so a marker looks
//! and is picked the same wherever it is. Coordinates are global metres
//! (x east, y north); a `Project` turns them into screen points.
//!
//! The colours are the markers' own (content), not the theme's.
use crate::combat::tactical::{Dash, Marker, Shape, color};
use egui::{Color32, Mesh, Painter, Pos2, Rect, Stroke, pos2, vec2};

/// Global metres to screen points.
pub(crate) trait Project {
    fn at(&self, x: f64, y: f64) -> Pos2;
    /// Screen points per metre.
    fn ppm(&self) -> f32;
}

/// How a marker is drawn.
pub(crate) struct Look {
    /// Thicker, for the marker under the pointer or picked.
    pub emphasis: bool,
    /// Names are drawn (a marker can still turn its own off).
    pub labels: bool,
    /// Multiplies the marker's opacity (a hidden layer's ghost, a draft).
    pub alpha: f32,
    /// Radius of the ring of a bare point, pixels.
    pub point_size: f32,
    /// What a text without words shows (the editor's hint), dimmed; empty
    /// draws nothing.
    pub placeholder: &'static str,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            emphasis: false,
            labels: true,
            alpha: 1.0,
            point_size: 8.0,
            placeholder: "",
        }
    }
}

/// Pixels within which a line or a point is picked.
const REACH: f32 = 8.0;
/// Radius of a pin's disc, pixels.
const PIN: f32 = 11.0;

pub(crate) fn extent(points: &[[f64; 3]]) -> f64 {
    let Some((min, max)) = span(points.iter().map(|p| [p[0], p[1]])) else {
        return 0.0;
    };
    (max[0] - min[0]).max(max[1] - min[1])
}

fn span(points: impl Iterator<Item = [f64; 2]>) -> Option<([f64; 2], [f64; 2])> {
    points.fold(None, |acc, p| match acc {
        None => Some((p, p)),
        Some((a, b)) => Some(([a[0].min(p[0]), a[1].min(p[1])], [b[0].max(p[0]), b[1].max(p[1])])),
    })
}

fn rotate(v: [f64; 2], degrees: f64) -> [f64; 2] {
    let (s, c) = degrees.to_radians().sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
}

/// The centre of a rectangle, whatever its rotation.
pub(crate) fn rect_center(m: &Marker) -> [f64; 2] {
    let e = m.end.unwrap_or(m.position);
    [(m.position[0] + e[0]) * 0.5, (m.position[1] + e[1]) * 0.5]
}

/// The four corners of a rectangle, rotated about its centre: bottom left,
/// bottom right, top right, top left before the rotation.
pub(crate) fn corners(m: &Marker) -> Option<[[f64; 2]; 4]> {
    let end = m.end?;
    let c = rect_center(m);
    let hx = (end[0] - m.position[0]).abs() * 0.5;
    let hy = (end[1] - m.position[1]).abs() * 0.5;
    let angle = f64::from(m.style.angle);
    Some([[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]].map(|v| {
        let r = rotate(v, angle);
        [c[0] + r[0], c[1] + r[1]]
    }))
}

/// The closed outline of a rectangle or a polygon.
pub(crate) fn outline(m: &Marker) -> Vec<[f64; 2]> {
    match m.shape() {
        Shape::Square | Shape::Rectangle => corners(m).map(|c| c.to_vec()).unwrap_or_default(),
        Shape::Polygon => m.path.iter().map(|p| [p[0], p[1]]).collect(),
        _ => Vec::new(),
    }
}

/// The points a line-like marker runs through.
pub(crate) fn polyline(m: &Marker) -> Vec<[f64; 2]> {
    match m.shape() {
        Shape::Line | Shape::Arrow => m.end.into_iter().fold(vec![[m.position[0], m.position[1]]], |mut v, e| {
            v.push([e[0], e[1]]);
            v
        }),
        Shape::Route | Shape::Freehand => m.path.iter().map(|p| [p[0], p[1]]).collect(),
        _ => Vec::new(),
    }
}

/// The smallest box around a marker.
pub(crate) fn bounds(m: &Marker) -> Option<([f64; 2], [f64; 2])> {
    match m.shape() {
        Shape::Circle => {
            let r = f64::from(m.radius);
            Some(([m.position[0] - r, m.position[1] - r], [m.position[0] + r, m.position[1] + r]))
        }
        Shape::Square | Shape::Rectangle | Shape::Polygon => span(outline(m).into_iter()),
        Shape::Line | Shape::Arrow | Shape::Route | Shape::Freehand => span(polyline(m).into_iter()),
        Shape::Point | Shape::Pin | Shape::Text => Some(([m.position[0], m.position[1]], [m.position[0], m.position[1]])),
    }
}

/// Where the name sits.
pub(crate) fn label_anchor(m: &Marker) -> [f64; 2] {
    match m.shape() {
        Shape::Square | Shape::Rectangle => rect_center(m),
        Shape::Polygon => {
            let pts = outline(m);
            let n = pts.len().max(1) as f64;
            [pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n]
        }
        Shape::Line | Shape::Arrow => {
            let e = m.end.unwrap_or(m.position);
            [(m.position[0] + e[0]) * 0.5, (m.position[1] + e[1]) * 0.5]
        }
        Shape::Route | Shape::Freehand => m
            .path
            .get(m.path.len() / 2)
            .map_or([m.position[0], m.position[1]], |p| [p[0], p[1]]),
        _ => [m.position[0], m.position[1]],
    }
}

/// Moves a marker by metres.
pub(crate) fn translate(m: &mut Marker, dx: f64, dy: f64) {
    m.position[0] += dx;
    m.position[1] += dy;
    if let Some(end) = &mut m.end {
        end[0] += dx;
        end[1] += dy;
    }
    for p in &mut m.path {
        p[0] += dx;
        p[1] += dy;
    }
}

// ---- screen geometry -----------------------------------------------------

fn dist_segment(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_sq().max(0.001)).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

fn dist_polyline(points: &[Pos2], p: Pos2) -> f32 {
    points.windows(2).map(|w| dist_segment(w[0], w[1], p)).fold(f32::INFINITY, f32::min)
}

pub(crate) fn polygon_contains(points: &[Pos2], p: Pos2) -> bool {
    let mut inside = false;
    let mut j = points.len().wrapping_sub(1);
    for i in 0..points.len() {
        let (a, b) = (points[i], points[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn polygon_area(points: &[Pos2]) -> f32 {
    let mut sum = 0.0;
    for i in 0..points.len() {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        sum += a.x * b.y - b.x * a.y;
    }
    sum * 0.5
}

fn in_triangle(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let side = |p1: Pos2, p2: Pos2, p3: Pos2| (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y);
    let (d1, d2, d3) = (side(p, a, b), side(p, b, c), side(p, c, a));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// Triangles filling a simple polygon, by ear clipping (concave included).
pub(crate) fn triangulate(points: &[Pos2]) -> Vec<[usize; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let ccw = polygon_area(points) > 0.0;
    let mut left: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n - 2);
    let mut guard = 0;
    while left.len() > 3 && guard < n * n {
        guard += 1;
        let m = left.len();
        let ear = (0..m).find(|&i| {
            let (a, b, c) = (left[(i + m - 1) % m], left[i], left[(i + 1) % m]);
            let turn =
                (points[b].x - points[a].x) * (points[c].y - points[b].y) - (points[b].y - points[a].y) * (points[c].x - points[b].x);
            (turn > 0.0) == ccw
                && turn != 0.0
                && !left
                    .iter()
                    .any(|&j| j != a && j != b && j != c && in_triangle(points[j], points[a], points[b], points[c]))
        });
        let Some(i) = ear else { break };
        let m = left.len();
        out.push([left[(i + m - 1) % m], left[i], left[(i + 1) % m]]);
        left.remove(i);
    }
    if left.len() == 3 {
        out.push([left[0], left[1], left[2]]);
    }
    out
}

// ---- painting --------------------------------------------------------------

fn with_alpha(c: Color32, factor: f32) -> Color32 {
    c.gamma_multiply(factor.clamp(0.0, 1.0))
}

/// Black or near-white, whichever reads on `c`.
fn contrast(c: Color32) -> Color32 {
    let luma = (0.299 * f32::from(c.r()) + 0.587 * f32::from(c.g()) + 0.114 * f32::from(c.b())) / 255.0;
    if luma > 0.6 {
        Color32::from_black_alpha(235)
    } else {
        Color32::WHITE
    }
}

/// Text with a dark halo, readable on any map.
pub(crate) fn halo_text(painter: &Painter, pos: Pos2, align: egui::Align2, text: &str, size: f32, color: Color32) {
    let font = egui::FontId::proportional(size);
    let halo = Color32::from_black_alpha(210);
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0), (-1.0, -1.0)] {
        painter.text(pos + vec2(dx, dy), align, text, font.clone(), halo);
    }
    painter.text(pos, align, text, font, color);
}

fn stroke_path(painter: &Painter, points: &[Pos2], stroke: Stroke, dash: Dash) {
    if points.len() < 2 {
        return;
    }
    match dash {
        Dash::Solid => {
            painter.add(egui::Shape::line(points.to_vec(), stroke));
        }
        Dash::Dashed => {
            painter.extend(egui::Shape::dashed_line(points, stroke, 10.0, 6.0));
        }
        Dash::Dotted => {
            painter.extend(egui::Shape::dotted_line(points, stroke.color, 6.0, stroke.width * 0.7));
        }
    }
}

fn closed(points: &[Pos2]) -> Vec<Pos2> {
    let mut v = points.to_vec();
    if let Some(first) = points.first() {
        v.push(*first);
    }
    v
}

fn fill_polygon(painter: &Painter, points: &[Pos2], fill: Color32) {
    if fill.a() == 0 || points.len() < 3 {
        return;
    }
    let mut mesh = Mesh::default();
    for p in points {
        mesh.colored_vertex(*p, fill);
    }
    for [a, b, c] in triangulate(points) {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Diagonal hatching clipped to a polygon of any shape (even-odd).
pub(crate) fn hatch(painter: &Painter, polygon: &[Pos2], color: Color32) {
    if polygon.len() < 3 {
        return;
    }
    let sum = |p: &Pos2| p.x + p.y;
    let min = polygon.iter().map(sum).fold(f32::INFINITY, f32::min);
    let max = polygon.iter().map(sum).fold(f32::NEG_INFINITY, f32::max);
    let step = ((max - min) / 128.0).max(10.0);
    let mut value = min + step * 0.5;
    while value < max {
        let mut hits: Vec<Pos2> = Vec::new();
        for i in 0..polygon.len() {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            let (da, db) = (sum(&a), sum(&b));
            if (da <= value && db > value) || (db <= value && da > value) {
                hits.push(a + (b - a) * ((value - da) / (db - da)));
            }
        }
        hits.sort_by(|a, b| a.x.total_cmp(&b.x));
        for pair in hits.chunks(2) {
            if let [a, b] = pair {
                painter.line_segment([*a, *b], Stroke::new(1.0, color.gamma_multiply(0.5)));
            }
        }
        value += step;
    }
}

fn arrowhead(painter: &Painter, from: Pos2, tip: Pos2, c: Color32, width: f32) {
    let d = tip - from;
    if d.length() < 1.0 {
        return;
    }
    let dir = d.normalized();
    let side = vec2(-dir.y, dir.x);
    let len = 8.0 + width * 2.0;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, tip - dir * len + side * len * 0.5, tip - dir * len - side * len * 0.5],
        c,
        Stroke::NONE,
    ));
}

/// Where the name of a marker would sit with no offset.
fn label_base(proj: &dyn Project, m: &Marker, point_size: f32) -> Option<(Pos2, egui::Align2)> {
    let screen = |q: [f64; 2]| proj.at(q[0], q[1]);
    let at = proj.at(m.position[0], m.position[1]);
    match m.shape() {
        Shape::Circle => Some((at, egui::Align2::CENTER_CENTER)),
        Shape::Square | Shape::Rectangle | Shape::Polygon => Some((screen(label_anchor(m)), egui::Align2::CENTER_CENTER)),
        Shape::Line | Shape::Arrow | Shape::Route | Shape::Freehand => {
            Some((screen(label_anchor(m)) + vec2(0.0, -6.0), egui::Align2::CENTER_BOTTOM))
        }
        Shape::Point => Some((at + vec2(0.0, point_size + 3.0), egui::Align2::CENTER_TOP)),
        Shape::Pin => Some((at + vec2(PIN + 5.0, 0.0), egui::Align2::LEFT_CENTER)),
        Shape::Text => None,
    }
}

/// Where the name of a marker is drawn: its place, moved by the marker's own
/// offset. None for a text, which is its own name.
pub(crate) fn label_spot(proj: &dyn Project, m: &Marker, point_size: f32) -> Option<(Pos2, egui::Align2)> {
    let (pos, align) = label_base(proj, m, point_size)?;
    let shift = vec2(m.style.label_dx * proj.ppm(), -m.style.label_dy * proj.ppm());
    Some((pos + shift, align))
}

/// The box a name takes, to pick it.
fn text_box(pos: Pos2, align: egui::Align2, text: &str, size: f32) -> Rect {
    let size_box = vec2(text.chars().count() as f32 * size * 0.58 + 8.0, size + 8.0);
    let min = pos - vec2(align.x().to_factor() * size_box.x, align.y().to_factor() * size_box.y);
    Rect::from_min_size(min, size_box)
}

/// The pointer is on the name of the marker.
pub(crate) fn label_hit(proj: &dyn Project, m: &Marker, p: Pos2, point_size: f32) -> bool {
    if m.name.is_empty() || !m.style.label {
        return false;
    }
    label_spot(proj, m, point_size).is_some_and(|(pos, align)| text_box(pos, align, &m.name, m.style.text_size).contains(p))
}

/// Draws a marker.
pub(crate) fn paint(painter: &Painter, proj: &dyn Project, m: &Marker, look: &Look) {
    let base = color(m.color);
    let c = with_alpha(base, look.alpha);
    let width = m.style.width + if look.emphasis { 1.0 } else { 0.0 };
    let stroke = Stroke::new(width, c);
    let fill = with_alpha(base, look.alpha * f32::from(m.style.fill) / 100.0);
    let screen = |p: [f64; 2]| proj.at(p[0], p[1]);
    let at = proj.at(m.position[0], m.position[1]);

    match m.shape() {
        Shape::Circle => {
            let r = (m.radius.min(1024.0)) * proj.ppm();
            if fill.a() > 0 {
                painter.circle_filled(at, r, fill);
            }
            if m.hatched {
                let ring: Vec<Pos2> = (0..32)
                    .map(|n| {
                        let a = n as f32 * std::f32::consts::TAU / 32.0;
                        at + vec2(a.cos(), a.sin()) * r
                    })
                    .collect();
                hatch(painter, &ring, c);
            }
            let ring: Vec<Pos2> = (0..=64)
                .map(|n| {
                    let a = n as f32 * std::f32::consts::TAU / 64.0;
                    at + vec2(a.cos(), a.sin()) * r
                })
                .collect();
            stroke_path(painter, &ring, stroke, m.style.dash);
        }
        Shape::Square | Shape::Rectangle | Shape::Polygon => {
            let pts: Vec<Pos2> = outline(m).into_iter().map(screen).collect();
            if pts.len() >= 3 {
                fill_polygon(painter, &pts, fill);
                if m.hatched {
                    hatch(painter, &pts, c);
                }
                stroke_path(painter, &closed(&pts), stroke, m.style.dash);
            }
        }
        Shape::Line | Shape::Arrow | Shape::Route | Shape::Freehand => {
            let pts: Vec<Pos2> = polyline(m).into_iter().map(screen).collect();
            stroke_path(painter, &pts, stroke, m.style.dash);
            if matches!(m.shape(), Shape::Arrow | Shape::Route)
                && let [.., a, b] = pts.as_slice()
            {
                arrowhead(painter, *a, *b, c, width);
            }
        }
        Shape::Point => {
            painter.circle_filled(at, 3.0, c);
            painter.circle_stroke(at, look.point_size.clamp(3.0, 32.0), Stroke::new(2.0, c));
        }
        Shape::Pin => {
            painter.circle_filled(at, PIN, c);
            painter.circle_stroke(
                at,
                PIN,
                Stroke::new(if look.emphasis { 2.5 } else { 1.5 }, Color32::from_black_alpha(150)),
            );
            let ink = contrast(c);
            if !m.style.badge.is_empty() {
                painter.text(
                    at,
                    egui::Align2::CENTER_CENTER,
                    &m.style.badge,
                    egui::FontId::proportional(12.5),
                    ink,
                );
            } else if let Some(icon) = crate::ui::icons::global(&m.style.icon) {
                painter.image(
                    icon.id(),
                    Rect::from_center_size(at, vec2(14.0, 14.0)),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    ink,
                );
            }
        }
        Shape::Text => {
            if m.name.is_empty() {
                if !look.placeholder.is_empty() {
                    halo_text(
                        painter,
                        at,
                        egui::Align2::CENTER_CENTER,
                        look.placeholder,
                        m.style.text_size,
                        c.gamma_multiply(0.5),
                    );
                }
            } else {
                halo_text(painter, at, egui::Align2::CENTER_CENTER, &m.name, m.style.text_size, c);
            }
        }
    }
    if look.labels
        && m.style.label
        && !m.name.is_empty()
        && let Some((pos, align)) = label_spot(proj, m, look.point_size)
    {
        halo_text(painter, pos, align, &m.name, m.style.text_size, c);
    }
}

// ---- picking ---------------------------------------------------------------

/// How near the pointer is to a marker: lower picks first. None when it
/// misses. An outline or a line beats an inside, and of two insides the
/// smaller wins, so a small zone inside a big one stays reachable.
pub(crate) fn pick_score(proj: &dyn Project, m: &Marker, p: Pos2) -> Option<f32> {
    let screen = |q: [f64; 2]| proj.at(q[0], q[1]);
    let at = proj.at(m.position[0], m.position[1]);
    match m.shape() {
        Shape::Circle => {
            let r = m.radius * proj.ppm();
            let d = at.distance(p);
            if (d - r).abs() <= REACH {
                Some((d - r).abs())
            } else if d < r {
                Some(100.0 + r * r / 1000.0)
            } else {
                None
            }
        }
        Shape::Square | Shape::Rectangle | Shape::Polygon => {
            let pts: Vec<Pos2> = outline(m).into_iter().map(screen).collect();
            if pts.len() < 3 {
                return None;
            }
            let edge = dist_polyline(&closed(&pts), p);
            if edge <= REACH {
                Some(edge)
            } else if polygon_contains(&pts, p) {
                Some(100.0 + polygon_area(&pts).abs() / 1000.0)
            } else {
                None
            }
        }
        Shape::Line | Shape::Arrow | Shape::Route | Shape::Freehand => {
            let pts: Vec<Pos2> = polyline(m).into_iter().map(screen).collect();
            let d = dist_polyline(&pts, p);
            (d <= REACH).then_some(d)
        }
        Shape::Point => (at.distance(p) <= 12.0).then(|| at.distance(p)),
        Shape::Pin => (at.distance(p) <= PIN + 3.0).then(|| at.distance(p)),
        Shape::Text => {
            let w = m.name.chars().count() as f32 * m.style.text_size * 0.58 + 8.0;
            Rect::from_center_size(at, vec2(w, m.style.text_size + 8.0))
                .contains(p)
                .then_some(0.5)
        }
    }
}

/// The marker under the pointer: the best score among `candidates`, the
/// later (higher) one winning a tie.
pub(crate) fn pick<'a>(proj: &dyn Project, markers: impl Iterator<Item = &'a Marker>, p: Pos2) -> Option<uuid::Uuid> {
    markers
        .filter_map(|m| pick_score(proj, m, p).map(|s| (m.id, s)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

// ---- handles -----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Handle {
    Radius,
    Corner(usize),
    Rotate,
    Start,
    End,
    Vertex(usize),
    /// The middle of an edge: dragging it adds a vertex.
    Edge(usize),
}

/// The handles of a marker, in screen points.
pub(crate) fn handles(proj: &dyn Project, m: &Marker) -> Vec<(Handle, Pos2)> {
    let screen = |q: [f64; 2]| proj.at(q[0], q[1]);
    match m.shape() {
        Shape::Circle if m.radius > 0.0 => vec![(Handle::Radius, proj.at(m.position[0] + f64::from(m.radius), m.position[1]))],
        Shape::Square | Shape::Rectangle => {
            let Some(c) = corners(m) else { return Vec::new() };
            let mut out: Vec<(Handle, Pos2)> = c.iter().enumerate().map(|(i, p)| (Handle::Corner(i), screen(*p))).collect();
            let top = screen([(c[2][0] + c[3][0]) * 0.5, (c[2][1] + c[3][1]) * 0.5]);
            let centre = screen(rect_center(m));
            let away = (top - centre).normalized();
            out.push((Handle::Rotate, top + if away.is_finite() { away * 24.0 } else { vec2(0.0, -24.0) }));
            out
        }
        Shape::Line | Shape::Arrow => {
            let Some(e) = m.end else { return Vec::new() };
            vec![(Handle::Start, at_pos(proj, m)), (Handle::End, proj.at(e[0], e[1]))]
        }
        Shape::Polygon | Shape::Route => {
            let pts = if m.shape() == Shape::Polygon { outline(m) } else { polyline(m) };
            let mut out: Vec<(Handle, Pos2)> = pts.iter().enumerate().map(|(i, p)| (Handle::Vertex(i), screen(*p))).collect();
            let edges = if m.shape() == Shape::Polygon {
                pts.len()
            } else {
                pts.len().saturating_sub(1)
            };
            for i in 0..edges {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                out.push((Handle::Edge(i), screen([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5])));
            }
            out
        }
        _ => Vec::new(),
    }
}

fn at_pos(proj: &dyn Project, m: &Marker) -> Pos2 {
    proj.at(m.position[0], m.position[1])
}

/// The handle under the pointer, if any.
pub(crate) fn handle_at(proj: &dyn Project, m: &Marker, p: Pos2) -> Option<Handle> {
    handles(proj, m)
        .into_iter()
        .filter(|(_, at)| at.distance(p) <= 9.0)
        .min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p)))
        .map(|(h, _)| h)
}

/// Drags `handle` of `m` to the global point `w`, from the marker as it was
/// when the drag began. `keep_square` locks the sides of a rectangle, and
/// `snap_angle` rounds a rotation to 15 degrees.
pub(crate) fn drag_handle(m: &mut Marker, original: &Marker, handle: Handle, w: [f64; 2], keep_square: bool, snap_angle: bool) {
    match handle {
        Handle::Radius => {
            let r = (w[0] - original.position[0]).hypot(w[1] - original.position[1]);
            m.radius = (r as f32).clamp(0.1, 1024.0);
            m.shape = Shape::Circle;
        }
        Handle::Start => {
            m.position[0] = w[0];
            m.position[1] = w[1];
        }
        Handle::End => {
            if let Some(end) = &mut m.end {
                end[0] = w[0];
                end[1] = w[1];
            }
        }
        Handle::Corner(i) => {
            let Some(c) = corners(original) else { return };
            let fixed = c[(i + 2) % 4];
            let angle = f64::from(original.style.angle);
            // the diagonal from the fixed corner, in the rectangle's own frame
            let mut u = rotate([w[0] - fixed[0], w[1] - fixed[1]], -angle);
            // a square stays one only while Shift is held: it is a rectangle again after
            let square = keep_square;
            if square {
                let side = u[0].abs().max(u[1].abs());
                u = [side * u[0].signum(), side * u[1].signum()];
            }
            let far = rotate(u, angle);
            let centre = [fixed[0] + far[0] * 0.5, fixed[1] + far[1] * 0.5];
            let z = original.position[2];
            m.position = [centre[0] - u[0] * 0.5, centre[1] - u[1] * 0.5, z];
            m.end = Some([centre[0] + u[0] * 0.5, centre[1] + u[1] * 0.5, z]);
            m.shape = if square { Shape::Square } else { Shape::Rectangle };
        }
        Handle::Rotate => {
            let c = rect_center(original);
            let mut degrees = (w[1] - c[1]).atan2(w[0] - c[0]).to_degrees() - 90.0;
            if snap_angle {
                degrees = (degrees / 15.0).round() * 15.0;
            }
            while degrees > 180.0 {
                degrees -= 360.0;
            }
            while degrees < -180.0 {
                degrees += 360.0;
            }
            m.style.angle = degrees as f32;
        }
        Handle::Vertex(i) => {
            if let Some(p) = m.path.get_mut(i) {
                p[0] = w[0];
                p[1] = w[1];
            }
            if i == 0 && !m.path.is_empty() {
                m.position[0] = w[0];
                m.position[1] = w[1];
            }
        }
        Handle::Edge(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    struct Flat(f32);
    impl Project for Flat {
        fn at(&self, x: f64, y: f64) -> Pos2 {
            pos2(x as f32 * self.0, -(y as f32) * self.0)
        }
        fn ppm(&self) -> f32 {
            self.0
        }
    }

    fn marker(shape: Shape) -> Marker {
        Marker {
            id: Uuid::new_v4(),
            shape,
            color: [255, 0, 0, 255],
            ..Marker::default()
        }
    }

    #[test]
    fn a_rotated_rectangle_keeps_its_centre_and_turns_its_corners() {
        let mut r = marker(Shape::Rectangle);
        r.position = [0.0, 0.0, 0.0];
        r.end = Some([20.0, 10.0, 0.0]);
        let flat = corners(&r).expect("corners");
        assert_eq!(flat[0], [0.0, 0.0]);
        assert_eq!(flat[2], [20.0, 10.0]);
        r.style.angle = 90.0;
        let turned = corners(&r).expect("corners");
        assert!(((turned[0][0] + turned[2][0]) * 0.5 - 10.0).abs() < 1e-9 && ((turned[0][1] + turned[2][1]) * 0.5 - 5.0).abs() < 1e-9);
        assert!(
            (turned[0][0] - 15.0).abs() < 1e-9 && (turned[0][1] - -5.0).abs() < 1e-9,
            "{turned:?}"
        );
    }

    #[test]
    fn dragging_a_corner_keeps_the_opposite_one_in_place() {
        let mut r = marker(Shape::Rectangle);
        r.position = [0.0, 0.0, 5.0];
        r.end = Some([20.0, 10.0, 5.0]);
        r.style.angle = 30.0;
        let original = r.clone();
        let fixed = corners(&original).expect("corners")[0];
        let mut moved = original.clone();
        drag_handle(&mut moved, &original, Handle::Corner(2), [40.0, 30.0], false, false);
        let after = corners(&moved).expect("corners");
        assert!(
            (after[0][0] - fixed[0]).abs() < 1e-6 && (after[0][1] - fixed[1]).abs() < 1e-6,
            "{after:?} vs {fixed:?}"
        );
        assert!((after[2][0] - 40.0).abs() < 1e-6 && (after[2][1] - 30.0).abs() < 1e-6, "{after:?}");
        assert_eq!(moved.position[2], 5.0);
    }

    #[test]
    fn a_concave_polygon_fills_without_covering_its_notch() {
        let l = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 4.0),
            pos2(4.0, 4.0),
            pos2(4.0, 10.0),
            pos2(0.0, 10.0),
        ];
        let triangles = triangulate(&l);
        assert_eq!(triangles.len(), 4);
        let area: f32 = triangles.iter().map(|t| polygon_area(&[l[t[0]], l[t[1]], l[t[2]]]).abs()).sum();
        assert!((area - 64.0).abs() < 1e-3, "{area}");
        assert!(polygon_contains(&l, pos2(2.0, 8.0)));
        assert!(!polygon_contains(&l, pos2(8.0, 8.0)));
    }

    #[test]
    fn the_smaller_zone_wins_inside_a_bigger_one_and_a_line_beats_both() {
        let proj = Flat(2.0);
        let mut big = marker(Shape::Circle);
        big.radius = 50.0;
        let mut small = marker(Shape::Circle);
        small.radius = 10.0;
        let mut line = marker(Shape::Line);
        line.position = [-5.0, 0.0, 0.0];
        line.end = Some([5.0, 0.0, 0.0]);
        let centre = proj.at(0.0, 0.0);
        let both = [big.clone(), small.clone()];
        assert_eq!(pick(&proj, both.iter(), centre), Some(small.id));
        let all = [big, small, line.clone()];
        assert_eq!(pick(&proj, all.iter(), centre), Some(line.id));
        assert_eq!(pick(&proj, all.iter(), pos2(500.0, 500.0)), None);
    }

    #[test]
    fn handles_follow_each_shape() {
        let proj = Flat(1.0);
        let mut c = marker(Shape::Circle);
        c.radius = 10.0;
        assert_eq!(handles(&proj, &c).len(), 1);
        let mut poly = marker(Shape::Polygon);
        poly.path = vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]];
        assert_eq!(handles(&proj, &poly).len(), 6, "three vertices and three edges");
        let mut route = marker(Shape::Route);
        route.path = vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]];
        assert_eq!(handles(&proj, &route).len(), 5, "three vertices and two edges");
        assert!(handles(&proj, &marker(Shape::Pin)).is_empty());
    }

    #[test]
    fn a_name_moves_with_its_offset_and_is_picked_where_it_is_drawn() {
        let proj = Flat(2.0);
        let mut m = marker(Shape::Circle);
        m.radius = 10.0;
        m.name = "Bridge".into();
        let (home, _) = label_spot(&proj, &m, 8.0).expect("spot");
        assert!(label_hit(&proj, &m, home, 8.0));
        m.style.label_dx = 30.0;
        m.style.label_dy = 12.0;
        let (moved, _) = label_spot(&proj, &m, 8.0).expect("spot");
        assert!(
            (moved.x - home.x - 60.0).abs() < 1e-3 && (moved.y - home.y + 24.0).abs() < 1e-3,
            "{home:?} -> {moved:?}"
        );
        assert!(label_hit(&proj, &m, moved, 8.0));
        assert!(!label_hit(&proj, &m, home + vec2(0.0, 80.0), 8.0));
        m.name.clear();
        assert!(!label_hit(&proj, &m, moved, 8.0), "no name, nothing to grab");
        assert!(label_spot(&proj, &marker(Shape::Text), 8.0).is_none(), "a text is its own name");
    }

    #[test]
    fn a_square_becomes_a_rectangle_again_unless_shift_holds_it() {
        let mut square = marker(Shape::Square);
        square.position = [0.0, 0.0, 0.0];
        square.end = Some([10.0, 10.0, 0.0]);
        let original = square.clone();
        let mut free = original.clone();
        drag_handle(&mut free, &original, Handle::Corner(2), [30.0, 12.0], false, false);
        assert_eq!(free.shape, Shape::Rectangle);
        let c = corners(&free).expect("corners");
        assert!((c[2][0] - 30.0).abs() < 1e-6 && (c[2][1] - 12.0).abs() < 1e-6, "{c:?}");
        let mut held = original.clone();
        drag_handle(&mut held, &original, Handle::Corner(2), [30.0, 12.0], true, false);
        assert_eq!(held.shape, Shape::Square);
    }
}
