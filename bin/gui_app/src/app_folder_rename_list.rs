use std::sync::Arc;
use app::file_intent::Action;
use app::app_folder::AppFolder;
use egui;
use egui_extras::{TableBuilder, Column};
use crate::fuzzy_search::{FuzzySearcher, render_search_bar};
use crate::clipped_selectable::ClippedSelectableLabel;
use crate::app_file_actions::{check_file_shortcuts, render_file_context_menu};

pub fn render_files_rename_list(
    ui: &mut egui::Ui, 
    searcher: &mut FuzzySearcher, folder: &Arc<AppFolder>,
) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let mut files = folder.get_mut_files_blocking(); 
    if file_tracker.get_action_count()[Action::Rename] == 0 {
        ui.heading("No renames");
        return;
    }

    let is_not_busy = folder.get_busy_lock().try_lock().is_ok();
    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();

    let mut is_select_all = false;
    let mut is_deselect_all = false;
    ui.add_enabled_ui(is_not_busy, |ui| {
        ui.horizontal(|ui| {
            is_select_all = ui.button("Select all").clicked();
            is_deselect_all = ui.button("Deselect all").clicked();
        });
    });

    render_search_bar(ui, searcher);
   
    let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
    ui.with_layout(layout, |ui| {
        let cell_layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        let row_height = 18.0;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(cell_layout)
            .column(Column::initial(0.0).resizable(false).clip(false))
            .column(Column::auto().resizable(true).clip(true))
            .column(Column::remainder().resizable(false).clip(true))
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

                    if !searcher.search(files.get_src(index)) {
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
                            ui.add_enabled_ui(is_not_busy, |ui| {
                                let mut is_enabled = files.get_is_enabled(index);
                                if ui.checkbox(&mut is_enabled, "").clicked() {
                                    files.set_is_enabled(is_enabled, index);
                                }
                            });
                        });
                        row.col(|ui| {
                            let descriptor = files.get_src_descriptor(index);
                            let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                            let is_conflict = files.get_is_conflict(index);
                            let src = files.get_src(index);
                            let mut label = egui::RichText::new(src);
                            if is_conflict {
                                label = label.color(egui::Color32::DARK_RED)
                            }
                            let elem = ClippedSelectableLabel::new(is_selected, label);
                            let res = ui.add(elem);
                            if res.clicked() {
                                if is_selected {
                                    *folder.get_selected_descriptor().blocking_write() = None;
                                } else {
                                    *folder.get_selected_descriptor().blocking_write() = *descriptor;
                                }
                            }
                            if is_not_busy && res.hovered() {
                                check_file_shortcuts(ui, &mut files, index);
                            }
                            res.context_menu(|ui| {
                                render_file_context_menu(ui, folder.get_folder_path(), &mut files, index, is_not_busy);
                            });
                        });
                        row.col(|ui| {
                            ui.add_enabled_ui(is_not_busy, |ui| {
                                let mut dest_edit_buffer = files.get_dest(index).to_string();
                                let elem = egui::TextEdit::singleline(&mut dest_edit_buffer);
                                let res = ui.add_sized(ui.available_size(), elem);
                                if res.changed() {
                                    files.set_dest(dest_edit_buffer, index);
                                }
                            });
                        });
                    });

                }
            });
    });
}
