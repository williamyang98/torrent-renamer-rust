use std::sync::Arc;
use app::file_intent::Action;
use app::app_folder::AppFolder;
use egui;
use egui_extras::{TableBuilder, Column};
use crate::clipped_selectable::ClippedSelectableLabel;
use crate::app_file_actions::{check_file_shortcuts, render_file_context_menu};

pub fn render_files_conflicts_list(
    ui: &mut egui::Ui, 
    folder: &Arc<AppFolder>,
) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let mut files = folder.get_mut_files_blocking(); 
    let is_not_busy = folder.get_busy_lock().try_lock().is_ok();
    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();
    
    // link the column widths across all of the tables
    let mut column_widths: Option<[f32;3]> = None;
    let mut is_add_separator = false;
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
            if is_add_separator {
                ui.separator();
            }
            is_add_separator = true;

            ui.label(egui::RichText::new(dest).strong().size(13.0));

            let row_height = 18.0;
            let cell_layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(cell_layout);
            table = match column_widths {
                Some(widths) => {
                    table
                        .column(Column::exact(widths[0]).resizable(false).clip(false))
                        .column(Column::exact(widths[1]).resizable(true).clip(true))
                        .column(Column::exact(widths[2]).resizable(false).clip(true))
                },
                None => {
                    table
                        .column(Column::auto_with_initial_suggestion(0.0).resizable(false).clip(false))
                        .column(Column::remainder().resizable(true).clip(true))
                        .column(Column::remainder().resizable(false).clip(true))
                }
            };

            table
                .header(row_height, |mut header| {
                    header.col(|_| {});
                    header.col(|ui| { ui.strong("Source"); });
                    header.col(|ui| { ui.strong("Destination"); });
                })
                .body(|mut body| {
                    let mut render_entry = |index: usize| {
                        let mut file = files.get(index).expect("Valid index from pending writes list");
                        let action = file.get_action(); 
                        let mut current_column_widths: [f32;3] = [0.0,0.0,0.0];
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                if action == Action::Rename || action == Action::Delete {
                                    ui.add_enabled_ui(is_not_busy, |ui| {
                                        let mut is_enabled = file.get_is_enabled();
                                        if ui.checkbox(&mut is_enabled, "").clicked() {
                                            file.set_is_enabled(is_enabled);
                                        }
                                    });
                                }
                                current_column_widths[0] = ui.available_width();
                            });
                            row.col(|ui| {
                                let descriptor = file.get_src_descriptor();
                                let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                                let src = file.get_src();
                                let elem = ClippedSelectableLabel::new(is_selected, src);
                                let res = ui.add(elem);
                                if res.clicked() {
                                    if is_selected {
                                        *folder.get_selected_descriptor().blocking_write() = None;
                                    } else {
                                        *folder.get_selected_descriptor().blocking_write() = *descriptor;
                                    }
                                }
                                if is_not_busy && res.hovered() {
                                    check_file_shortcuts(ui, &mut file);
                                }
                                res.context_menu(|ui| {
                                    render_file_context_menu(ui, folder.get_folder_path(), &mut file, is_not_busy);
                                });
                                current_column_widths[1] = ui.available_width();
                            });
                            row.col(|ui| {
                                if action == Action::Rename {
                                    ui.add_enabled_ui(is_not_busy, |ui| {
                                        let mut dest_edit_buffer = file.get_dest().to_string();
                                        let elem = egui::TextEdit::singleline(&mut dest_edit_buffer);
                                        let res = ui.add_sized(ui.available_size(), elem);
                                        if res.changed() {
                                            file.set_dest(dest_edit_buffer);
                                        }
                                    });
                                }
                                current_column_widths[2] = ui.available_width();
                            });
                            if column_widths.is_none() {
                                column_widths = Some(current_column_widths);
                            }
                        });
                    };

                    if let Some(index) = source_index {
                        if !indices.contains(index) {
                            render_entry(*index);
                        }
                    }

                    for index in indices {
                        render_entry(*index);
                    }
                });
        });
    }

    if total_conflicts == 0 {
        ui.heading("No conflicts");
    }
}
