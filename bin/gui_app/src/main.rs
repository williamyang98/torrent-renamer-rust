use app::app::App;
use std::path::Path;
use std::sync::Arc;
use gui_app::app::GuiApp;

struct FailedGuiApp {
    message: String,
}

impl FailedGuiApp {
    fn new(message: String) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl eframe::App for FailedGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .show(ctx, |ui| {
                let label = egui::RichText::new(self.message.as_str()).color(egui::Color32::DARK_RED);
                ui.heading(label);
            });
    }
}

fn print_usage() {
    println!("Usage: gui_app <folder_path> [config_path]");
}

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        print_usage();
        return Ok(());
    };

    if args.contains(&"--help".to_owned()) || args.contains(&"-h".to_owned()) {
        print_usage();
        return Ok(());
    }
    
    let root_path = &args[1];
    let default_config_path = Path::new("./res").to_string_lossy().to_string();
    let config_path = args.get(2).unwrap_or(&default_config_path);

    let native_options = eframe::NativeOptions { 
        maximized: true, 
        ..Default::default() 
    };

    eframe::run_native(
        "Torrent Renamer", 
        native_options, 
        Box::new({
            let root_path = root_path.clone();
            let config_path = config_path.clone();
            move |_| {
                let runtime = match tokio::runtime::Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        let message = format!("Failed to create tokio runtime: {}", err);
                        return Box::new(FailedGuiApp::new(message));
                    },
                };

                let app = match runtime.block_on(App::new(config_path.as_str())) {
                    Ok(app) => Arc::new(app),
                    Err(err) => {
                        let message = format!("Failed to create application: {}", err);
                        return Box::new(FailedGuiApp::new(message));
                    },
                };

                runtime.spawn({
                    let app = app.clone();
                    async move {
                        let (res_0, res_1) = tokio::join!(
                            app.load_folders(root_path),
                            app.login(),
                        );
                        res_0.or(res_1)
                    }
                });

                let gui = GuiApp::new(app, runtime);
                Box::new(gui)
            }
        }),
    )
}
