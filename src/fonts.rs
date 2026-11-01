//! Use the operating system's UI fonts instead of egui's built-in ones:
//! San Francisco on macOS, the desktop's configured sans-serif on Linux.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use fontique::{
    Collection, CollectionOptions, GenericFamily, QueryFamily, QueryStatus, SourceCache,
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

/// Install the system fonts and macOS-like text sizes. If a font can't be
/// found, egui's built-in one stays in place for that family.
pub fn install(ctx: &egui::Context) {
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
    for (family, preferences) in families {
        let Some(mut data) = find_font(&mut collection, &mut cache, preferences) else {
            continue;
        };
        tweak_like_native(&mut data);
        if family == FontFamily::Proportional {
            bold_font = Some(data.clone());
        }
        let name = format!("system-{family:?}");
        fonts.font_data.insert(name.clone(), data.into());
        // First choice; egui's own fonts stay behind it for missing characters.
        fonts.families.entry(family).or_default().insert(0, name);
    }

    // Bold: the same font at weight 700, where it's a variable font with a
    // weight axis (San Francisco is); the regular fonts behind it.
    let mut bold_family = fonts.families[&FontFamily::Proportional].clone();
    if let Some(mut data) = bold_font
        && data
            .variation_axes()
            .iter()
            .any(|axis| axis.tag.to_be_bytes() == *b"wght")
    {
        data.tweak.coords.push("wght", 700.0);
        fonts.font_data.insert("system-bold".into(), data.into());
        bold_family.insert(0, "system-bold".into());
    }
    fonts
        .families
        .insert(FontFamily::Name(BOLD.into()), bold_family);
    ctx.set_fonts(fonts);

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

/// The regular-weight font of the first of `families` the system has.
fn find_font<'a>(
    collection: &mut Collection,
    cache: &mut SourceCache,
    families: Vec<QueryFamily<'a>>,
) -> Option<FontData> {
    let mut query = collection.query(cache);
    query.set_families(families);
    let mut found = None;
    query.matches_with(|font| {
        found = Some(font.clone());
        QueryStatus::Stop
    });
    let font = found?;
    let (blob, _id) = font.blob.into_raw_parts();
    Some(FontData::from_blob(blob, font.index))
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, RichText};

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
