// disable console when compiling in release
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"), 
    windows_subsystem = "windows"
)]

use app::app::App;
use gui_app::app::GuiApp;
use std::path::Path;
use std::sync::Arc;

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

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
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

    let native_options = eframe::NativeOptions::default();
    let app = App::new(config_path.as_str()).await;
    
    tokio::task::block_in_place(move || {
        eframe::run_native(
            "Torrent Renamer", 
            native_options, 
            Box::new({
                let root_path = root_path.clone();
                move |_| {
                    let app = match app {
                        Ok(app) => Arc::new(app),
                        Err(err) => {
                            let message = format!("Failed to create application: {}", err);
                            return Box::new(FailedGuiApp::new(message));
                        },
                    };

                    tokio::spawn({
                        let app = app.clone();
                        async move {
                            let (res_0, res_1) = tokio::join!(
                                app.load_folders(root_path),
                                app.login(),
                            );
                            res_0.or(res_1)
                        }
                    });

                    let gui = GuiApp::new(app);
                    Box::new(gui)
                }
            }),
        )
    })
}
