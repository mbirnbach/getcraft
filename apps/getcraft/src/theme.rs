//! Colours and widget styling: a dark, card-based look in the spirit of app-store launchers.

use egui::{Color32, CornerRadius, FontId, Stroke, TextStyle, Theme, Vec2, vec2};

pub const BG: Color32 = Color32::from_rgb(0x13, 0x13, 0x14);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1c);
pub const CARD: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x22);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x25, 0x25, 0x29);
pub const INPUT: Color32 = Color32::from_rgb(0x26, 0x26, 0x2a);
pub const BORDER: Color32 = Color32::from_rgb(0x30, 0x30, 0x35);
pub const TEXT: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf4);
pub const MUTED: Color32 = Color32::from_rgb(0xa0, 0xa0, 0xa8);
pub const FAINT: Color32 = Color32::from_rgb(0x6c, 0x6c, 0x74);
pub const ACCENT: Color32 = Color32::from_rgb(0x4f, 0x8c, 0xff);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x1f, 0x33, 0x5c);
pub const SUCCESS: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);

pub const RADIUS: u8 = 10;
pub const PILL: u8 = 15;

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(Theme::Dark);
    ctx.style_mut_of(Theme::Dark, |style| {
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(21.0));
        style.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(14.0, 6.0);
        style.spacing.interact_size.y = 28.0;

        let v = &mut style.visuals;
        v.panel_fill = BG;
        v.window_fill = SIDEBAR;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(8);
        v.extreme_bg_color = INPUT;
        v.faint_bg_color = CARD;
        v.override_text_color = None;
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = ACCENT_SOFT;
        v.selection.stroke = Stroke::new(1.0, TEXT);

        let radius = CornerRadius::same(8);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = radius;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.inactive.weak_bg_fill = INPUT;
        v.widgets.inactive.bg_fill = INPUT;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.hovered.weak_bg_fill = CARD_HOVER;
        v.widgets.hovered.bg_fill = CARD_HOVER;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
        v.widgets.hovered.fg_stroke = Stroke::new(1.5, TEXT);
        v.widgets.active.weak_bg_fill = BORDER;
        v.widgets.active.fg_stroke = Stroke::new(1.5, TEXT);
    });
}

/// Outlined pill button, the default action style ("Install", "Open").
pub fn pill(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_owned()).strong().color(TEXT))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.5, TEXT))
        .corner_radius(PILL)
        .min_size(Vec2::new(78.0, 30.0))
}

/// Filled pill button for the action we want people to take ("Update").
pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_owned()).strong().color(Color32::WHITE))
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .corner_radius(PILL)
        .min_size(Vec2::new(78.0, 30.0))
}

/// Solid white pill used for prominent panel actions.
pub fn light(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_owned()).strong().color(BG))
        .fill(TEXT)
        .corner_radius(PILL)
        .min_size(Vec2::new(0.0, 32.0))
}
