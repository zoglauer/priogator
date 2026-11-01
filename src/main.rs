mod active;
mod app;
mod backup;
mod board;
mod demo;
mod fonts;
mod model;
mod repeat;
mod storage;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let config = match storage::Config::load_or_create() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Prioritizer: cannot read settings: {e}");
            std::process::exit(1);
        }
    };

    if std::env::args().any(|a| a == "--demo") {
        let mut folder = storage::SyncFolder::new(&config.sync_dir, &config.device);
        let (mut board, _) = folder.load();
        demo::fill(&mut board, jiff::Zoned::now().date());
        if let Err(e) = folder.save(&board) {
            eprintln!("Prioritizer: cannot write demo data: {e}");
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Prioritizer")
            .with_app_id("prioritizer")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .expect("icon is a valid PNG"),
            )
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([700.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Prioritizer",
        options,
        Box::new(|cc| Ok(Box::new(app::PrioritizerApp::new(&cc.egui_ctx, config)))),
    )
}
