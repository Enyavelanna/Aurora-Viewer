//! The annotated map as a PNG (the region editor's Set tab): the terrain,
//! the markers drawn over it, a title, a legend of the pins, the scale and
//! the north. The save dialog asks where to put it first, so that a cancel
//! costs nothing; then the picture is made and written on a worker, from
//! copies of what it needs, and the frame loop never waits for it. One
//! picture at a time: the editor's button is off while one is being made.
use super::*;
use crate::combat::export::{self, Sheet};
use aurora_i18n::tr;

impl App {
    pub(super) fn export_marker_map(&mut self, job: ui::combat_editor::ExportJob) {
        if self.map_export.is_some() {
            return;
        }
        let Some(region) = self.world.main() else {
            self.world.system_message(tr!("tactical-set-region-required"));
            return;
        };
        let side_m = f64::from(region.heightmap.size_x.max(region.heightmap.size_y));
        let (ox, oy) = aurora_net::handle_to_origin(region.handle);
        let region_name = region.name.clone();
        let view = self.settings.maps.tactical.map_view;
        let Some(inputs) = ui::minimap::export_inputs(&self.world, view, job.transparent, &self.settings.maps) else {
            self.world.system_message(tr!("combat-editor-file-failed"));
            return;
        };
        let cfg = &self.settings.maps.tactical;
        let set = cfg.active_set_name();
        let heading = tr!("combat-export-legend");
        let sheet = Sheet {
            region: &region_name,
            set: &set,
            origin: [f64::from(ox), f64::from(oy)],
            size_m: side_m,
            side: job.side,
            title: job.title,
            legend: job.legend,
            legend_heading: heading,
        };
        let svg = export::svg(&cfg.draw_order(), &sheet);
        let stem: String = format!("{region_name} {set}")
            .trim()
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
            .collect();
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("combat-editor-export-image"))
            .set_file_name(format!("{stem}.png"))
            .add_filter("PNG", &["png"])
            .save_file()
        else {
            return;
        };
        // the messages are made here: the worker only picks one
        let saved = tr!("combat-editor-file-saved", name = path.display().to_string()).to_owned();
        let failed = tr!("combat-editor-file-failed").to_owned();
        let fonts = ui::fonts::export_faces();
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("aurora-map-export".into()).spawn(move || {
            let started = std::time::Instant::now();
            let base = ui::minimap::export_rasters(&inputs, job.side as usize);
            let rasters_ms = started.elapsed().as_millis();
            let done = export::render(&base, &svg, &fonts, job.side, job.transparent)
                .ok()
                .and_then(|png| std::fs::write(&path, png).ok())
                .is_some();
            log::info!(
                "map export: {} px, rasters {rasters_ms} ms, total {} ms, {}",
                job.side,
                started.elapsed().as_millis(),
                if done { "saved" } else { "failed" }
            );
            let _ = tx.send(if done { saved } else { failed });
        });
        match spawned {
            Ok(_) => self.map_export = Some(rx),
            Err(_) => self.world.system_message(tr!("combat-editor-file-failed")),
        }
    }

    /// The worker's word, once the picture is written (or not).
    pub(super) fn poll_map_export(&mut self) {
        let Some(rx) = &self.map_export else {
            return;
        };
        match rx.try_recv() {
            Ok(text) => {
                self.map_export = None;
                self.world.system_message(text);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.map_export = None;
                self.world.system_message(tr!("combat-editor-file-failed"));
            }
        }
    }
}
