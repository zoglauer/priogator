//! Text colours of macOS (AppKit's label colours) for both light and dark mode.
//! egui's own text is a medium grey, noticeably lighter than in native apps.

use eframe::egui::{self, Color32, Theme};

/// AppKit `labelColor`: 85 % black (light) / 85 % white (dark).
const LABEL_ALPHA: f32 = 0.85;
/// AppKit `secondaryLabelColor`.
const SECONDARY_ALPHA_LIGHT: f32 = 0.50;
const SECONDARY_ALPHA_DARK: f32 = 0.55;

pub fn install(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        let base = match theme {
            Theme::Light => Color32::BLACK,
            Theme::Dark => Color32::WHITE,
        };
        let secondary = match theme {
            Theme::Light => SECONDARY_ALPHA_LIGHT,
            Theme::Dark => SECONDARY_ALPHA_DARK,
        };
        ctx.style_mut_of(theme, |style| {
            let visuals = &mut style.visuals;
            let label = base.gamma_multiply(LABEL_ALPHA);
            // Plain text, and the text on buttons, checkboxes etc. at rest.
            visuals.widgets.noninteractive.fg_stroke.color = label;
            visuals.widgets.inactive.fg_stroke.color = label;
            visuals.weak_text_color = Some(base.gamma_multiply(secondary));
        });
    }
}
