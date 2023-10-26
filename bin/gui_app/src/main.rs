use app::file_intent::Action;
use tokio;
use egui;
use egui_extras;
use eframe;
use tokio::sync::RwLockReadGuard;
use app::app::App;
use app::app_folder::{AppFolder, AppFileMutableContext, ConflictTable};
use app::app_folder::{AppFileContextSetter, AppFileContextGetter};
use tvdb::models::{Series, Episode};
use std::sync::Arc;
use std::path::Path;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum FileTab {
    FileAction(Action),
    Conflicts,
}

struct GuiApp {
    app: Arc<App>,
    runtime: tokio::runtime::Runtime,
    show_series_search: bool,
    series_search: String,
    selected_tab: FileTab,
}

impl GuiApp {
    fn new(app: Arc<App>, runtime: tokio::runtime::Runtime) -> Self {
        Self {
            app,
            runtime,
            show_series_search: false,
            series_search: "".to_owned(),
            selected_tab: FileTab::FileAction(Action::Complete),
        }
    }
}

fn render_errors_list(gui: &mut GuiApp, folder: &Arc<AppFolder>, ui: &mut egui::Ui, ctx: &egui::Context) {
    let errors_guard = folder.get_errors().try_write();
    let mut errors = match errors_guard {
        Ok(errors) => errors,
        Err(_) => {
            ui.spinner();
            return;
        },
    };

    let total_items = errors.len();
    if total_items == 0 {
        ui.label("No errors");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let mut selected_index = None;
            for (index, error) in errors.iter().enumerate() {
                if ui.selectable_label(false, error.as_str()).clicked() {
                    selected_index = Some(index);
                }
            }

            if let Some(index) = selected_index {
                errors.remove(index);  
            }
        });
    });
}

fn render_files_selectable_list(
    gui: &mut GuiApp, folder: &Arc<AppFolder>, mut files: &mut AppFileMutableContext<'_>, 
    selected_action: Action,
    ui: &mut egui::Ui, ctx: &egui::Context,
) {
    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();

    let mut is_select_all = false;
    let mut is_deselect_all = false;
    ui.horizontal(|ui| {
        is_select_all = ui.button("Select all").clicked();
        is_deselect_all = ui.button("Deselect all").clicked();
    });

    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            for index in 0..files.get_total_items() {
                let action = files.get_action(index);
                if action != selected_action {
                    continue;
                }
                ui.horizontal(|ui| {
                    let mut is_enabled = files.get_is_enabled(index);
                    if ui.checkbox(&mut is_enabled, "").clicked() {
                        files.set_is_enabled(is_enabled, index);
                    }
                    if is_select_all {
                        files.set_is_enabled(true, index);
                    }
                    if is_deselect_all {
                        files.set_is_enabled(false, index);
                    }

                    let descriptor = files.get_src_descriptor(index);
                    let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                    let src = files.get_src(index);
                    if ui.selectable_label(is_selected, src).clicked() {
                        if is_selected {
                            *folder.get_selected_descriptor().blocking_write() = None;
                        } else {
                            *folder.get_selected_descriptor().blocking_write() = *descriptor;
                        }
                    }
                });
            }
        });
    });
}

fn render_files_basic_list(
    gui: &mut GuiApp, folder: &Arc<AppFolder>, mut files: &mut AppFileMutableContext<'_>, 
    selected_action: Action,
    ui: &mut egui::Ui, ctx: &egui::Context,
) {
    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            for index in 0..files.get_total_items() {
                let action = files.get_action(index);
                if action != selected_action {
                    continue;
                }
                let descriptor = files.get_src_descriptor(index);
                let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                let src = files.get_src(index);
                if ui.selectable_label(is_selected, src).clicked() {
                    if is_selected {
                        *folder.get_selected_descriptor().blocking_write() = None;
                    } else {
                        *folder.get_selected_descriptor().blocking_write() = *descriptor;
                    }
                }
            }
        });
    });
}

fn render_files_rename_list(
    gui: &mut GuiApp, folder: &Arc<AppFolder>, mut files: &mut AppFileMutableContext<'_>, 
    ui: &mut egui::Ui, ctx: &egui::Context,
) {
    let mut is_select_all = false;
    let mut is_deselect_all = false;
    ui.horizontal(|ui| {
        is_select_all = ui.button("Select all").clicked();
        is_deselect_all = ui.button("Deselect all").clicked();
    });
    
    use egui_extras::{Column, TableBuilder};

    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Center).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::LEFT))
                .column(Column::auto())
                .column(Column::auto().clip(true))
                .column(Column::auto().clip(true));
            
            let row_height = 18.0;
            table
                .header(row_height, |mut header| {
                    header.col(|ui| {});
                    header.col(|ui| { ui.strong("Source"); });
                    header.col(|ui| { ui.strong("Destination"); });
                })
                .body(|mut body| {
                    for index in 0..files.get_total_items() {
                        let action = files.get_action(index);
                        if action != Action::Rename {
                            continue;
                        }

                        if is_select_all {
                            files.set_is_enabled(true, index);
                        }
                        if is_deselect_all {
                            files.set_is_enabled(false, index);
                        }

                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                let mut is_enabled = files.get_is_enabled(index);
                                if ui.checkbox(&mut is_enabled, "").clicked() {
                                    files.set_is_enabled(is_enabled, index);
                                }
                            });
                            row.col(|ui| {
                                let is_conflict = files.get_is_conflict(index);
                                let src = files.get_src(index);
                                if is_conflict {
                                    ui.colored_label(egui::Color32::DARK_RED, src);
                                } else {
                                    ui.label(src);
                                }
                            });
                            row.col(|ui| {
                                let dst = files.get_dest(index);
                                let mut dst_tmp = dst.to_string();
                                if ui.text_edit_singleline(&mut dst_tmp).changed() {
                                    files.set_dest(dst_tmp, index);       
                                }
                            });
                        });

                    }
                });
        });
    });

}

fn render_files_conflicts_list(
    gui: &mut GuiApp, folder: &Arc<AppFolder>, 
    mut files: &mut AppFileMutableContext<'_>, table: RwLockReadGuard<ConflictTable>, 
    ui: &mut egui::Ui, ctx: &egui::Context,
) {
    let mut is_first = true;
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            for (row_id, (dest, indices)) in table.get_pending_writes().iter().enumerate() {
                let mut total_files = indices.len();
                if total_files == 0 {
                    continue;
                }
                let source_index = table.get_source_index(dest.as_str());
                if source_index.is_some() {
                    total_files += 1;
                }
                let is_conflict = total_files > 1;
                if !is_conflict {
                    continue;
                }

                ui.push_id(row_id, |ui| {
                    if !is_first {
                        ui.separator();
                    }
                    is_first = false;

                    ui.strong(dest);

                    use egui_extras::{Column, TableBuilder};
                    let row_height = 18.0;
                    let mut table = TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::auto())
                        .column(Column::auto().clip(true))
                        .column(Column::auto().clip(true));

                    table
                        .header(row_height, |mut header| {
                            header.col(|ui| {});
                            header.col(|ui| { ui.strong("Source"); });
                            header.col(|ui| { ui.strong("Destination"); });
                        })
                        .body(|mut body| {
                            if let Some(index) = source_index {
                                let src = files.get_src(*index);
                                body.row(row_height, |mut row| {
                                    row.col(|ui| {});
                                    row.col(|ui| { ui.label(src); });
                                    row.col(|ui| {});
                                });
                            }

                            for index in indices {
                                let index = *index;
                                let action = files.get_action(index); 
                                body.row(row_height, |mut row| {
                                    row.col(|ui| {
                                        if action == Action::Rename || action == Action::Delete {
                                            let mut is_enabled = files.get_is_enabled(index);
                                            if ui.checkbox(&mut is_enabled, "").clicked() {
                                                files.set_is_enabled(is_enabled, index);
                                            }
                                        }
                                    });

                                    row.col(|ui| {
                                        let src = files.get_src(index);
                                        ui.label(src);
                                    });

                                    row.col(|ui| {
                                        if action == Action::Rename {
                                            let dst = files.get_dest(index);
                                            let mut dst_tmp = dst.to_string();
                                            if ui.text_edit_singleline(&mut dst_tmp).changed() {
                                                files.set_dest(dst_tmp, index);       
                                            }
                                        }
                                    });
                                });
                            }
                        });
                });
            }
        });
    });
}

fn render_files_list(gui: &mut GuiApp, folder: &Arc<AppFolder>, ui: &mut egui::Ui, ctx: &egui::Context) {
    let table_lock = folder.get_conflict_table().try_read();
    let table_guard = match table_lock {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let total_conflicts = {
        let mut total_conflicts = 0;
        for (dest, indices) in table_guard.get_pending_writes() {
            let mut total_files = indices.len();
            if total_files == 0 {
                continue;
            }
            if table_guard.get_source_index(dest.as_str()).is_some() {
                total_files += 1;
            }
            let is_conflict = total_files > 1;
            if is_conflict {
                total_conflicts += 1;
            }
        }
        total_conflicts
    };

    static FILE_TABS: [FileTab;6] = [
        FileTab::FileAction(Action::Complete), 
        FileTab::FileAction(Action::Rename), 
        FileTab::FileAction(Action::Delete), 
        FileTab::FileAction(Action::Ignore), 
        FileTab::FileAction(Action::Whitelist), 
        FileTab::Conflicts
    ];
    ui.horizontal(|ui| {
        let old_selected_tab = gui.selected_tab;
        for tab in FILE_TABS {
            let label = match tab {
                FileTab::Conflicts => format!("Conflicts {}", total_conflicts),
                FileTab::FileAction(action) => {
                    let label = match action {
                        Action::Complete => "Complete",
                        Action::Rename => "Rename",
                        Action::Delete => "Delete",
                        Action::Ignore => "Ignore",
                        Action::Whitelist => "Whitelist",
                    };
                    let count = table_guard.get_action_count()[action];
                    format!("{} {}", label, count)
                },
            };

            let is_selected = tab == old_selected_tab;
            if ui.selectable_label(is_selected,label).clicked() {
                gui.selected_tab = tab;
            }
        }
    });
    
    let mut files = match folder.get_mut_files_try_blocking() {
        Some(files) => files,
        None => {
            ui.spinner();
            return;
        },
    };

    let total_items = files.get_total_items();
    if total_items == 0 {
        ui.label("Empty folder");
        return;
    }

    match gui.selected_tab {
        FileTab::FileAction(action) => match action {
            Action::Rename => render_files_rename_list(gui, folder, &mut files, ui, ctx),
            Action::Delete => render_files_selectable_list(gui, folder, &mut files, action, ui, ctx),
            _ => render_files_basic_list(gui, folder, &mut files, action, ui, ctx),
        },
        FileTab::Conflicts => render_files_conflicts_list(gui, folder, &mut files, table_guard, ui, ctx),
    };
    gui.runtime.spawn({
        let folder = folder.clone();
        async move {
            folder.flush_file_changes().await
        }
    });
}

fn render_folder_controls(gui: &mut GuiApp, folder: &Arc<AppFolder>, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        if ui.button("Update file intents").clicked() {
            let folder = folder.clone();
            gui.runtime.spawn(async move {
                folder.update_file_intents().await
            });
        };

        if ui.button("Load cache from file").clicked() {
            let folder = folder.clone();
            gui.runtime.spawn(async move {
                folder.load_cache_from_file().await
            });
        };

        if ui.button("Refresh cache from api").clicked() {
            let folder = folder.clone();
            let session = gui.app.get_login_session().clone();
            gui.runtime.spawn(async move {
                let session_guard = session.read().await;
                if let Some(session) = session_guard.as_ref() {
                    folder.refresh_cache_from_api(session.clone()).await?;
                    folder.save_cache_to_file().await?;
                }
                Some(())
            });
        };

        if ui.button("Execute changes").clicked() {
            let folder = folder.clone();
            gui.runtime.spawn(async move {
                folder.execute_file_changes().await;
                folder.update_file_intents().await
            });
        };
        
        ui.toggle_value(&mut gui.show_series_search, "Search series");
    });
}

fn render_folder_info(gui: &mut GuiApp, folder: &Arc<AppFolder>, ui: &mut egui::Ui, ctx: &egui::Context) {
    let cache_lock = folder.get_cache().try_read();
    let cache_guard = match cache_lock {
        Ok(cache) => cache,
        Err(_) => {
            ui.spinner();
            return;
        },
    };

    let cache = match cache_guard.as_ref() {
        Some(cache) => cache,
        None => {
            ui.label("No cache loaded");
            return;
        },
    };
    
    ui.heading("Series");
    ui.push_id("series_table", |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            render_series_table(&cache.series, ui, ctx);
        });
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            render_episode_table(episode, ui, ctx);
        });
    });
}

fn render_folder_panel(gui: &mut GuiApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let (folder, is_not_busy) = {
        let folders = match gui.app.get_folders().try_read() {
            Ok(folders) => folders,
            Err(_) => {
                ui.spinner();
                return;
            },
        };

        let selected_index = gui.app.get_selected_folder_index().blocking_read();
        let selected_index = match *selected_index {
            Some(index) => index,
            None => {
                ui.label("No folder selected");
                return;
            },
        };

        let folder = folders[selected_index].clone();
        let is_not_busy = folder.get_busy_lock().try_lock().is_ok();
        (folder, is_not_busy)
    };
    
    if !*folder.get_is_initial_load().blocking_read() {
        gui.runtime.spawn({
            let folder = folder.clone();
            async move {
                *folder.get_is_initial_load().write().await = true;
                if folder.is_cache_loaded().await {
                    return Some(());
                }
                folder.load_cache_from_file().await?;
                folder.update_file_intents().await
            }
        });
    }

    egui::TopBottomPanel::top("folder_controls")
        .resizable(false)
        .show_inside(ui, |ui| {
            render_folder_controls(gui, &folder, ui, ctx);
        });

    egui::TopBottomPanel::bottom("folder_error_list")
        .resizable(true)
        .show_inside(ui, |ui| {
            render_errors_list(gui, &folder, ui, ctx);
        });

    egui::SidePanel::right("folder_info")
        .resizable(true)
        .show_inside(ui, |ui| {
            render_folder_info(gui, &folder, ui, ctx);
        });

    egui::CentralPanel::default()
        .show_inside(ui, |ui| {
            ui.add_enabled_ui(is_not_busy, |ui| {
                render_files_list(gui, &folder, ui, ctx);
            });
        });
}

fn render_folders_list_panel(gui: &mut GuiApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    let folders = match gui.app.get_folders().try_read() {
        Ok(folders) => folders,
        Err(_) => {
            ui.spinner();
            return;
        },
    };

    let total_folders = folders.len();
    ui.heading(format!("Folders ({})", total_folders));
    if gui.app.get_folders_busy_lock().try_lock().is_err() {
        ui.spinner();
        return;
    }

    if total_folders == 0 {
        ui.label("No folders here");
        return;
    }
    
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let selected_index = *gui.app.get_selected_folder_index().blocking_read();
            for (index, folder) in folders.iter().enumerate() {
                let label = format!("{}", folder.get_folder_name());
                let mut is_selected = selected_index == Some(index);
                if ui.toggle_value(&mut is_selected, label).clicked() {
                    let mut selected_index = gui.app.get_selected_folder_index().blocking_write();
                    if !is_selected {
                        *selected_index = None;
                    } else {
                        *selected_index = Some(index);
                    }
                }
            }
        });
    });
}

fn render_series_search_list(gui: &mut GuiApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    if gui.app.get_series_busy_lock().try_lock().is_err() {
        ui.spinner();
        return;
    }

    let series = match gui.app.get_series().try_read() {
        Ok(series) => series,
        Err(_) => {
            ui.spinner();
            return;
        },
    };

    let series = match series.as_ref() {
        Some(series) => series,
        None => {
            ui.label("No search has been performed yet");
            return;
        },
    };

    if series.is_empty() {
        ui.label("Search gave no results");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        use egui_extras::{Column, TableBuilder};
        let row_height = 18.0;
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto().clip(true))
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::auto());

        table
            .header(row_height, |mut header| {
                header.col(|ui| { ui.strong("Name"); });
                header.col(|ui| { ui.strong("Status"); });
                header.col(|ui| { ui.strong("First Aired"); });
                header.col(|ui| { ui.strong(""); });
            })
            .body(|mut body| {
                let selected_index = *gui.app.get_selected_series_index().blocking_read();
                for (index, entry) in series.iter().enumerate() {
                    body.row(row_height, |mut row| {
                        row.col(|ui| { 
                            let is_selected = Some(index) == selected_index;
                            if ui.selectable_label(is_selected, entry.name.as_str()).clicked() {
                                if is_selected {
                                    *gui.app.get_selected_series_index().blocking_write() = None;
                                } else {
                                    *gui.app.get_selected_series_index().blocking_write() = Some(index);
                                }
                            }
                        });
                        row.col(|ui| {
                            let label = entry.status.as_ref().map(|x| x.as_str()).unwrap_or("Unknown");
                            ui.label(label);
                        });
                        row.col(|ui| {
                            let label = entry.first_aired.as_ref().map(|x| x.as_str()).unwrap_or("Unknown");
                            ui.label(label);
                        });
                        row.col(|ui| {
                            if ui.button("Select").clicked() {
                                gui.runtime.spawn({
                                    let entry_id = entry.id;
                                    let app = gui.app.clone();
                                    async move {
                                        app.set_series_to_current_folder(entry_id).await
                                    }
                                });
                            }
                        });
                    });

                }
            });
    });
}

fn render_series_table(series: &Series, ui: &mut egui::Ui, _ctx: &egui::Context) {
    egui::Grid::new("series_table")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("ID");
            ui.label(format!("{}", series.id));
            ui.end_row();

            ui.strong("Name");
            let label = format!("{}", series.name.as_str());
            let gui_label = egui::Label::new(label).wrap(true);
            ui.add(gui_label);
            ui.end_row();

            ui.strong("Status");
            let label = series.status.as_ref().map(|x| x.as_str()).unwrap_or("Unknown");
            ui.label(label);
            ui.end_row();

            ui.strong("Air date");
            let label = series.first_aired.as_ref().map(|x| x.as_str()).unwrap_or("Unknown");
            ui.label(label);
            ui.end_row();

            ui.strong("Genre");
            let label = match &series.genre {
                None => "Unknown".to_string(),
                Some(genres) => genres.join(","),
            };
            let gui_label = egui::Label::new(label).wrap(true);
            ui.add(gui_label);
            ui.end_row();

            ui.strong("Overview");
            let label = series.overview.as_ref().map(|x| x.as_str()).unwrap_or("Unknown");
            let gui_label = egui::Label::new(label).wrap(true);
            ui.add(gui_label);
            ui.end_row();
        });
}

fn render_episode_table(episode: &Episode, ui: &mut egui::Ui, _ctx: &egui::Context) {
    egui::Grid::new("episode_table")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("ID");
            ui.label(format!("{}", episode.id));
            ui.end_row();

            ui.strong("Index");
            ui.label(format!("S{:02}E{:02}", episode.season, episode.episode));
            ui.end_row();

            ui.strong("Name");
            ui.label(episode.name.as_ref().map(|x| x.as_str()).unwrap_or("None"));
            ui.end_row();

            ui.strong("Air date"); 
            let label = episode.first_aired.as_ref().map(|x| x.as_str()).unwrap_or("Unknown");
            ui.label(label);
            ui.end_row();

            ui.strong("Overview");
            let label = episode.overview.as_ref().map(|x| x.as_str()).unwrap_or("Unknown");
            let gui_label = egui::Label::new(label).wrap(true);
            ui.add(gui_label);
            ui.end_row();
        });
}

fn render_series_search_info_panel(gui: &mut GuiApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    if gui.app.get_series_busy_lock().try_lock().is_err() {
        ui.spinner();
        return;
    }

    let series = match gui.app.get_series().try_read() {
        Ok(series) => series,
        Err(_) => {
            ui.spinner();
            return;
        },
    };

    let series = match series.as_ref() {
        Some(series) => series,
        None => {
            ui.label("No series information");
            return;
        },
    };

    let selected_index = *gui.app.get_selected_series_index().blocking_read();
    let selected_index = match selected_index {
        Some(index) => index,
        None => {
            ui.label("No series selected");
            return;
        },
    };
    
    let series = &series.get(selected_index);
    let series = match series {
        Some(series) => series,
        None => {
            ui.colored_label(egui::Color32::DARK_RED, "Series index is outside of bounds");
            return;
        },
    };
    
    ui.push_id("series_search_info_table", |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            render_series_table(series, ui, _ctx);
        });
    });
}

fn render_series_search(gui: &mut GuiApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    egui::TopBottomPanel::top("search_bar")
        .resizable(true)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let res = ui.text_edit_singleline(&mut gui.series_search);
                let is_pressed = ui.button("Search").clicked();
                let is_entered = res.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if is_pressed || is_entered {
                    gui.runtime.spawn({
                        let series_search = gui.series_search.clone();
                        let app = gui.app.clone();
                        async move {
                            app.update_search_series(series_search).await
                        }
                    });
                }
            });
        });

    egui::SidePanel::right("search_series_info")
        .resizable(true)
        .default_width(120.0)
        .min_width(80.0)
        .show_inside(ui, |ui| {
            render_series_search_info_panel(gui, ui, ctx); 
        });

    egui::CentralPanel::default()
        .show_inside(ui, |ui| {
            render_series_search_list(gui, ui, ctx);
        });
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("Folders")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                render_folders_list_panel(self, ui, ctx);
            });

        egui::CentralPanel::default()
            .show(ctx, |ui| {
                render_folder_panel(self, ui, ctx);
            });

        let mut is_open = self.show_series_search;
        egui::Window::new("Series Search")
            .collapsible(false)
            .vscroll(false)
            .open(&mut is_open)
            .show(ctx, |ui| {
                render_series_search(self, ui, ctx);
            });
        self.show_series_search = is_open;
    }
}

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
                ui.label(self.message.as_str());
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

    let mut native_options = eframe::NativeOptions::default();
    native_options.maximized = true;

    eframe::run_native(
        "Torrent Renamer", 
        native_options, 
        Box::new({
            let root_path = root_path.clone();
            let config_path = config_path.clone();
            move |_| {
                let runtime = match tokio::runtime::Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(err) => return Box::new(FailedGuiApp::new(format!("Failed to create tokio runtime: {:?}", err))),
                };

                let app = match runtime.block_on(App::new(config_path.as_str())) {
                    Ok(app) => Arc::new(app),
                    Err(err) => return Box::new(FailedGuiApp::new(format!("Failed to create application: {:?}", err))),
                };

                runtime.spawn({
                    let app = app.clone();
                    async move {
                        tokio::join!(
                            app.open_folders(root_path.as_str()),
                            app.login(),
                        )
                    }
                });

                let gui = GuiApp::new(app, runtime);
                Box::new(gui)
            }
        }),
    )
}
