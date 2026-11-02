mod active;
mod app;
mod backup;
mod board;
mod demo;
mod fonts;
mod licenses;
#[cfg(target_os = "macos")]
mod macos_menu;
mod model;
mod repeat;
mod storage;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let config = match storage::Config::load_or_create() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Priogator: cannot read settings: {e}");
            std::process::exit(1);
        }
    };

    if std::env::args().any(|a| a == "--demo") {
        let mut folder = storage::SyncFolder::new(&config.sync_dir, &config.device);
        let (mut board, _) = folder.load();
        demo::fill(&mut board, jiff::Zoned::now().date());
        if let Err(e) = folder.save(&board) {
            eprintln!("Priogator: cannot write demo data: {e}");
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Priogator")
            .with_app_id("priogator")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .expect("icon is a valid PNG"),
            )
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([700.0, 400.0]),
        // fonts::install sets up the fallback for missing characters itself.
        system_font_fallback: false,
        ..Default::default()
    };
    eframe::run_native(
        "Priogator",
        options,
        Box::new(|cc| {
            #[cfg(target_os = "macos")]
            macos_menu::install(&cc.egui_ctx);
            Ok(Box::new(app::PriogatorApp::new(&cc.egui_ctx, config)))
        }),
    )
}
