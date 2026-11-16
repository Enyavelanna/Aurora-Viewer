//! Native egui input through the minimap, without grid or browser access.
use super::*;
use crate::combat::tactical::Shape;
use crate::ui::widgets::elide_tests::Frames;

fn draw(
    frames: &mut Frames,
    map: &mut MiniMap,
    world: &World,
    tiles: &mut MapTiles,
    camera: &MapCamera,
    opts: &mut MapSettings,
    events: Vec<egui::Event>,
) -> Vec<MiniMapAction> {
    let mut actions = Vec::new();
    let palette = crate::theme::Theme::default().palette();
    frames.run(events, |ui| {
        actions = map.draw(ui, &palette, world, tiles, camera, opts, vec2(300.0, 300.0), true);
    });
    actions
}
fn world() -> World {
    World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()))
}
fn click(at: Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(at),
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}
#[test]
fn combat_fixed_map_stays_on_the_same_world_point_after_camera_and_avatar_motion() {
    let mut world = world();
    world.agent.position = Vec3::new(50.0, 60.0, 30.0);
    let mut camera = MapCamera {
        pos: world.agent.position,
        dir: Vec3::X,
        hfov: 1.0,
        far: 128.0,
    };
    let mut frames = Frames::new();
    let mut map = MiniMap::default();
    let mut tiles = MapTiles::new();
    let mut opts = MapSettings::default();
    opts.tactical.fixed_center = true;
    draw(&mut frames, &mut map, &world, &mut tiles, &camera, &mut opts, vec![]);
    let anchor = map.fixed_origin.expect("fixed centre");
    world.agent.position.x += 100.0;
    world.agent.yaw = 1.7;
    camera.pos = world.agent.position + Vec3::new(20.0, 40.0, 10.0);
    camera.dir = Vec3::Y;
    draw(&mut frames, &mut map, &world, &mut tiles, &camera, &mut opts, vec![]);
    assert_eq!(map.fixed_origin, Some(anchor));
    map.recenter = true;
    draw(&mut frames, &mut map, &world, &mut tiles, &camera, &mut opts, vec![]);
    assert_eq!(map.fixed_origin, Some(to_global(&world, world.agent.position)));
}
#[test]
fn combat_two_click_shapes_claim_double_clicks_and_escape_preserves_existing_markers() {
    let world = world();
    let camera = MapCamera {
        pos: Vec3::ZERO,
        dir: Vec3::X,
        hfov: 1.0,
        far: 128.0,
    };
    let mut frames = Frames::new();
    let mut map = MiniMap::default();
    let mut tiles = MapTiles::new();
    let mut opts = MapSettings {
        mini_double_click: 2,
        ..Default::default()
    };
    opts.tactical.enabled = true;
    opts.tactical.session.draw = Some(Shape::Square);
    draw(&mut frames, &mut map, &world, &mut tiles, &camera, &mut opts, vec![]);
    for point in [pos2(80.0, 80.0), pos2(150.0, 120.0)] {
        for pressed in [true, false] {
            let actions = draw(&mut frames, &mut map, &world, &mut tiles, &camera, &mut opts, click(point, pressed));
            assert!(!actions.iter().any(|a| matches!(a, MiniMapAction::Teleport { .. })));
        }
    }
    assert_eq!(opts.tactical.markers.len(), 1);
    let marker = &opts.tactical.markers[0];
    let end = marker.end.expect("square end");
    assert_eq!((end[0] - marker.position[0]).abs(), (end[1] - marker.position[1]).abs());
    let original = marker.clone();
    opts.tactical.session.editing = true;
    draw(
        &mut frames,
        &mut map,
        &world,
        &mut tiles,
        &camera,
        &mut opts,
        click(pos2(80.0, 80.0), true),
    );
    for point in [pos2(95.0, 95.0), pos2(110.0, 110.0)] {
        let actions = draw(
            &mut frames,
            &mut map,
            &world,
            &mut tiles,
            &camera,
            &mut opts,
            vec![egui::Event::PointerMoved(point)],
        );
        assert!(!actions.iter().any(|a| matches!(a, MiniMapAction::Teleport { .. })));
    }
    assert_ne!(opts.tactical.markers[0].position, original.position);
    draw(
        &mut frames,
        &mut map,
        &world,
        &mut tiles,
        &camera,
        &mut opts,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(opts.tactical.markers[0].position, original.position);
    assert_eq!(opts.tactical.markers[0].end, original.end);
    draw(
        &mut frames,
        &mut map,
        &world,
        &mut tiles,
        &camera,
        &mut opts,
        click(pos2(110.0, 110.0), false),
    );
    opts.tactical.session.draw = Some(Shape::Line);
    opts.tactical.session.drawing_start = Some([1.0, 2.0, 3.0]);
    draw(
        &mut frames,
        &mut map,
        &world,
        &mut tiles,
        &camera,
        &mut opts,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(opts.tactical.session.draw.is_none() && opts.tactical.session.drawing_start.is_none());
    assert_eq!(opts.tactical.markers.len(), 1);
}

#[test]
fn the_export_rasters_come_from_copied_inputs_layer_by_layer() {
    use crate::combat::tactical::{Ground, MapView};
    let mut heightmap = crate::world::terrain::Heightmap::new(256, 256);
    for (i, h) in heightmap.heights.iter_mut().enumerate() {
        *h = 10.0 + (i % 256) as f32 * 0.1;
    }
    let inputs = |view: MapView, markers_only: bool| ExportInputs {
        heightmap: heightmap.clone(),
        water: 20.0,
        origin: (256.0, 512.0),
        size_m: (256.0, 256.0),
        view,
        markers_only,
        objects: view.objects.then(|| {
            vec![ObjectDot {
                gx: 256.0 + 128.0,
                gy: 512.0 + 128.0,
                r: 2.0,
                color: YOU_ABOVE,
            }]
        }),
        parcels: view.parcels.then(|| ParcelInputs {
            regions: vec![ParcelRegion {
                origin: (256.0, 512.0),
                size: (256.0, 256.0),
                // one south line on the second cell row, nothing else
                overlay: Some((64, 64, {
                    let mut cells = vec![0u8; 64 * 64];
                    cells[64] = 0x80;
                    cells
                })),
                banned: None,
            }],
        }),
        for_sale: false,
    };
    let side = 64;
    // the markers alone: no ground, only what the view asks for
    let only = export_rasters(&inputs(MapView::preset(2, false), true), side);
    assert!(only.is_empty(), "a clean view of the markers alone has no layer");
    let full = export_rasters(&inputs(MapView::default(), false), side);
    assert_eq!(full.len(), 3, "ground, objects, parcels");
    assert!(full.iter().all(|img| img.size == [side, side]));
    // shaded ground: water on the west (low), land further east
    let ground = &full[0];
    assert_ne!(ground.pixels[0], ground.pixels[side - 1], "the ground is shaded by height");
    let flat = export_rasters(
        &inputs(
            MapView {
                ground: Ground::Flat,
                ..MapView::preset(2, false)
            },
            false,
        ),
        side,
    );
    assert_eq!(flat.len(), 1);
    assert!(
        flat[0].pixels.iter().all(|px| *px == flat[0].pixels[0]),
        "a flat ground is one colour"
    );
    // the object: a square at the centre, (2 r tpm) texels wide
    let objects = &full[1];
    let centre = side / 2;
    assert_eq!(objects.pixels[centre * side + centre], YOU_ABOVE);
    assert_eq!(objects.pixels[0], Color32::TRANSPARENT);
    assert_eq!(
        objects.pixels.iter().filter(|px| **px == YOU_ABOVE).count(),
        1,
        "round(2 × 2 m × 0.25 texels/m) = 1 texel"
    );
    // the parcel lines: the region's north border along the top row, the
    // cell's south line one texel wide on the bottom row (the region's own
    // south edge falls on the row below the picture, as on the mini-map)
    let parcels = &full[2];
    let line = Color32::from_rgba_unmultiplied(255, 255, 255, 255);
    assert!(parcels.pixels[..side].iter().all(|px| *px == line), "the north border");
    assert_eq!(
        parcels.pixels[(side - 1) * side],
        line,
        "the cell's south line starts at its west corner"
    );
    assert_eq!(parcels.pixels[(side - 1) * side + 1], line, "and ends at its east corner");
    assert_eq!(
        parcels.pixels[(side - 1) * side + 2],
        Color32::TRANSPARENT,
        "the next cell has no line"
    );
    assert_eq!(parcels.pixels[(side - 2) * side + 1], Color32::TRANSPARENT, "nothing above it");
}
