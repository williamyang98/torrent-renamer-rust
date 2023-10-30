use app::app::App;
use egui;
use egui_extras::{Column, TableBuilder};
use tvdb::models::Series;
use std::sync::Arc;
use tokio;
use crate::fuzzy_search::{FuzzySearcher, render_search_bar};
use crate::clipped_selectable::ClippedSelectableLabel;
use crate::helpers::render_invisible_width_widget;
use crate::tvdb_tables::render_series_table;

pub struct GuiSeriesSearch {
    search_string: String,
    searcher: FuzzySearcher,
}

impl GuiSeriesSearch {
    pub fn new() -> Self {
        Self {
            search_string: "".to_string(),
            searcher: FuzzySearcher::new(),
        }
    }
}

impl Default for GuiSeriesSearch {
    fn default() -> Self {
        Self::new()
    }
}

fn render_series_search_list(
    ui: &mut egui::Ui,
    gui: &mut GuiSeriesSearch, app: &Arc<App>,
) {
    if app.get_series_busy_lock().try_lock().is_err() {
        ui.spinner();
        return;
    }

    let series = app.get_series().blocking_read();
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
    
    let folders = app.get_folders().blocking_read();
    let folder_index = *app.get_selected_folder_index().blocking_read();
    let folder = match folder_index {
        None => None,
        Some(index) => folders.get(index).cloned(),
    };
    drop(folders);
    let session = app.get_login_session().blocking_read();
    let is_folder_selected = folder.is_some();
    let is_logged_in = session.is_some();
    let is_not_busy = match folder.as_ref() {
        None => false,
        Some(folder) => folder.get_busy_lock().try_lock().is_ok(),
    };
    let is_series_selectable = is_folder_selected && is_logged_in && is_not_busy;

    render_search_bar(ui, &mut gui.searcher);

    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let cell_layout = egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(false);
            let row_height = 18.0;
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(cell_layout)
                .column(Column::remainder().resizable(true).clip(true))
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
                    let selected_index = *app.get_selected_series_index().blocking_read();
                    for (index, entry) in series.iter().enumerate() {
                        if !gui.searcher.search(entry.name.as_str()) {
                            continue;
                        }

                        body.row(row_height, |mut row| {
                            row.col(|ui| { 
                                let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
                                ui.with_layout(layout, |ui| {
                                    let is_selected = Some(index) == selected_index;
                                    let elem = ClippedSelectableLabel::new(is_selected, entry.name.as_str());
                                    let res = ui.add(elem);
                                    if res.clicked() {
                                        if is_selected {
                                            *app.get_selected_series_index().blocking_write() = None;
                                        } else {
                                            *app.get_selected_series_index().blocking_write() = Some(index);
                                        }
                                    }
                                });
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
                                ui.add_enabled_ui(is_series_selectable, |ui| {
                                    let res = ui.button("Select");
                                    if res.clicked() {
                                        tokio::spawn({
                                            let series_id = entry.id;
                                            let folder = folder.clone();
                                            let session = session.clone();
                                            async move {
                                                if let Some(folder) = folder {
                                                    if let Some(session) = session {
                                                        folder.load_cache_from_api(session, series_id).await?;
                                                        tokio::join!(
                                                            folder.update_file_intents(),
                                                            folder.save_cache_to_file(),
                                                        );
                                                        Some(())
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                }
                                            }
                                        });
                                    }
                                    res.on_disabled_hover_ui(|ui| {
                                        if !is_logged_in            { ui.label("Not logged in"); }
                                        else if !is_folder_selected { ui.label("No folder is selected"); }
                                        else if !is_not_busy        { ui.label("Folder is busy"); }
                                    });
                                });
                            });
                        });

                    }
                });
        });
    });

}

fn render_series_search_info_panel(
    ui: &mut egui::Ui, 
    series_list: Option<&Vec<Series>>, selected_index: Option<usize>,
) {
    render_invisible_width_widget(ui);

    let series_list = match series_list {
        Some(series_list) => series_list,
        None => {
            ui.label("No series information");
            return;
        },
    };

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

fn render_series_search_bar(
    ui: &mut egui::Ui, 
    gui: &mut GuiSeriesSearch, app: &Arc<App>,
) {
    let is_not_busy = app.get_series_busy_lock().try_lock().is_ok();
    ui.add_enabled_ui(is_not_busy, |ui| {
        let layout = egui::Layout::right_to_left(egui::Align::Min)
            .with_cross_justify(false)
            .with_main_justify(false)
            .with_main_wrap(false)
            .with_main_align(egui::Align::LEFT);
        ui.with_layout(layout, |ui| {
            let is_logged_in = app.get_login_session().blocking_read().is_some();
            let mut is_pressed = false;
            ui.add_enabled_ui(is_logged_in, |ui| {
                let res = ui.button("Search");
                is_pressed = res.clicked();
                res.on_disabled_hover_ui(|ui| {
                    ui.label("Not logged in");
                });
            });

            let elem = egui::TextEdit::singleline(&mut gui.search_string);
            let size = egui::vec2(
                ui.available_width(),
                ui.spacing().interact_size.y,
            );
            let line_res = ui.add_sized(size, elem);

            let is_entered = line_res.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if is_pressed || is_entered {
                tokio::spawn({
                    let series_search = gui.search_string.clone();
                    let app = app.clone();
                    async move {
                        app.update_search_series(series_search).await
                    }
                });
            }
        });
    });
}

pub fn render_series_search(
    ui: &mut egui::Ui, 
    gui: &mut GuiSeriesSearch, app: &Arc<App>,
) {
    let series = app.get_series().blocking_read();
    let selected_index = *app.get_selected_series_index().blocking_read();

    egui::SidePanel::right("search_series_info")
        .resizable(true)
        .show_inside(ui, |ui| {
            render_series_search_info_panel(ui, series.as_ref(), selected_index); 
        });

    egui::CentralPanel::default()
        .show_inside(ui, |ui| {
            render_series_search_bar(ui, gui, app);
            ui.separator();
            render_series_search_list(ui, gui, app);
        });
}

