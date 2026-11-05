//! The building blocks of the Combat window's pages: a titled card, a
//! switch row, a slider row, a pill, two columns that stack when narrow.
//! Every tab is made of these, so the window reads as one dashboard.
use super::widgets;
use crate::theme::Palette;
use egui::{Color32, RichText, Vec2, vec2};

/// Width from which the cards of a page sit in two columns.
pub(crate) const TWO_COLUMNS: f32 = 760.0;
/// Width of a slider's rail in a card.
const SLIDER_W: f32 = 150.0;

/// A titled block: icon, title, an optional count, and, once something in
/// it differs from its default, a button that puts the defaults back.
pub(crate) struct Card<'a> {
    icon: &'a str,
    title: &'a str,
    badge: Option<String>,
    help: &'a str,
    modified: bool,
}

impl<'a> Card<'a> {
    pub fn new(icon: &'a str, title: &'a str) -> Self {
        Self {
            icon,
            title,
            badge: None,
            help: "",
            modified: false,
        }
    }
    pub fn badge(mut self, text: impl Into<String>) -> Self {
        self.badge = Some(text.into());
        self
    }
    /// One line of explanation under the title.
    pub fn help(mut self, text: &'a str) -> Self {
        self.help = text;
        self
    }
    /// Something here differs from the default: the reset button shows.
    pub fn modified(mut self, yes: bool) -> Self {
        self.modified = yes;
        self
    }
    /// Draws the card; true when its reset button was pressed (the page
    /// puts the card's settings back).
    pub fn show(self, ui: &mut egui::Ui, p: &Palette, body: impl FnOnce(&mut egui::Ui)) -> bool {
        let mut reset = false;
        widgets::surface(ui, p, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if let Some(picture) = super::icons::global(self.icon) {
                    ui.add(egui::Image::new((picture.id(), Vec2::splat(16.0))).tint(p.muted));
                }
                ui.label(RichText::new(self.title).strong().size(14.0).color(p.ink));
                if let Some(text) = &self.badge {
                    badge(ui, p, text, p.violet);
                }
                if self.modified {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        reset = widgets::icon_button(ui, p, "restore", aurora_i18n::tr!("combat-card-reset"), true);
                    });
                }
            });
            if !self.help.is_empty() {
                muted(ui, p, self.help);
            }
            ui.add_space(6.0);
            body(ui);
        });
        reset
    }
}

pub(crate) fn muted(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(11.5).color(p.muted)).wrap());
}

/// A framed warning line.
pub(crate) fn notice(ui: &mut egui::Ui, p: &Palette, text: &str) {
    widgets::surface(ui, p, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(text).color(p.warn));
    });
}

/// A small rounded label.
pub(crate) fn badge(ui: &mut egui::Ui, p: &Palette, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), p.ink);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(12.0, 4.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, ui.visuals().widgets.inactive.corner_radius, color.gamma_multiply(0.25));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.ink);
}

/// A switch shown as a pill with a state dot (Sending, Receiving).
pub(crate) fn pill(ui: &mut egui::Ui, p: &Palette, on: &mut bool, text: &str, tip: &str) {
    let color = if *on { p.success } else { p.muted_dim };
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, if *on { p.ink } else { p.muted });
    let (dot, gap, pad) = (4.0, 6.0, egui::vec2(10.0, 5.0));
    let size = egui::vec2(pad.x * 2.0 + dot * 2.0 + gap + galley.size().x, pad.y * 2.0 + galley.size().y);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    let face = if resp.hovered() { p.raised } else { p.raised.gamma_multiply(0.7) };
    ui.painter().rect_filled(rect, ui.visuals().widgets.inactive.corner_radius, face);
    ui.painter()
        .circle_filled(egui::pos2(rect.left() + pad.x + dot, rect.center().y), dot, color);
    ui.painter().galley(
        egui::pos2(rect.left() + pad.x + dot * 2.0 + gap, rect.center().y - galley.size().y / 2.0),
        galley,
        p.ink,
    );
    if resp.clicked() {
        *on = !*on;
    }
}

/// A small heading that splits a long card into groups.
pub(crate) fn section(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).size(11.5).strong().color(p.muted));
    ui.add_space(2.0);
}

/// A switch and its label; `help` shows under the label when given.
pub(crate) fn switch_row(ui: &mut egui::Ui, p: &Palette, on: &mut bool, label: &str, help: &str) -> bool {
    let before = *on;
    ui.horizontal(|ui| {
        widgets::switch(ui, p, on);
        ui.vertical(|ui| {
            let text = ui.add(egui::Label::new(RichText::new(label).color(p.ink)).sense(egui::Sense::click()));
            if text.clicked() {
                *on = !*on;
            }
            if !help.is_empty() {
                muted(ui, p, help);
            }
        });
    });
    ui.add_space(2.0);
    *on != before
}

/// A label on the left, its control on the right; `help` is on hover.
pub(crate) fn field_row(ui: &mut egui::Ui, label: &str, help: &str, body: impl FnOnce(&mut egui::Ui) -> bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let text = ui.label(label);
        if !help.is_empty() {
            text.on_hover_text(help);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = body(ui);
        });
    });
    ui.add_space(2.0);
    changed
}

/// A slider row, in the preferences' look (rail, trailing fill, number).
pub(crate) fn slider_row<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    help: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    suffix: &str,
) -> bool {
    field_row(ui, label, help, |ui| {
        ui.spacing_mut().slider_width = SLIDER_W;
        ui.add(
            egui::Slider::new(value, range)
                .suffix(suffix)
                .custom_parser(aurora_i18n::parse_number)
                .trailing_fill(true),
        )
        .changed()
    })
}

/// A colour row.
pub(crate) fn color_row(ui: &mut egui::Ui, p: &Palette, id: &str, label: &str, value: &mut [u8; 4], alpha: bool) -> bool {
    field_row(ui, label, "", |ui| widgets::color_button(ui, p, id, value, alpha).changed())
}

/// A one-line text box row (an asset UUID).
pub(crate) fn text_row(ui: &mut egui::Ui, label: &str, help: &str, value: &mut String) -> bool {
    field_row(ui, label, help, |ui| {
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(240.0)
                .font(egui::TextStyle::Monospace),
        )
        .changed()
    })
}

/// The rows behind a switch: greyed while it is off, set in a little.
pub(crate) fn under(ui: &mut egui::Ui, enabled: bool, body: impl FnOnce(&mut egui::Ui) -> bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(26.0);
        ui.vertical(|ui| {
            ui.add_enabled_ui(enabled, |ui| {
                changed = body(ui);
            });
        });
    });
    changed
}

/// Two columns of cards when the room allows, one after the other when
/// not. Both get `state`, which two closures could not both borrow.
pub(crate) fn columns<T>(
    ui: &mut egui::Ui,
    state: &mut T,
    left: impl FnOnce(&mut egui::Ui, &mut T),
    right: impl FnOnce(&mut egui::Ui, &mut T),
) {
    if ui.available_width() >= TWO_COLUMNS {
        ui.columns(2, |cols| {
            cols[0].vertical(|ui| left(ui, state));
            cols[1].vertical(|ui| right(ui, state));
        });
    } else {
        left(ui, state);
        ui.add_space(10.0);
        right(ui, state);
    }
}

/// The gap between two stacked cards.
pub(crate) fn gap(ui: &mut egui::Ui) {
    ui.add_space(10.0);
}
