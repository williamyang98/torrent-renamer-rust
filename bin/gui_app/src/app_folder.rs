use app::app_folder::AppFolder;
use app::file_intent::Action;
use std::sync::Arc;
use tvdb::api::LoginSession;
use tokio;
use crate::fuzzy_search::FuzzySearcher;
use crate::app_folder_files_tab_list::{FileTab, render_files_tab_list};
use crate::app_folder_episode_cache_list::render_episode_cache_list;
use crate::helpers::render_invisible_width_widget;
use crate::tvdb_tables::{render_series_table, render_episode_table};
use crate::error_list::render_errors_list;

pub struct GuiAppFolder {
    searcher: FuzzySearcher,
    selected_tab: FileTab,
    is_show_episode_cache: bool,
    pub(crate) is_show_series_search: bool,
}

impl GuiAppFolder {
    pub fn new() -> Self {
        Self {
            searcher: FuzzySearcher::new(),
            selected_tab: FileTab::FileAction(Action::Complete),
            is_show_episode_cache: false,
            is_show_series_search: false,
        }
    }
}

impl Default for GuiAppFolder {
    fn default() -> Self {
        Self::new()
    }
}

fn render_folder_controls(
    ui: &mut egui::Ui, session: Option<&Arc<LoginSession>>,
    gui: &mut GuiAppFolder, folder: &Arc<AppFolder>,
) {
    let is_not_busy = folder.get_busy_lock().try_lock().is_ok();
    let is_cache_loaded = folder.get_cache().blocking_read().is_some();
    let is_logged_in = session.is_some();

    ui.horizontal(|ui| {
        ui.add_enabled_ui(is_cache_loaded && is_not_busy, |ui| {
            let res = ui.button("Update file intents");
            if res.clicked() {
                let folder = folder.clone();
                tokio::spawn(async move {
                    folder.update_file_intents().await
                });
            }
            res.on_disabled_hover_ui(|ui| {
                if !is_cache_loaded  { ui.label("Cache is unloaded"); } 
                else if !is_not_busy { ui.label("Folder is busy"); }
            });
        });

        ui.add_enabled_ui(is_not_busy, |ui| {
            let res = ui.button("Load cache from file");
            if res.clicked() {
                let folder = folder.clone();
                tokio::spawn(async move {
                    folder.load_cache_from_file().await?;
                    folder.update_file_intents().await
                });
            };
            res.on_disabled_hover_ui(|ui| {
                if !is_not_busy { ui.label("Folder is busy"); }
            });
        });
        
        ui.add_enabled_ui(is_cache_loaded && is_not_busy && is_logged_in, |ui| {
            let res = ui.button("Refresh cache from api");
            if res.clicked() {
                if let Some(session) = session {
                    tokio::spawn({
                        let folder = folder.clone();
                        let session = session.clone();
                        async move {
                            folder.refresh_cache_from_api(session).await?;
                            tokio::join!(
                                folder.update_file_intents(),
                                folder.save_cache_to_file(),
                            );
                            Some(())
                        }
                    });
                }
            }
            res.on_disabled_hover_ui(|ui| {
                if !is_cache_loaded   { ui.label("Cache is unloaded"); }
                else if !is_not_busy  { ui.label("Folder is busy"); }
                else if !is_logged_in { ui.label("Not logged in"); }
            });
        });

        ui.add_enabled_ui(is_not_busy, |ui| {
            let res = ui.button("Execute changes");
            if res.clicked() {
                let folder = folder.clone();
                tokio::spawn(async move {
                    folder.execute_file_changes().await;
                    folder.update_file_intents().await
                });
            };
            res.on_disabled_hover_ui(|ui| {
                if !is_not_busy { ui.label("Folder is busy"); }
            });
        });

        if ui.button("Load bookmarks").clicked() {
            let folder = folder.clone();
            tokio::spawn(async move {
                folder.load_bookmarks_from_file().await
            });
        }

        ui.toggle_value(&mut gui.is_show_series_search, "Search series");
        ui.add_enabled_ui(is_cache_loaded, |ui| {
            let res = ui.toggle_value(&mut gui.is_show_episode_cache, "Search episodes");
            res.on_disabled_hover_ui(|ui| {
                ui.label("Cache is unloaded");
            });
        });
    });
}

fn render_folder_info(ui: &mut egui::Ui, folder: &Arc<AppFolder>) {
    render_invisible_width_widget(ui);

    let cache = folder.get_cache().blocking_read();
    let cache = match cache.as_ref() {
        Some(cache) => cache,
        None => {
            ui.label("No cache loaded");
            return;
        },
    };
    
    ui.heading("Series");
    ui.push_id("series_table", |ui| {
        render_series_table(ui, &cache.series);
    });

    ui.separator();

    ui.heading("Episode");
    let descriptor = *folder.get_selected_descriptor().blocking_read(); 
    let key = match descriptor {
        Some(key) => key,
        None => {
            ui.label("No episode selected");
            return;
        },
    };

    let episode_index = match cache.episode_cache.get(&key) {
        Some(index) => *index,
        None => {
            ui.label("Episode not in cache");
            return;
        },
    };

    let episode = match cache.episodes.get(episode_index) {
        Some(episode) => episode,   
        None => {
            ui.colored_label(egui::Color32::DARK_RED, "Episode index out of range of episodes list");
            return;
        },
    };
    
    ui.push_id("episodes_table", |ui| {
        render_episode_table(ui, episode);
    });
}

pub fn render_app_folder(
    ui: &mut egui::Ui, session: Option<&Arc<LoginSession>>,
    gui: &mut GuiAppFolder, folder: &Arc<AppFolder>,
) {
    tokio::spawn({
        let folder = folder.clone();
        async move {
            folder.perform_initial_load().await
        }
    });

    egui::TopBottomPanel::top("folder_controls")
        .resizable(false)
        .show_inside(ui, |ui| {
            render_folder_controls(ui, session, gui, folder);
        });
    
    egui::SidePanel::right("folder_info")
        .resizable(true)
        .show_inside(ui, |ui| {
            ui.push_id("folder_info", |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    render_folder_info(ui, folder);
                });
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show_inside(ui, |ui| {
            if let Ok(mut errors) = folder.get_errors().try_write() {
                if !errors.is_empty() {
                    egui::TopBottomPanel::bottom("folder_error_list")
                        .resizable(true)
                        .show_inside(ui, |ui| {
                            render_errors_list(ui, errors.as_mut());
                        });
                }
            } 

            egui::CentralPanel::default()
                .show_inside(ui, |ui| {
                    let id = match gui.is_show_episode_cache {
                        false => "folder_file_list",
                        true => "folder_episode_cache",
                    };
                    ui.push_id(id, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if !gui.is_show_episode_cache {
                                render_files_tab_list(ui, &mut gui.selected_tab, &mut gui.searcher, folder);
                            } else {
                                render_episode_cache_list(ui, &mut gui.searcher, folder);
                            }
                        });
                    });
                });
        });
}

