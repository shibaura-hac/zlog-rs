//! Japanese text in egui, with a font of the system instead of one embedded.
//!
//! egui's own fonts have no kana or kanji. At start-up the fonts installed on the
//! computer are searched for a Japanese family, in the order below, and the first
//! one found is added to every egui family: first for Japanese, so that kanji take
//! Japanese glyph forms (not Chinese ones from a CJK font for another language), and
//! after egui's fonts otherwise, so that Latin text keeps egui's look and widths.
//! Nothing is redistributed: the font is read from the user's own system.

use std::sync::{Arc, OnceLock};

use egui::{FontData, FontDefinitions, FontFamily};

/// Japanese families by preference: macOS, Windows, then common free fonts.
const FAMILIES: &[&str] = &[
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Yu Gothic UI",
    "Yu Gothic",
    "Meiryo UI",
    "Meiryo",
    "MS UI Gothic",
    "MS Gothic",
    "Noto Sans CJK JP",
    "Noto Sans JP",
    "Source Han Sans JP",
    "IPAexGothic",
    "IPAGothic",
];

const NAME: &str = "japanese";

/// The font found at start-up; the search runs once.
static FOUND: OnceLock<Option<FontData>> = OnceLock::new();

/// Add a Japanese font of the system to egui, ordered for the current language;
/// called again when the language changes. Without one, Japanese shows as boxes.
pub fn install(ctx: &egui::Context) {
    let Some(font) = FOUND.get_or_init(find) else {
        return;
    };
    ctx.set_fonts(definitions(font.clone(), crate::i18n::current() == "ja"));
}

/// The first Japanese family installed, as egui font data (with its index in a
/// font collection such as a `.ttc` file).
fn find() -> Option<FontData> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    FAMILIES.iter().find_map(|family| {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight::NORMAL,
            ..Default::default()
        };
        let id = db.query(&query)?;
        db.with_face_data(id, |data, index| {
            let mut font = FontData::from_owned(data.to_vec());
            font.index = index;
            font
        })
    })
}

/// egui's fonts with `font` added to every family, first when the user interface
/// is in Japanese.
fn definitions(font: FontData, japanese_first: bool) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(NAME.into(), Arc::new(font));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let list = fonts.families.entry(family.clone()).or_default();
        // Digits and Latin letters of the monospace family stay egui's, so the
        // columns of the QSO table line up in either language.
        if japanese_first && family == FontFamily::Proportional {
            list.insert(0, NAME.into());
        } else {
            list.push(NAME.into());
        }
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy() -> FontData {
        FontData::from_static(&[])
    }

    #[test]
    fn japanese_first_only_for_proportional_in_japanese() {
        let f = definitions(dummy(), true);
        assert_eq!(f.families[&FontFamily::Proportional][0], NAME);
        assert_eq!(f.families[&FontFamily::Monospace].last().unwrap(), NAME);
        let f = definitions(dummy(), false);
        assert_eq!(f.families[&FontFamily::Proportional].last().unwrap(), NAME);
        assert_ne!(f.families[&FontFamily::Proportional][0], NAME);
    }
}
