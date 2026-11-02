//! The annotated map as a picture: the region's terrain, objects and parcel
//! lines underneath, then every marker as vector shapes (an SVG rasterised
//! with resvg, so lines stay crisp at any size), with a title, a legend of
//! the lettered pins, the scale and the north.
//!
//! The colours of the markers are the user's; the rest of the sheet uses a
//! fixed dark look, so a picture reads the same wherever it is shared.
use super::tactical::{Dash, Marker, Shape};
use crate::ui::combat_annotate as geom;
use std::fmt::Write;

/// What the picture is of.
pub(crate) struct Sheet<'a> {
    pub region: &'a str,
    pub set: &'a str,
    /// South-west corner of the region, global metres.
    pub origin: [f64; 2],
    /// Metres the picture covers on each side.
    pub size_m: f64,
    /// Picture side, pixels.
    pub side: u32,
    pub title: bool,
    pub legend: bool,
    /// The heading of the legend (translated by the caller).
    pub legend_heading: &'a str,
}

/// XML text, escaped.
fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn rgb(c: [u8; 4]) -> String {
    format!("rgb({},{},{})", c[0], c[1], c[2])
}

/// Black or near-white, whichever reads on `c`.
fn contrast(c: [u8; 4]) -> &'static str {
    let luma = (0.299 * f32::from(c[0]) + 0.587 * f32::from(c[1]) + 0.114 * f32::from(c[2])) / 255.0;
    if luma > 0.6 { "#0b0d12" } else { "#ffffff" }
}

struct Frame<'a> {
    sheet: &'a Sheet<'a>,
    /// Pixels per metre.
    k: f64,
    /// Pixels per editor point: strokes and texts keep their look at any size.
    u: f64,
}

impl Frame<'_> {
    fn x(&self, gx: f64) -> f64 {
        (gx - self.sheet.origin[0]) * self.k
    }
    fn y(&self, gy: f64) -> f64 {
        f64::from(self.sheet.side) - (gy - self.sheet.origin[1]) * self.k
    }
    fn points(&self, pts: &[[f64; 2]]) -> String {
        pts.iter()
            .map(|p| format!("{:.2},{:.2}", self.x(p[0]), self.y(p[1])))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The stroke attributes of a line of this style.
fn stroke_attrs(m: &Marker, u: f64) -> String {
    let w = f64::from(m.style.width) * u;
    let dash = match m.style.dash {
        Dash::Solid => String::new(),
        Dash::Dashed => format!(" stroke-dasharray=\"{:.1} {:.1}\"", 11.0 * u, 7.0 * u),
        Dash::Dotted => format!(
            " stroke-dasharray=\"{:.1} {:.1}\" stroke-linecap=\"round\"",
            0.1 * u,
            5.0 * w.max(u)
        ),
    };
    format!(
        "stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\"{dash}",
        rgb(m.color),
        f64::from(m.color[3]) / 255.0,
        w
    )
}

fn fill_attrs(m: &Marker, hatch: Option<usize>) -> String {
    let alpha = f64::from(m.color[3]) / 255.0 * f64::from(m.style.fill) / 100.0;
    match hatch {
        Some(n) => format!("fill=\"url(#hatch{n})\""),
        None if alpha > 0.0 => format!("fill=\"{}\" fill-opacity=\"{alpha:.3}\"", rgb(m.color)),
        None => "fill=\"none\"".to_owned(),
    }
}

/// A label with a dark halo.
fn halo(out: &mut String, x: f64, y: f64, anchor: &str, baseline: &str, size: f64, color: &str, bold: bool, text: &str) {
    let weight = if bold { " font-weight=\"bold\"" } else { "" };
    let _ = write!(
        out,
        "<text x=\"{x:.2}\" y=\"{y:.2}\" text-anchor=\"{anchor}\" dominant-baseline=\"{baseline}\" font-size=\"{size:.1}\"{weight} \
         fill=\"{color}\" stroke=\"#000000\" stroke-opacity=\"0.85\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\" paint-order=\"stroke\">{}</text>",
        size * 0.22,
        esc(text)
    );
}

fn arrowhead(out: &mut String, from: (f64, f64), tip: (f64, f64), m: &Marker, u: f64) {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = dx.hypot(dy);
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let head = (8.0 + f64::from(m.style.width) * 2.0) * u;
    let (nx, ny) = (-uy, ux);
    let _ = write!(
        out,
        "<polygon points=\"{:.2},{:.2} {:.2},{:.2} {:.2},{:.2}\" fill=\"{}\" fill-opacity=\"{:.3}\"/>",
        tip.0,
        tip.1,
        tip.0 - ux * head + nx * head * 0.5,
        tip.1 - uy * head + ny * head * 0.5,
        tip.0 - ux * head - nx * head * 0.5,
        tip.1 - uy * head - ny * head * 0.5,
        rgb(m.color),
        f64::from(m.color[3]) / 255.0
    );
}

/// One marker as SVG. `n` numbers its hatch pattern.
fn marker(out: &mut String, f: &Frame, m: &Marker, n: usize) {
    let u = f.u;
    let at = (f.x(m.position[0]), f.y(m.position[1]));
    let stroke = stroke_attrs(m, u);
    let hatch_id = m.hatched.then_some(n);
    if let Some(id) = hatch_id {
        let _ = write!(
            out,
            "<defs><pattern id=\"hatch{id}\" patternUnits=\"userSpaceOnUse\" width=\"{:.1}\" height=\"{:.1}\" patternTransform=\"rotate(45)\">\
             <line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"{:.1}\" stroke=\"{}\" stroke-opacity=\"0.55\" stroke-width=\"{:.2}\"/></pattern></defs>",
            9.0 * u,
            9.0 * u,
            9.0 * u,
            rgb(m.color),
            u
        );
    }
    // where the name sits by default: (x, y, anchor, baseline)
    let mut name: Option<(f64, f64, &str, &str)> = None;
    match m.shape() {
        Shape::Circle => {
            let r = f64::from(m.radius.min(1024.0)) * f.k;
            let _ = write!(
                out,
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{r:.2}\" {} {stroke}/>",
                at.0,
                at.1,
                fill_attrs(m, None)
            );
            if hatch_id.is_some() {
                let _ = write!(
                    out,
                    "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{r:.2}\" {}/>",
                    at.0,
                    at.1,
                    fill_attrs(m, hatch_id)
                );
            }
            name = Some((at.0, at.1, "middle", "central"));
        }
        Shape::Square | Shape::Rectangle | Shape::Polygon => {
            let pts = geom::outline(m);
            if pts.len() >= 3 {
                let points = f.points(&pts);
                let _ = write!(out, "<polygon points=\"{points}\" {} {stroke}/>", fill_attrs(m, None));
                if hatch_id.is_some() {
                    let _ = write!(out, "<polygon points=\"{points}\" {}/>", fill_attrs(m, hatch_id));
                }
                let a = geom::label_anchor(m);
                name = Some((f.x(a[0]), f.y(a[1]), "middle", "central"));
            }
        }
        Shape::Line | Shape::Arrow | Shape::Route | Shape::Freehand => {
            let pts = geom::polyline(m);
            if pts.len() >= 2 {
                let _ = write!(out, "<polyline points=\"{}\" fill=\"none\" {stroke}/>", f.points(&pts));
                if matches!(m.shape(), Shape::Arrow | Shape::Route)
                    && let [.., a, b] = pts.as_slice()
                {
                    arrowhead(out, (f.x(a[0]), f.y(a[1])), (f.x(b[0]), f.y(b[1])), m, u);
                }
                let a = geom::label_anchor(m);
                name = Some((f.x(a[0]), f.y(a[1]) - 6.0 * u, "middle", "text-after-edge"));
            }
        }
        Shape::Point => {
            let _ = write!(
                out,
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{}\"/><circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"none\" {stroke}/>",
                at.0,
                at.1,
                3.0 * u,
                rgb(m.color),
                at.0,
                at.1,
                8.0 * u
            );
            name = Some((at.0, at.1 + 11.0 * u, "middle", "text-before-edge"));
        }
        Shape::Pin => {
            let r = 11.0 * u;
            let _ = write!(
                out,
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{r:.2}\" fill=\"{}\" stroke=\"#000000\" stroke-opacity=\"0.6\" stroke-width=\"{:.2}\"/>",
                at.0,
                at.1,
                rgb(m.color),
                1.5 * u
            );
            let ink = contrast(m.color);
            if m.style.badge.is_empty() {
                let _ = write!(
                    out,
                    "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{ink}\"/>",
                    at.0,
                    at.1,
                    3.5 * u
                );
            } else {
                let _ = write!(
                    out,
                    "<text x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-size=\"{:.1}\" font-weight=\"bold\" fill=\"{ink}\">{}</text>",
                    at.0,
                    at.1,
                    12.5 * u,
                    esc(&m.style.badge)
                );
            }
            name = Some((at.0 + r + 5.0 * u, at.1, "start", "central"));
        }
        Shape::Text => {
            if !m.name.is_empty() {
                halo(
                    out,
                    at.0,
                    at.1,
                    "middle",
                    "central",
                    f64::from(m.style.text_size) * u,
                    &rgb(m.color),
                    false,
                    &m.name,
                );
            }
        }
    }
    if m.style.label
        && !m.name.is_empty()
        && let Some((x, y, anchor, baseline)) = name
    {
        let (x, y) = (x + f64::from(m.style.label_dx) * f.k, y - f64::from(m.style.label_dy) * f.k);
        halo(
            out,
            x,
            y,
            anchor,
            baseline,
            f64::from(m.style.text_size) * u,
            &rgb(m.color),
            false,
            &m.name,
        );
    }
}

/// A round length for the scale bar.
fn nice_length(max_metres: f64) -> f64 {
    [5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0]
        .into_iter()
        .rev()
        .find(|v| *v <= max_metres)
        .unwrap_or(5.0)
}

/// The sheet as SVG: `markers` back to front.
pub(crate) fn svg(markers: &[&Marker], sheet: &Sheet) -> String {
    let side = f64::from(sheet.side);
    let f = Frame {
        sheet,
        k: side / sheet.size_m.max(1.0),
        u: side / 1024.0,
    };
    let u = f.u;
    let mut out = String::new();
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{0}\" height=\"{0}\" viewBox=\"0 0 {0} {0}\">",
        sheet.side
    );
    for (n, m) in markers.iter().enumerate() {
        marker(&mut out, &f, m, n);
    }
    // the scale and the north
    let metres = nice_length(sheet.size_m * 0.25);
    let (bx, by) = (24.0 * u, side - 28.0 * u);
    let len = metres * f.k;
    let _ = write!(
        out,
        "<g stroke=\"#ffffff\" stroke-width=\"{:.2}\"><line x1=\"{bx:.2}\" y1=\"{by:.2}\" x2=\"{:.2}\" y2=\"{by:.2}\"/>\
         <line x1=\"{bx:.2}\" y1=\"{:.2}\" x2=\"{bx:.2}\" y2=\"{:.2}\"/><line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/></g>",
        2.0 * u,
        bx + len,
        by - 5.0 * u,
        by + 5.0 * u,
        bx + len,
        by - 5.0 * u,
        bx + len,
        by + 5.0 * u
    );
    halo(
        &mut out,
        bx + len * 0.5,
        by - 8.0 * u,
        "middle",
        "text-after-edge",
        13.0 * u,
        "#ffffff",
        false,
        &format!("{metres:.0} m"),
    );
    let (nx, ny) = (side - 34.0 * u, 52.0 * u);
    let _ = write!(
        out,
        "<polygon points=\"{nx:.2},{:.2} {:.2},{:.2} {:.2},{:.2}\" fill=\"#ffffff\" stroke=\"#000000\" stroke-opacity=\"0.6\" stroke-width=\"{:.2}\"/>",
        ny - 22.0 * u,
        nx + 9.0 * u,
        ny + 2.0 * u,
        nx - 9.0 * u,
        ny + 2.0 * u,
        u
    );
    halo(
        &mut out,
        nx,
        ny + 6.0 * u,
        "middle",
        "text-before-edge",
        15.0 * u,
        "#ffffff",
        true,
        "N",
    );
    if sheet.title {
        let width = (sheet.region.chars().count().max(sheet.set.chars().count()) as f64 * 15.0 * u).clamp(180.0 * u, side * 0.6);
        let _ = write!(
            out,
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{width:.2}\" height=\"{:.2}\" rx=\"{:.2}\" fill=\"#0b0d12\" fill-opacity=\"0.72\"/>",
            16.0 * u,
            16.0 * u,
            if sheet.set.is_empty() { 44.0 * u } else { 68.0 * u },
            6.0 * u
        );
        halo(
            &mut out,
            30.0 * u,
            26.0 * u,
            "start",
            "text-before-edge",
            24.0 * u,
            "#ffffff",
            true,
            sheet.region,
        );
        if !sheet.set.is_empty() {
            halo(
                &mut out,
                30.0 * u,
                56.0 * u,
                "start",
                "text-before-edge",
                15.0 * u,
                "#c4b5fd",
                false,
                sheet.set,
            );
        }
    }
    if sheet.legend {
        let pins: Vec<&&Marker> = markers
            .iter()
            .filter(|m| m.shape == Shape::Pin && !m.name.is_empty())
            .take(18)
            .collect();
        if !pins.is_empty() {
            let row = 26.0 * u;
            let longest = pins.iter().map(|m| m.name.chars().count()).max().unwrap_or(0) as f64;
            let width = (longest * 9.5 * u + 70.0 * u).max(sheet.legend_heading.chars().count() as f64 * 12.0 * u + 40.0 * u);
            let height = row * pins.len() as f64 + 44.0 * u;
            let (lx, ly) = (side - width - 16.0 * u, side - height - 16.0 * u);
            let _ = write!(
                out,
                "<rect x=\"{lx:.2}\" y=\"{ly:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" rx=\"{:.2}\" fill=\"#0b0d12\" fill-opacity=\"0.72\"/>",
                6.0 * u
            );
            halo(
                &mut out,
                lx + 14.0 * u,
                ly + 12.0 * u,
                "start",
                "text-before-edge",
                13.0 * u,
                "#c4b5fd",
                true,
                sheet.legend_heading,
            );
            for (i, m) in pins.iter().enumerate() {
                let cy = ly + 40.0 * u + row * i as f64 + row * 0.5;
                let _ = write!(
                    out,
                    "<circle cx=\"{:.2}\" cy=\"{cy:.2}\" r=\"{:.2}\" fill=\"{}\"/>",
                    lx + 24.0 * u,
                    9.0 * u,
                    rgb(m.color)
                );
                let _ = write!(
                    out,
                    "<text x=\"{:.2}\" y=\"{cy:.2}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-size=\"{:.1}\" font-weight=\"bold\" fill=\"{}\">{}</text>",
                    lx + 24.0 * u,
                    10.5 * u,
                    contrast(m.color),
                    esc(&m.style.badge)
                );
                halo(&mut out, lx + 42.0 * u, cy, "start", "central", 14.0 * u, "#ffffff", false, &m.name);
            }
        }
    }
    out.push_str("</svg>");
    out
}

/// Terrain and the like under the markers, then the markers, as PNG bytes.
/// `base` are square pictures of the region (opaque terrain first), each
/// `side` pixels wide; `fonts` the faces the names are written with.
pub(crate) fn render(base: &[egui::ColorImage], svg: &str, fonts: &[Vec<u8>], side: u32) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};
    let mut canvas = tiny_skia::Pixmap::new(side, side).ok_or("size")?;
    let n = (side as usize) * (side as usize);
    // straight RGBA over a dark ground
    let mut buf: Vec<[u8; 4]> = vec![[24, 27, 35, 255]; n];
    for image in base {
        if image.size != [side as usize, side as usize] {
            return Err("layer size".into());
        }
        for (out, px) in buf.iter_mut().zip(&image.pixels) {
            let [r, g, b, a] = px.to_srgba_unmultiplied();
            let a = u32::from(a);
            for (o, s) in out.iter_mut().zip([r, g, b]) {
                *o = ((u32::from(s) * a + u32::from(*o) * (255 - a)) / 255) as u8;
            }
        }
    }
    for (dst, src) in canvas.pixels_mut().iter_mut().zip(&buf) {
        *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], 255).premultiply();
    }
    let mut options = usvg::Options::default();
    let mut family = None;
    {
        let db = options.fontdb_mut();
        for face in fonts {
            db.load_font_data(face.clone());
        }
        family = db
            .faces()
            .next()
            .and_then(|f| f.families.first().map(|(name, _)| name.clone()))
            .or(family);
        if let Some(name) = &family {
            db.set_sans_serif_family(name.clone());
        }
    }
    if let Some(name) = family {
        options.font_family = name;
    }
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut canvas.as_mut());
    canvas.encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sheet(side: u32) -> Sheet<'static> {
        Sheet {
            region: "Aurora <Plaza> & co",
            set: "Attack",
            origin: [1000.0, 2000.0],
            size_m: 256.0,
            side,
            title: true,
            legend: true,
            legend_heading: "Legend",
        }
    }

    fn marker(shape: Shape) -> Marker {
        Marker {
            id: Uuid::new_v4(),
            shape,
            color: [239, 68, 68, 255],
            ..Marker::default()
        }
    }

    #[test]
    fn every_shape_becomes_markup_and_names_are_escaped() {
        let mut circle = marker(Shape::Circle);
        circle.position = [1100.0, 2100.0, 0.0];
        circle.radius = 20.0;
        circle.name = "A & <B>".into();
        circle.hatched = true;
        let mut poly = marker(Shape::Polygon);
        poly.path = vec![[1010.0, 2010.0, 0.0], [1050.0, 2010.0, 0.0], [1030.0, 2050.0, 0.0]];
        poly.position = poly.path[0];
        poly.style.dash = Dash::Dashed;
        let mut route = marker(Shape::Route);
        route.path = vec![[1010.0, 2200.0, 0.0], [1100.0, 2150.0, 0.0], [1200.0, 2220.0, 0.0]];
        route.position = route.path[0];
        let mut pin = marker(Shape::Pin);
        pin.position = [1200.0, 2100.0, 0.0];
        pin.style.badge = "A".into();
        pin.name = "Spawn".into();
        let mut text = marker(Shape::Text);
        text.position = [1050.0, 2150.0, 0.0];
        text.name = "MID".into();
        let all = [&circle, &poly, &route, &pin, &text];
        let svg = svg(&all, &sheet(1024));
        for part in [
            "<circle",
            "<polygon",
            "<polyline",
            "stroke-dasharray",
            "url(#hatch0)",
            "A &amp; &lt;B&gt;",
            "Aurora &lt;Plaza&gt; &amp; co",
            "MID",
            "Legend",
            "Spawn",
        ] {
            assert!(svg.contains(part), "missing {part}");
        }
        assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>"));
        assert!(!svg.contains("A & <B>"));
    }

    #[test]
    fn a_name_moved_by_hand_is_written_where_it_was_put() {
        let mut m = marker(Shape::Circle);
        m.position = [1128.0, 2128.0, 0.0];
        m.radius = 10.0;
        m.name = "Bridge".into();
        let plain = svg(&[&m], &sheet(1024));
        m.style.label_dx = 20.0;
        m.style.label_dy = 10.0;
        let moved = svg(&[&m], &sheet(1024));
        let at = |s: &str| {
            let i = s.find(">Bridge<").expect("name");
            let head = &s[..i];
            let x: f64 = head
                .rsplit("<text x=\"")
                .next()
                .expect("x")
                .split('"')
                .next()
                .expect("x")
                .parse()
                .expect("number");
            let y: f64 = head
                .rsplit(" y=\"")
                .next()
                .expect("y")
                .split('"')
                .next()
                .expect("y")
                .parse()
                .expect("number");
            (x, y)
        };
        let (a, b) = (at(&plain), at(&moved));
        assert!((b.0 - a.0 - 80.0).abs() < 0.5 && (b.1 - a.1 + 40.0).abs() < 0.5, "{a:?} -> {b:?}");
    }

    #[test]
    fn the_picture_has_the_terrain_under_the_shapes_and_is_a_png() {
        let side = 128;
        let terrain = egui::ColorImage::filled([side, side], egui::Color32::from_rgb(40, 80, 40));
        let mut zone = marker(Shape::Circle);
        zone.position = [1128.0, 2128.0, 0.0];
        zone.radius = 20.0;
        zone.style.fill = 100;
        let mut s = sheet(side as u32);
        s.title = false;
        s.legend = false;
        let svg = svg(&[&zone], &s);
        let png = render(&[terrain], &svg, &[], side as u32).expect("render");
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        let image = image::load_from_memory(&png).expect("decode").to_rgba8();
        // the middle of the zone is the zone's red, a corner is the terrain
        let middle = image.get_pixel(side as u32 / 2, side as u32 / 2);
        assert!(middle[0] > 200 && middle[1] < 90, "{middle:?}");
        let corner = image.get_pixel(60, 10);
        assert!(corner[0] < 80 && corner[1] > 60, "{corner:?}");
    }

    #[test]
    fn a_layer_of_the_wrong_size_is_refused() {
        let wrong = egui::ColorImage::filled([10, 10], egui::Color32::WHITE);
        assert!(
            render(
                &[wrong],
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"/>",
                &[],
                64
            )
            .is_err()
        );
        assert_eq!(nice_length(60.0), 50.0);
    }
}
