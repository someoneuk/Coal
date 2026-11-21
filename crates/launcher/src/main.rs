mod app;
mod inj;
mod log;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([640.0, 460.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Executor",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
