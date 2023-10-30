use app::app::App;
use std::sync::Arc;
use eframe;
use egui;
use tokio;
use crate::helpers::render_invisible_width_widget;
use crate::error_list::render_errors_list;
use crate::settings_menu::{GuiSettings, render_settings_menu};
use crate::app_folders_list::{GuiAppFoldersList, render_folders_list};
use crate::app_folder::{GuiAppFolder, render_app_folder};
use crate::app_series_search::{GuiSeriesSearch, render_series_search};

pub struct GuiApp {
    pub(crate) app: Arc<App>,
    pub(crate) gui_app_folders_list: GuiAppFoldersList,
    pub(crate) gui_app_folder: GuiAppFolder,
    pub(crate) gui_series_search: GuiSeriesSearch,
    gui_settings: GuiSettings,

    is_folder_busy_check_thread_spawned: bool,
    is_gui_settings_opened: bool,
}

impl GuiApp {
    pub fn new(app: Arc<App>) -> Self {
        Self {
            app,
            gui_app_folders_list: GuiAppFoldersList::new(),
            gui_app_folder: GuiAppFolder::new(),
            gui_series_search: GuiSeriesSearch::new(),
            gui_settings: GuiSettings::new(),
            is_folder_busy_check_thread_spawned: false,
            is_gui_settings_opened: false,
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.gui_settings.update_frame(ctx, frame);

        // Create a thread that refreshes ui when folders are updated
        if !self.is_folder_busy_check_thread_spawned {
            self.is_folder_busy_check_thread_spawned = true;
            let ctx = ctx.clone();
            let app = self.app.clone();
            tokio::spawn(async move {
                let mut old_busy_count = None;
                loop {
                    let folders = app.get_folders().read().await;
                    let mut total_busy_folders = 0;
                    for folder in folders.iter() {
                        if folder.get_busy_lock().try_lock().is_err() {
                            total_busy_folders += 1;
                        }
                    }
                    drop(folders);

                    let is_refresh = old_busy_count != Some(total_busy_folders);
                    old_busy_count = Some(total_busy_folders);
                    if is_refresh {
                        ctx.request_repaint();
                    }
                    let duration = tokio::time::Duration::from_millis(100);
                    tokio::time::sleep(duration).await;
                }
            });
        }

        egui::SidePanel::left("Folders")
            .resizable(true)
            .show(ctx, |ui| {
                render_invisible_width_widget(ui);
                if let Ok(mut errors) = self.app.get_errors().try_write() {
                    if !errors.is_empty() {
                        egui::TopBottomPanel::bottom("app_error_list")
                            .resizable(true)
                            .show_inside(ui, |ui| {
                                render_errors_list(ui, errors.as_mut());
                            });
                    }
                } 
                egui::CentralPanel::default()
                    .frame(egui::Frame::none())
                    .show_inside(ui, |ui| {
                        render_folders_list(ui, &mut self.gui_app_folders_list, &self.app, &mut self.is_gui_settings_opened);
                    });
            });

        egui::CentralPanel::default()
            .show(ctx, |ui| {
                let folders = self.app.get_folders().blocking_read();
                let folder_index = *self.app.get_selected_folder_index().blocking_read();
                let folder_index = match folder_index {
                    Some(index) => index,
                    None => {
                        ui.label("No folder selected");
                        return;
                    },
                };

                let folder = folders[folder_index].clone();
                drop(folders);

                let session = self.app.get_login_session().blocking_read();
                render_app_folder(ui, session.as_ref(), &mut self.gui_app_folder, &folder);
            });

        egui::Window::new("Series Search")
            .collapsible(false)
            .vscroll(false)
            .open(&mut self.gui_app_folder.is_show_series_search)
            .show(ctx, |ui| {
                render_series_search(ui, &mut self.gui_series_search, &self.app);
            });
        
        egui::Window::new("Settings Menu")
            .collapsible(false)
            .vscroll(true)
            .hscroll(true)
            .open(&mut self.is_gui_settings_opened)
            .show(ctx, |ui| {
                render_settings_menu(ui, ctx, &mut self.gui_settings);
            });
    }
}

