//! The annotated map as a PNG (the region editor's Set tab): the terrain,
//! the markers drawn over it, a title, a legend of the pins, the scale and
//! the north. The save dialog comes first; the rasters, the SVG, the render and
//! the file write run on a worker from a snapshot taken here, and the result
//! comes back as the same system message as ever.
use super::*;
use crate::combat::export::{self, Sheet};
use crate::combat::tactical::Marker;
use aurora_i18n::tr;

/// How a picture ended.
enum Outcome {
    Saved(String),
    Failed,
}

/// One export at a time, on its own thread.
pub(super) struct MapExports {
    tx: crossbeam_channel::Sender<Outcome>,
    rx: crossbeam_channel::Receiver<Outcome>,
    running: bool,
}
impl Default for MapExports {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx, running: false }
    }
}
impl MapExports {
    /// A picture is being made: the editor's button waits.
    pub(super) fn running(&self) -> bool {
        self.running
    }
}

/// What the worker needs, all owned.
struct Work {
    inputs: ui::minimap::ExportInputs,
    markers: Vec<Marker>,
    region: String,
    set: String,
    origin: [f64; 2],
    side_m: f64,
    job: ui::combat_editor::ExportJob,
    legend_heading: String,
    path: std::path::PathBuf,
}

impl Work {
    fn run(self) -> Outcome {
        let base = self.inputs.rasters();
        let sheet = Sheet {
            region: &self.region,
            set: &self.set,
            origin: self.origin,
            size_m: self.side_m,
            side: self.job.side,
            title: self.job.title,
            legend: self.job.legend,
            legend_heading: &self.legend_heading,
        };
        let order: Vec<&Marker> = self.markers.iter().collect();
        let svg = export::svg(&order, &sheet);
        let Ok(png) = export::render(&base, &svg, &ui::fonts::export_faces(), self.job.side, self.job.transparent) else {
            return Outcome::Failed;
        };
        match std::fs::write(&self.path, png) {
            Ok(()) => Outcome::Saved(self.path.display().to_string()),
            Err(_) => Outcome::Failed,
        }
    }
}

impl App {
    /// Reports the pictures that are done.
    pub(super) fn poll_map_export(&mut self) {
        while let Ok(outcome) = self.map_exports.rx.try_recv() {
            self.map_exports.running = false;
            let text = match outcome {
                Outcome::Saved(name) => tr!("combat-editor-file-saved", name = name).to_owned(),
                Outcome::Failed => tr!("combat-editor-file-failed").to_owned(),
            };
            self.world.system_message(text);
        }
    }

    pub(super) fn export_marker_map(&mut self, job: ui::combat_editor::ExportJob) {
        if self.map_exports.running {
            return;
        }
        let Some(region) = self.world.main() else {
            self.world.system_message(tr!("tactical-set-region-required"));
            return;
        };
        let side_m = f64::from(region.heightmap.size_x.max(region.heightmap.size_y));
        let (ox, oy) = aurora_net::handle_to_origin(region.handle);
        let region_name = region.name.clone();
        let set = self.settings.maps.tactical.active_set_name();
        let stem: String = format!("{region_name} {set}")
            .trim()
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
            .collect();
        // the dialog first: a cancel costs nothing
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("combat-editor-export-image"))
            .set_file_name(format!("{stem}.png"))
            .add_filter("PNG", &["png"])
            .save_file()
        else {
            return;
        };
        let view = self.settings.maps.tactical.map_view;
        let Some(inputs) = ui::minimap::export_inputs(&self.world, job.side as usize, view, job.transparent, &self.settings.maps) else {
            self.world.system_message(tr!("combat-editor-file-failed"));
            return;
        };
        let work = Work {
            inputs,
            markers: self.settings.maps.tactical.draw_order().into_iter().cloned().collect(),
            region: region_name,
            set,
            origin: [f64::from(ox), f64::from(oy)],
            side_m,
            job,
            legend_heading: tr!("combat-export-legend").to_owned(),
            path,
        };
        let tx = self.map_exports.tx.clone();
        let spawned = std::thread::Builder::new().name("map-export".into()).spawn(move || {
            let _ = tx.send(work.run());
        });
        match spawned {
            Ok(_) => self.map_exports.running = true,
            Err(_) => self.world.system_message(tr!("combat-editor-file-failed")),
        }
    }
}
