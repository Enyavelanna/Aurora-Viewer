//! The annotated map as a PNG (the region editor's Set tab): the terrain,
//! the markers drawn over it, a title, a legend of the pins, the scale and
//! the north. The save dialog asks where to put it.
use super::*;
use crate::combat::export::{self, Sheet};
use aurora_i18n::tr;

impl App {
    pub(super) fn export_marker_map(&mut self, job: ui::combat_editor::ExportJob) {
        let Some(region) = self.world.main() else {
            self.world.system_message(tr!("tactical-set-region-required"));
            return;
        };
        let side_m = f64::from(region.heightmap.size_x.max(region.heightmap.size_y));
        let (ox, oy) = aurora_net::handle_to_origin(region.handle);
        let region_name = region.name.clone();
        let (objects, parcels) = self.combat_window.map_layers();
        let Some(base) = ui::minimap::export_rasters(&self.world, job.side as usize, objects, parcels, &self.settings.maps) else {
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
        let png = match export::render(&base, &svg, &ui::fonts::export_faces(), job.side) {
            Ok(png) => png,
            Err(_) => {
                self.world.system_message(tr!("combat-editor-file-failed"));
                return;
            }
        };
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
        let text = match std::fs::write(&path, png) {
            Ok(()) => tr!("combat-editor-file-saved", name = path.display().to_string()).to_owned(),
            Err(_) => tr!("combat-editor-file-failed").to_owned(),
        };
        self.world.system_message(text);
    }
}
