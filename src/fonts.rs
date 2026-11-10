//! Use the operating system's UI fonts instead of egui's built-in ones:
//! San Francisco on macOS, the desktop's configured sans-serif on Linux.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use fontique::{
    Attributes, Collection, CollectionOptions, FamilyId, FontStyle, FontWeight, FontWidth,
    GenericFamily, QueryFamily, QueryStatus, SourceCache,
};

/// Body text size in points, as in macOS.
const BODY_SIZE: f32 = 13.0;

/// Name of the bold variant of the UI font, see [`bold`].
const BOLD: &str = "bold";

/// The UI font in bold, e.g. for the app's name. Falls back to the regular
/// weight where the system font can't do bold.
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

/// Install the system fonts, the fallback for missing characters, and
/// macOS-like text sizes. If a font can't be
/// found, egui's built-in one stays in place for that family.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
    // Characters the UI font lacks (☰, emoji, CJK, …) come from whichever
    // system font has them. Done here rather than by eframe, so the tests
    // run exactly this too.
    ctx.add_font_provider(std::sync::Arc::new(
        egui_system_font_provider::SystemFontProvider::new(),
    ));

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(10.5)),
            (TextStyle::Body, FontId::proportional(BODY_SIZE)),
            (TextStyle::Button, FontId::proportional(BODY_SIZE)),
            (TextStyle::Heading, FontId::proportional(17.0)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
    });
}

/// The system's UI fonts, regular and bold, in front of egui's own.
fn definitions() -> FontDefinitions {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let mut cache = SourceCache::default();
    let mut fonts = FontDefinitions::default();

    use QueryFamily::{Generic, Named};
    let families = [
        (
            FontFamily::Proportional,
            vec![
                Generic(GenericFamily::SystemUi),
                Generic(GenericFamily::SansSerif),
            ],
        ),
        (
            FontFamily::Monospace,
            // macOS answers the generic monospace query with Courier, so ask
            // for its UI monospace fonts by name first.
            vec![
                Named("SF Mono"),
                Named("Menlo"),
                Generic(GenericFamily::UiMonospace),
                Generic(GenericFamily::Monospace),
            ],
        ),
    ];
    let mut bold_font = None;
    let mut ui_family = None;
    for (family, preferences) in families {
        let Some((mut data, id)) =
            find_font(&mut collection, &mut cache, preferences, FontWeight::NORMAL)
        else {
            continue;
        };
        tweak_like_native(&mut data);
        if family == FontFamily::Proportional {
            bold_font = Some(data.clone());
            ui_family = Some(id);
        }
        let name = format!("system-{family:?}");
        fonts.font_data.insert(name.clone(), data.into());
        // First choice; egui's own fonts stay behind it for missing characters.
        fonts.families.entry(family).or_default().insert(0, name);
    }

    // Bold: the same font at weight 700 where it's a variable font with a
    // weight axis (San Francisco, Cantarell), else the family's own bold
    // face (DejaVu, Noto Sans, Ubuntu on Linux); the regular fonts behind it.
    let mut bold_family = fonts.families[&FontFamily::Proportional].clone();
    let bold = match bold_font {
        Some(mut data)
            if data
                .variation_axes()
                .iter()
                .any(|axis| axis.tag.to_be_bytes() == *b"wght") =>
        {
            data.tweak.coords.push("wght", 700.0);
            Some(data)
        }
        _ => ui_family.and_then(|id| {
            let (mut data, _) = find_font(
                &mut collection,
                &mut cache,
                vec![QueryFamily::Id(id)],
                FontWeight::BOLD,
            )?;
            tweak_like_native(&mut data);
            Some(data)
        }),
    };
    if let Some(data) = bold {
        fonts.font_data.insert("system-bold".into(), data.into());
        bold_family.insert(0, "system-bold".into());
    }
    fonts
        .families
        .insert(FontFamily::Name(BOLD.into()), bold_family);
    fonts
}

/// Render closer to the operating system's own text drawing.
fn tweak_like_native(data: &mut FontData) {
    // Variable fonts with an optical-size axis default to some design size
    // (28 pt for San Francisco, its display design: tighter and darker).
    // The OS picks the design for the actual text size; do the same for body
    // text. San Francisco clamps this to 17, its text design.
    if let Some(opsz) = data
        .variation_axes()
        .into_iter()
        .find(|axis| axis.tag.to_be_bytes() == *b"opsz")
    {
        let size = BODY_SIZE.clamp(opsz.range.min, opsz.range.max);
        data.tweak.coords.push("opsz", size);
    }
    // macOS doesn't hint glyph outlines; hinting makes text narrower and darker.
    if cfg!(target_os = "macos") {
        data.tweak.hinting = Some(false);
    }
}

/// The font of the first of `families` the system has, in that weight, and
/// its family. Only a real face of that weight: none if it would have to be
/// emboldened artificially.
fn find_font<'a>(
    collection: &mut Collection,
    cache: &mut SourceCache,
    families: Vec<QueryFamily<'a>>,
    weight: FontWeight,
) -> Option<(FontData, FamilyId)> {
    let mut query = collection.query(cache);
    query.set_families(families);
    query.set_attributes(Attributes::new(
        FontWidth::NORMAL,
        FontStyle::Normal,
        weight,
    ));
    let mut found = None;
    query.matches_with(|font| {
        found = Some(font.clone());
        QueryStatus::Stop
    });
    let font = found?;
    if font.synthesis.embolden() {
        return None;
    }
    let (blob, _id) = font.blob.into_raw_parts();
    Some((FontData::from_blob(blob, font.index), font.family.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::RichText;

    /// Bold must really be bold, on macOS (a variable font) and on Linux
    /// (DejaVu, Noto Sans, Ubuntu: a separate bold file), not the regular
    /// weight in disguise.
    #[test]
    fn bold_is_a_real_bold_face() {
        let fonts = definitions();
        if !fonts.font_data.contains_key("system-Proportional") {
            return; // no system fonts at all: egui's own are used
        }
        let bold = &fonts.families[&FontFamily::Name(BOLD.into())];
        assert_eq!(bold.first().map(String::as_str), Some("system-bold"));
        let regular = &fonts.font_data["system-Proportional"];
        let bold = &fonts.font_data["system-bold"];
        // Either a weight set on the variable font, or another face.
        let mut heavier = regular.tweak.coords.clone();
        heavier.push("wght", 700.0);
        assert!(
            bold.tweak.coords == heavier
                || bold.bytes() != regular.bytes()
                || bold.index != regular.index,
            "bold is the regular font"
        );
    }

    /// Render sample text with the app's fonts to `$FONT_PREVIEW` (a PNG),
    /// for comparing with native macOS text rendering.
    /// Run with `FONT_PREVIEW=/tmp/egui.png cargo test font_preview -- --ignored`.
    #[test]
    #[ignore]
    fn font_preview() {
        let path = std::env::var("FONT_PREVIEW").expect("set FONT_PREVIEW to an output path");
        let mut harness = egui_kittest::Harness::builder()
            .with_size([520.0, 110.0])
            .with_pixels_per_point(2.0)
            .build_ui(|ui| {
                ui.style_mut().spacing.item_spacing.y = 8.0;
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label("Prepare kick-off workshop · Proposal for Müller GmbH 0123");
                        ui.label(
                            RichText::new(
                                "Prepare kick-off workshop · Proposal for Müller GmbH 0123",
                            )
                            .color(egui::Color32::BLACK),
                        );
                        ui.heading("Work  Private ventures  House");
                    });
                });
            });
        harness.ctx.set_theme(egui::Theme::Light);
        super::install(&harness.ctx);
        crate::theme::install(&harness.ctx);
        harness.run();
        harness.render().expect("render").save(path).expect("save");
    }
}
