//! App icons: bundled for the known Crafting Apps (instant, works offline), fetched from the
//! tool's repository for anything discovered later.

use getcraft_core::catalog::Tool;

macro_rules! bundled {
    ($($id:literal),* $(,)?) => {
        fn bundled(id: &str) -> Option<&'static [u8]> {
            match id {
                $($id => Some(include_bytes!(concat!("../../../assets/icons/", $id, ".png"))),)*
                _ => None,
            }
        }
    };
}

bundled!(
    "photocraft",
    "vectorcraft",
    "filmcraft",
    "lightcraft",
    "pdfcraft",
    "effectcraft",
    "designcraft",
    "gridcraft",
    "cadcraft",
    "deckcraft",
    "soundcraft",
    "wordcraft",
);

pub fn image(tool: &Tool) -> egui::Image<'static> {
    match bundled(&tool.id) {
        Some(bytes) => egui::Image::from_bytes(format!("bytes://icon/{}.png", tool.id), bytes),
        None => egui::Image::from_uri(tool.icon_url()),
    }
}
