use anyhow::Result;
use eframe::egui;
use termii::app::App;
use termii::languages::messages::Messages;

fn main() -> Result<()> {
    env_logger::init();

    let runtime = tokio::runtime::Runtime::new()?;
    let handle = runtime.handle().clone();
    let messages = Messages::load();
    let app_title = messages.get("app.title").to_string();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        app_title.as_str(),
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, handle)))),
    )
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    drop(runtime);
    Ok(())
}
