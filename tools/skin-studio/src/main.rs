fn main() -> eframe::Result {
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        let result = (|| {
            let home = ddi_skin_studio::mcp::project_home()?;
            tokio::runtime::Runtime::new()
                .map_err(|e| e.to_string())?
                .block_on(ddi_skin_studio::mcp::SkinCraftMcp::new(home).serve_stdio())
        })();
        if let Err(error) = result {
            eprintln!("SkinCraft MCP: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    eframe::run_native(
        "SkinCraft",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("SkinCraft")
                .with_app_id("dev.sugoijan.skincraft")
                .with_inner_size([1600.0, 1000.0])
                .with_min_inner_size([1000.0, 700.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            ddi_skin_studio::SkinCraft::new(cc)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(Into::into)
        }),
    )
}
