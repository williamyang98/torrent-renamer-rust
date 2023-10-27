use tokio;
use egui;
use egui_extras::{Column, TableBuilder};
use eframe;
use app::app::App;
use app::app_folder::{AppFolder, AppFileMutableContext, AppFileContextGetter};
use app::bookmarks::Bookmark;
use app::file_intent::Action;
use tvdb::models::{Series, Episode};
use std::sync::Arc;
use std::path::Path;
use open as cross_open;
use lazy_static::lazy_static;
use enum_map;

struct FuzzySearcher {
    search_edit_line: String,
    search_edit_line_filtered: String,
    input_edit_line_filtered: String,
    char_blacklist: Vec<char>,
}

impl FuzzySearcher {
    fn new() -> Self {
        Self {
            search_edit_line: "".to_owned(),
            search_edit_line_filtered: "".to_owned(),
            input_edit_line_filtered: "".to_owned(),
            char_blacklist: vec!['.', '-', ' '],
        }
    }

    fn update_search_filtered(&mut self) {
        self.search_edit_line_filtered.clear();
        for c in self.search_edit_line.chars() {
            if self.char_blacklist.contains(&c) {
                continue;
            }
            if c.is_ascii() {
                self.search_edit_line_filtered.push(c.to_ascii_lowercase());
            }
        }
    }

    fn search(&mut self, input: &str) -> bool {
        if self.search_edit_line_filtered.is_empty() {
            return true;
        }

        self.input_edit_line_filtered.clear();
        for c in input.chars() {
            if self.char_blacklist.contains(&c) {
                continue;
            }
            if c.is_ascii() {
                self.input_edit_line_filtered.push(c.to_ascii_lowercase());
            }
        }
        self.input_edit_line_filtered.contains(self.search_edit_line_filtered.as_str())
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum FileTab {
    FileAction(Action),
    Conflicts,
}

struct GuiApp {
    app: Arc<App>,
    runtime: tokio::runtime::Runtime,
    selected_tab: FileTab,
    show_series_search: bool,
    series_api_search: String,
    series_fuzzy_search: FuzzySearcher,
    episodes_fuzzy_search: FuzzySearcher,
}

impl GuiApp {
    fn new(app: Arc<App>, runtime: tokio::runtime::Runtime) -> Self {
        Self {
            app,
            runtime,
            show_series_search: false,
            selected_tab: FileTab::FileAction(Action::Complete),
            series_api_search: "".to_string(),
            series_fuzzy_search: FuzzySearcher::new(),
            episodes_fuzzy_search: FuzzySearcher::new(),
        }
    }
}

lazy_static! {
    static ref ACTION_SHORTCUTS: enum_map::EnumMap<Action, egui::KeyboardShortcut> = enum_map::enum_map!{
        Action::Delete => egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Delete),
        Action::Ignore => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::I),
        Action::Rename => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::R),
        Action::Whitelist => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::W),
        Action::Complete => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::C),
    };
}

fn render_search_bar(ui: &mut egui::Ui, search_bar: &mut FuzzySearcher) {
    ui.horizontal(|ui| {
        let res = ui.text_edit_singleline(&mut search_bar.search_edit_line);
        if res.changed() {
            search_bar.update_search_filtered();
        }
        if ui.button("Clear").clicked() {
            search_bar.search_edit_line.clear();
            search_bar.update_search_filtered();
        }
    });
}

fn render_errors_list(ui: &mut egui::Ui, errors: &mut Vec<String>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let total_items = errors.len();
        if total_items == 0 {
            ui.label("No errors");
            return;
        }
        
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let mut selected_index = None;
            for (index, error) in errors.iter().enumerate().rev() {
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

fn check_file_shortcuts(ui: &mut egui::Ui, files: &mut AppFileMutableContext<'_>, index: usize) {
    let current_action = files.get_action(index);
    for action in Action::iterator() {
        let action = *action;
        if action == current_action {
            continue;
        }
        let shortcut = &ACTION_SHORTCUTS[action];
        if ui.input_mut(|i| i.consume_shortcut(shortcut)) {
            files.set_action(action, index);
        }
    }
}

fn render_file_context_menu(
    gui: &mut GuiApp, ui: &mut egui::Ui, 
    folder_path: &str, files: &mut AppFileMutableContext<'_>, index: usize,
) {
    let current_action = files.get_action(index);
    if ui.button("Open file").clicked() {
        gui.runtime.spawn({
            let src = files.get_src(index);
            let filename_path = Path::new(folder_path).join(src);
            let filename_path_str = filename_path.to_string_lossy().to_string();
            async move {
                cross_open::that(filename_path_str)
            }
        });
        ui.close_menu();
    }

    if ui.button("Open folder").clicked() {
        gui.runtime.spawn({
            let src = files.get_src(index);
            let filename_path = Path::new(folder_path).join(src);
            let folder_path = filename_path.parent().unwrap_or(Path::new("."));
            let folder_path_str = folder_path.to_string_lossy().to_string();
            async move {
                cross_open::that(folder_path_str)
            }
        });
        ui.close_menu();
    }

    ui.separator();
    
    for action in Action::iterator() {
        let action = *action;
        if action == current_action {
            continue;
        }
        let button = egui::Button::new(action.to_str())
            .shortcut_text(ui.ctx().format_shortcut(&ACTION_SHORTCUTS[action]));
        if ui.add(button).clicked() {
            files.set_action(action, index);
            ui.close_menu();
        }
    }
}

fn render_file_bookmarks(ui: &mut egui::Ui, bookmark: &mut Bookmark) -> bool {
    let height = ui.text_style_height(&egui::TextStyle::Monospace);
    let mut is_changed = false;
    ui.horizontal(|ui| {
        {
            let value = &mut bookmark.is_favourite;
            let label = egui::RichText::new("★").strong().size(height).color(
                match value {
                    true => egui::Color32::GOLD,
                    false => egui::Color32::LIGHT_GRAY,
                }
            );
            let elem = egui::Label::new(label).sense(egui::Sense::click());
            if ui.add(elem).clicked() {
                *value = !*value;
                is_changed = true;
            }
        }
        {
            let value = &mut bookmark.is_unread;
            let label = egui::RichText::new("？").strong().size(height).color(
                match value {
                    true => egui::Color32::DARK_RED,
                    false => egui::Color32::LIGHT_GRAY,
                }
            );
            let elem = egui::Label::new(label).sense(egui::Sense::click());
            if ui.add(elem).clicked() {
                *value = !*value;
                is_changed = true;
            }
        }
        {
            let value = &mut bookmark.is_read;
            let label = egui::RichText::new("✔").strong().size(height).color(
                match value {
                    true => egui::Color32::DARK_GREEN,
                    false => egui::Color32::LIGHT_GRAY,
                }
            );
            let elem = egui::Label::new(label).sense(egui::Sense::click());
            if ui.add(elem).clicked() {
                *value = !*value;
                is_changed = true;
            }
        }
    });
    is_changed
}

fn render_files_selectable_list(gui: &mut GuiApp, ui: &mut egui::Ui, selected_action: Action, folder: &Arc<AppFolder>) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let mut files = folder.get_mut_files_blocking(); 
    let mut bookmarks = folder.get_bookmarks().blocking_write();
    let mut is_bookmarks_changed = false;

    if file_tracker.get_action_count()[selected_action] == 0 {
        ui.heading(format!("No {}s", selected_action.to_str().to_lowercase()));
        return;
    }

    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();
    let mut is_select_all = false;
    let mut is_deselect_all = false;
    ui.horizontal(|ui| {
        is_select_all = ui.button("Select all").clicked();
        is_deselect_all = ui.button("Deselect all").clicked();
    });
    
    render_search_bar(ui, &mut gui.episodes_fuzzy_search);

    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            for index in 0..files.get_total_items() {
                let action = files.get_action(index);
                if action != selected_action {
                    continue;
                }
                
                if !gui.episodes_fuzzy_search.search(files.get_src(index)) {
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

                    let src = files.get_src(index);
                    let bookmark = bookmarks.get_mut_with_insert(src);
                    is_bookmarks_changed = render_file_bookmarks(ui, bookmark) || is_bookmarks_changed;

                    let descriptor = files.get_src_descriptor(index);
                    let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                    let res = ui.selectable_label(is_selected, src);
                    if res.clicked() {
                        if is_selected {
                            *folder.get_selected_descriptor().blocking_write() = None;
                        } else {
                            *folder.get_selected_descriptor().blocking_write() = *descriptor;
                        }
                    }
                    if res.hovered() {
                        check_file_shortcuts(ui, &mut files, index);
                    }
                    res.context_menu(|ui| {
                        render_file_context_menu(gui, ui, folder.get_folder_path(), &mut files, index);
                    });
                });
            }
        });
    });

    if is_bookmarks_changed {
        gui.runtime.spawn({
            let folder = folder.clone();
            async move {
                folder.save_bookmarks_to_file().await
            }
        });
    }
}

fn render_files_basic_list(gui: &mut GuiApp, ui: &mut egui::Ui, selected_action: Action, folder: &Arc<AppFolder>) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let mut files = folder.get_mut_files_blocking();
    let mut bookmarks = folder.get_bookmarks().blocking_write();
    let mut is_bookmarks_changed = false;

    if file_tracker.get_action_count()[selected_action] == 0 {
        ui.heading(format!("No {}s", selected_action.to_str().to_lowercase()));
        return;
    }

    render_search_bar(ui, &mut gui.episodes_fuzzy_search);

    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            for index in 0..files.get_total_items() {
                let action = files.get_action(index);
                if action != selected_action {
                    continue;
                }

                if !gui.episodes_fuzzy_search.search(files.get_src(index)) {
                    continue;
                }
                
                ui.horizontal(|ui| {
                    let src = files.get_src(index);
                    let bookmark = bookmarks.get_mut_with_insert(src);
                    is_bookmarks_changed = render_file_bookmarks(ui, bookmark) || is_bookmarks_changed;

                    let descriptor = files.get_src_descriptor(index);
                    let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                    let res = ui.selectable_label(is_selected, src);
                    if res.clicked() {
                        if is_selected {
                            *folder.get_selected_descriptor().blocking_write() = None;
                        } else {
                            *folder.get_selected_descriptor().blocking_write() = *descriptor;
                        }
                    }
                    if res.hovered() {
                        check_file_shortcuts(ui, &mut files, index);
                    }
                    res.context_menu(|ui| {
                        render_file_context_menu(gui, ui, folder.get_folder_path(), &mut files, index);
                    });
                });
            }
        });
    });

    if is_bookmarks_changed {
        gui.runtime.spawn({
            let folder = folder.clone();
            async move {
                folder.save_bookmarks_to_file().await
            }
        });
    }
}

fn render_files_rename_list(gui: &mut GuiApp, ui: &mut egui::Ui, folder: &Arc<AppFolder>) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let mut files = folder.get_mut_files_blocking(); 
    if file_tracker.get_action_count()[Action::Rename] == 0 {
        ui.heading("No renames");
        return;
    }

    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();

    let mut is_select_all = false;
    let mut is_deselect_all = false;
    ui.horizontal(|ui| {
        is_select_all = ui.button("Select all").clicked();
        is_deselect_all = ui.button("Deselect all").clicked();
    });

    render_search_bar(ui, &mut gui.episodes_fuzzy_search);
    
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Center).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let row_height = 18.0;
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::LEFT))
                .column(Column::auto().resizable(false))
                .column(Column::auto().clip(true))
                .column(Column::remainder().clip(true).resizable(false))
                .header(row_height, |mut header| {
                    header.col(|_| {});
                    header.col(|ui| { ui.strong("Source"); });
                    header.col(|ui| { ui.strong("Destination"); });
                })
                .body(|mut body| {
                    for index in 0..files.get_total_items() {
                        let action = files.get_action(index);
                        if action != Action::Rename {
                            continue;
                        }

                        if !gui.episodes_fuzzy_search.search(files.get_src(index)) {
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
                                let descriptor = files.get_src_descriptor(index);
                                let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                                let is_conflict = files.get_is_conflict(index);
                                let src = files.get_src(index);
                                let res = if is_conflict {
                                    ui.selectable_label(is_selected, egui::RichText::new(src).color(egui::Color32::DARK_RED))
                                } else {
                                    ui.selectable_label(is_selected, src)
                                };
                                if res.clicked() {
                                    if is_selected {
                                        *folder.get_selected_descriptor().blocking_write() = None;
                                    } else {
                                        *folder.get_selected_descriptor().blocking_write() = *descriptor;
                                    }
                                }
                                if res.hovered() {
                                    check_file_shortcuts(ui, &mut files, index);
                                }
                                res.context_menu(|ui| {
                                    render_file_context_menu(gui, ui, folder.get_folder_path(), &mut files, index);
                                });
                            });
                            row.col(|ui| {
                                let mut dest_edit_buffer = files.get_dest(index).to_string();
                                if ui.text_edit_singleline(&mut dest_edit_buffer).changed() {
                                    files.set_dest(dest_edit_buffer, index);
                                }
                            });
                        });

                    }
                });
        });
    });

}

fn render_files_conflicts_list(gui: &mut GuiApp, ui: &mut egui::Ui, folder: &Arc<AppFolder>) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let mut files = folder.get_mut_files_blocking(); 
    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();
    
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            // keep track of when to add separators
            let mut is_first = true;
            let mut total_conflicts = 0;
            for (row_id, (dest, indices)) in file_tracker.get_pending_writes().iter().enumerate() {
                let mut total_files = indices.len();
                if total_files == 0 {
                    continue;
                }
                let source_index = file_tracker.get_source_index(dest.as_str());
                if source_index.is_some() {
                    total_files += 1;
                }
                let is_conflict = total_files > 1;
                if !is_conflict {
                    continue;
                }
                total_conflicts += 1;

                ui.push_id(row_id, |ui| {
                    if !is_first {
                        ui.separator();
                    }
                    is_first = false;

                    ui.heading(dest);

                    let row_height = 18.0;
                    TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::auto().resizable(false))
                        .column(Column::auto().clip(true))
                        .column(Column::remainder().resizable(false).clip(true))
                        .header(row_height, |mut header| {
                            header.col(|_| {});
                            header.col(|ui| { ui.strong("Source"); });
                            header.col(|ui| { ui.strong("Destination"); });
                        })
                        .body(|mut body| {
                            if let Some(index) = source_index {
                                let index = *index;
                                body.row(row_height, |mut row| {
                                    row.col(|_| {});
                                    row.col(|ui| { 
                                        let descriptor = files.get_src_descriptor(index);
                                        let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                                        let src = files.get_src(index);
                                        let res = ui.selectable_label(false, src); 
                                        if res.clicked() {
                                            if is_selected {
                                                *folder.get_selected_descriptor().blocking_write() = None;
                                            } else {
                                                *folder.get_selected_descriptor().blocking_write() = *descriptor;
                                            }
                                        }
                                        if res.hovered() {
                                            check_file_shortcuts(ui, &mut files, index);
                                        }
                                        res.context_menu(|ui| {
                                            render_file_context_menu(gui, ui, folder.get_folder_path(), &mut files, index);
                                        });
                                    });
                                    row.col(|_| {});
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
                                        let res = ui.selectable_label(false, src); 
                                        if res.hovered() {
                                            check_file_shortcuts(ui, &mut files, index);
                                        }
                                        res.context_menu(|ui| {
                                            render_file_context_menu(gui, ui, folder.get_folder_path(), &mut files, index);
                                        });
                                    });
                                    row.col(|ui| {
                                        if action == Action::Rename {
                                            let mut dest_edit_buffer = files.get_dest(index).to_string();
                                            if ui.text_edit_singleline(&mut dest_edit_buffer).changed() {
                                                files.set_dest(dest_edit_buffer, index);
                                            }
                                        }
                                    });
                                });
                            }
                        });
                });
            }

            if total_conflicts == 0 {
                ui.heading("No conflicts");
            }
        });
    });
}

fn render_files_tab_bar(gui: &mut GuiApp, ui: &mut egui::Ui, folder: &Arc<AppFolder>) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let total_conflicts = {
        let mut total_conflicts = 0;
        for (dest, indices) in file_tracker.get_pending_writes() {
            let mut total_files = indices.len();
            if total_files == 0 {
                continue;
            }
            if file_tracker.get_source_index(dest.as_str()).is_some() {
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
                    let count = file_tracker.get_action_count()[action];
                    format!("{} {}", action.to_str(), count)
                },
            };

            let is_selected = tab == old_selected_tab;
            if ui.selectable_label(is_selected,label).clicked() {
                gui.selected_tab = tab;
            }
        }
    });
}

fn render_files_list(gui: &mut GuiApp, ui: &mut egui::Ui, folder: &Arc<AppFolder>) {
    // Place all our lock guards in this scope so we can flush file changes afterwards
    {
        render_files_tab_bar(gui, ui, folder);
        ui.separator();
        match gui.selected_tab {
            FileTab::FileAction(action) => match action {
                Action::Rename => render_files_rename_list(gui, ui, folder),
                Action::Delete => render_files_selectable_list(gui, ui, action, folder),
                _ => render_files_basic_list(gui, ui, action, folder),
            },
            FileTab::Conflicts => render_files_conflicts_list(gui, ui, folder),
        };
    }
    
    folder.flush_file_changes_blocking();
}

fn render_folder_controls(gui: &mut GuiApp, ui: &mut egui::Ui, folder: &Arc<AppFolder>) {
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

fn render_folder_info(ui: &mut egui::Ui, folder: &Arc<AppFolder>) {
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            render_series_table(ui, &cache.series);
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
            render_episode_table(ui, episode);
        });
    });
}

fn render_folder_panel(gui: &mut GuiApp, ui: &mut egui::Ui) {
    let folders = gui.app.get_folders().blocking_read();
    let folder_index = *gui.app.get_selected_folder_index().blocking_read();
    let folder_index = match folder_index {
        Some(index) => index,
        None => {
            ui.label("No folder selected");
            return;
        },
    };

    let folder = folders[folder_index].clone();
    drop(folders);
    let is_not_busy = folder.get_busy_lock().try_lock().is_ok();

    gui.runtime.spawn({
        let folder = folder.clone();
        async move {
            folder.perform_initial_load().await
        }
    });
    
    egui::TopBottomPanel::top("folder_controls")
        .resizable(false)
        .show_inside(ui, |ui| {
            ui.add_enabled_ui(is_not_busy, |ui| {
                render_folder_controls(gui, ui, &folder);
            });
        });

    egui::TopBottomPanel::bottom("folder_error_list")
        .resizable(true)
        .show_inside(ui, |ui| {
            if let Ok(mut errors) = folder.get_errors().try_write() {
                render_errors_list(ui, errors.as_mut());
            } else {
                ui.spinner();
            }
        });

    egui::SidePanel::right("folder_info")
        .resizable(true)
        .show_inside(ui, |ui| {
            ui.push_id("folder_info", |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    render_folder_info(ui, &folder);
                });
            });
        });

    egui::CentralPanel::default()
        .show_inside(ui, |ui| {
            ui.push_id("folder_files_list", |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(is_not_busy, |ui| {
                        render_files_list(gui, ui, &folder);
                    });
                });
            });
        });
}

fn render_folders_list_panel(gui: &mut GuiApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    let folders = gui.app.get_folders().blocking_read();
    ui.heading(format!("Folders ({})", folders.len()));
    if gui.app.get_folders_busy_lock().try_lock().is_err() {
        ui.spinner();
        return;
    }

    if folders.is_empty() {
        ui.label("No folders");
        return;
    }
    
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let selected_index = *gui.app.get_selected_folder_index().blocking_read();
            for (index, folder) in folders.iter().enumerate() {
                let label = folder.get_folder_name().to_string();
                let mut is_selected = selected_index == Some(index);
                let res = ui.toggle_value(&mut is_selected, label);
                if res.clicked() {
                    let mut selected_index = gui.app.get_selected_folder_index().blocking_write();
                    if !is_selected {
                        *selected_index = None;
                    } else {
                        *selected_index = Some(index);
                    }
                }
                res.context_menu(|ui| {
                    if ui.button("Open folder").clicked() {
                        gui.runtime.spawn({
                            let folder_path_str = folder.get_folder_path().to_string();
                            async move {
                                cross_open::that(folder_path_str)
                            }
                        });
                        ui.close_menu();
                    }
                });
            }
        });
    });
}

fn render_series_search_list(gui: &mut GuiApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    if gui.app.get_series_busy_lock().try_lock().is_err() {
        ui.spinner();
        return;
    }

    let series = gui.app.get_series().blocking_read();
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

    render_search_bar(ui, &mut gui.series_fuzzy_search);

    let row_height = 18.0;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().clip(true))
        .column(Column::auto().resizable(false))
        .column(Column::auto().resizable(false))
        .column(Column::auto().resizable(false))
        .header(row_height, |mut header| {
            header.col(|ui| { ui.strong("Name"); });
            header.col(|ui| { ui.strong("Status"); });
            header.col(|ui| { ui.strong("First Aired"); });
            header.col(|ui| { ui.strong(""); });
        })
        .body(|mut body| {
            let selected_index = *gui.app.get_selected_series_index().blocking_read();
            for (index, entry) in series.iter().enumerate() {
                if !gui.series_fuzzy_search.search(entry.name.as_str()) {
                    continue;
                }

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
                        let label = entry.status.as_deref().unwrap_or("Unknown");
                        ui.label(label);
                    });
                    row.col(|ui| {
                        let label = entry.first_aired.as_deref().unwrap_or("Unknown");
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
}

fn render_series_table(ui: &mut egui::Ui, series: &Series) {
    egui::Grid::new("series_table")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("ID");
            ui.label(format!("{}", series.id));
            ui.end_row();

            ui.strong("Name");
            let gui_label = egui::Label::new(series.name.as_str()).wrap(true);
            ui.add(gui_label);
            ui.end_row();

            ui.strong("Status");
            let label = series.status.as_deref().unwrap_or("Unknown");
            ui.label(label);
            ui.end_row();

            ui.strong("Air date");
            let label = series.first_aired.as_deref().unwrap_or("Unknown");
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
            let label = series.overview.as_deref().unwrap_or("Unknown");
            let gui_label = egui::Label::new(label).wrap(true);
            ui.add(gui_label);
            ui.end_row();
        });
}

fn render_episode_table(ui: &mut egui::Ui, episode: &Episode) {
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
            ui.label(episode.name.as_deref().unwrap_or("None"));
            ui.end_row();

            ui.strong("Air date"); 
            let label = episode.first_aired.as_deref().unwrap_or("Unknown");
            ui.label(label);
            ui.end_row();

            ui.strong("Overview");
            let label = episode.overview.as_deref().unwrap_or("Unknown");
            let gui_label = egui::Label::new(label).wrap(true);
            ui.add(gui_label);
            ui.end_row();
        });
}

fn render_series_search_info_panel(gui: &mut GuiApp, ui: &mut egui::Ui) {
    let series_opt = match gui.app.get_series().try_read() {
        Ok(series) => series,
        Err(_) => {
            ui.spinner();
            return;
        },
    };

    let series_list = match series_opt.as_ref() {
        Some(series_list) => series_list,
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
    
    let series = match series_list.get(selected_index) {
        Some(series) => series,
        None => {
            ui.colored_label(egui::Color32::DARK_RED, "Series index is outside of bounds");
            return;
        },
    };
    
    render_series_table(ui, series);
}

fn render_series_search(gui: &mut GuiApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    egui::TopBottomPanel::top("search_bar")
        .resizable(true)
        .show_inside(ui, |ui| {
            let is_not_busy = gui.app.get_series_busy_lock().try_lock().is_ok();
            ui.add_enabled_ui(is_not_busy, |ui| {
                ui.horizontal(|ui| {
                    let res = ui.text_edit_singleline(&mut gui.series_api_search);
                    let is_pressed = ui.button("Search").clicked();
                    let is_entered = res.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if is_pressed || is_entered {
                        gui.runtime.spawn({
                            let series_search = gui.series_api_search.clone();
                            let app = gui.app.clone();
                            async move {
                                app.update_search_series(series_search).await
                            }
                        });
                    }
                });
            });
        });

    egui::SidePanel::right("search_series_info")
        .resizable(true)
        .default_width(120.0)
        .min_width(80.0)
        .show_inside(ui, |ui| {
            ui.push_id("series_search_info_table", |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    render_series_search_info_panel(gui, ui); 
                });
            });
        });

    egui::CentralPanel::default()
        .show_inside(ui, |ui| {
            ui.push_id("series_search_list", |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    render_series_search_list(gui, ui, ctx);
                });
            });
        });
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("Folders")
            .resizable(true)
            .default_width(350.0)
            .min_width(100.0)
            .show(ctx, |ui| {
                render_folders_list_panel(self, ui, ctx);
            });

        egui::CentralPanel::default()
            .show(ctx, |ui| {
                render_folder_panel(self, ui);
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
