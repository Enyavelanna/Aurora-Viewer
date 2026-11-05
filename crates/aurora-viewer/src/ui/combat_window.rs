//! Dedicated Combat workspace, using Aurora's shared floater and preference rows.
use super::{options, widgets};
use crate::{settings::Settings, theme::Palette};
use aurora_i18n::tr;

pub const WINDOW: &str = "combat_workspace";
/// The Allies tab, which the Sharing tab sends you to.
pub(crate) const ALLIES_TAB: usize = 7;

#[derive(Default)]
pub(crate) struct CombatWindow {
    pub tab: usize,
    controls: options::OptionsUi,
    editor: super::combat_editor::Editor,
}

impl CombatWindow {
    pub fn editor_open(&self) -> bool {
        self.editor.open
    }
    pub fn map_images(&mut self, images: Vec<super::minimap::RegionImage>) {
        self.editor.images = images;
    }
    pub fn open_editor(&mut self) {
        self.editor.open = true;
    }
    /// A picture of the map the person asked for, once.
    pub fn take_export(&mut self) -> Option<super::combat_editor::ExportJob> {
        self.editor.take_export()
    }
    pub fn show_editor_map(&mut self) {
        self.editor.open = true;
        self.editor.show_map();
    }
    pub fn show_editor_layers(&mut self) {
        self.editor.open = true;
        self.editor.show_layers();
    }
    pub fn show_editor_set(&mut self) {
        self.editor.open = true;
        self.editor.show_set();
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        settings: &mut Settings,
        world: &crate::world::World,
        images: &std::collections::HashMap<uuid::Uuid, egui::TextureHandle>,
        open: &mut bool,
    ) -> options::OptionsResult {
        let mut result = options::OptionsResult::default();
        if !*open {
            self.editor.show(ctx, p, &mut settings.maps.tactical, world);
            return result;
        }
        self.controls.tactical_context = crate::combat::tactical::Context::from_world(world, &settings.maps.tactical);
        self.controls.combat_medal_images.clear();
        for asset in &settings.combat.medal_textures {
            if let Ok(id) = uuid::Uuid::parse_str(asset)
                && let Some(image) = images.get(&id)
            {
                self.controls.combat_medal_images.insert(id, image.clone());
            }
        }
        let room = ctx.content_rect();
        let size = egui::vec2(1100.0, 740.0).min(room.size() - egui::vec2(40.0, 80.0));
        widgets::Floater::new(WINDOW, tr!("combat-title"), room.min + egui::vec2(20.0, 50.0), size)
            .steady_size()
            .show(ctx, p, open, |ui| {
                result.changed |= widgets::switch_labeled(ui, p, &mut settings.combat.enabled, tr!("combat-mode")).changed();
                widgets::divider(ui, p);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        for (tab, (icon, label)) in [
                            ("layout-dashboard-filled", tr!("monitor-tab-overview")),
                            ("bolt-filled", tr!("combat-page-feedback")),
                            ("star-filled", tr!("combat-page-rewards")),
                            ("map", tr!("combat-page-minimap")),
                            ("map-pin-filled", tr!("combat-page-sets")),
                            ("flag-filled", tr!("combat-page-pings")),
                            ("share", tr!("combat-page-sharing")),
                            ("heart-handshake", tr!("combat-page-allies")),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            if widgets::pill_tab_wide(ui, p, icon, label, self.tab == tab, 170.0, true, "").clicked() {
                                self.tab = tab;
                            }
                        }
                    });
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        if self.tab == 4
                            && widgets::action_button(
                                ui,
                                p,
                                "pencil-filled",
                                tr!("combat-editor-open"),
                                widgets::ActionKind::Primary,
                                egui::vec2(0.0, 30.0),
                            )
                            .clicked()
                        {
                            self.editor.open = true;
                        }
                        egui::ScrollArea::vertical().id_salt((WINDOW, self.tab)).show(ui, |ui| {
                            // The preference row engine, native cogs included, draws the cards.
                            let mut r = options::OptionsResult::default();
                            result.changed |= super::combat_pages::show(ui, p, settings, &mut self.controls, &mut r, self.tab);
                            result.combat_preview = r.combat_preview;
                        });
                        if let Some(tab) = self.controls.combat_goto.take() {
                            self.tab = tab;
                        }
                    });
                });
            });
        self.editor.show(ctx, p, &mut settings.maps.tactical, world);
        result
    }
}
