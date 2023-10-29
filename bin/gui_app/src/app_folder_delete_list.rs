use std::sync::Arc;
use app::file_intent::Action;
use app::app_folder::AppFolder;
use egui;
use crate::fuzzy_search::{FuzzySearcher, render_search_bar};
use crate::clipped_selectable::ClippedSelectableLabel;
use crate::app_file_actions::{check_file_shortcuts, render_file_context_menu};

pub fn render_files_delete_list(
    ui: &mut egui::Ui, 
    searcher: &mut FuzzySearcher, folder: &Arc<AppFolder>,
) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let mut files = folder.get_mut_files_blocking(); 
    if file_tracker.get_action_count()[Action::Delete] == 0 {
        ui.heading(format!("No {}s", Action::Delete.to_str().to_lowercase()));
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

    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            for index in 0..files.get_total_items() {
                let action = files.get_action(index);
                if action != Action::Delete {
                    continue;
                }

                if !searcher.search(files.get_src(index)) {
                    continue;
                }

                ui.horizontal(|ui| {
                    let mut is_enabled = files.get_is_enabled(index);
                    ui.add_enabled_ui(is_not_busy, |ui| {
                        if ui.checkbox(&mut is_enabled, "").clicked() {
                            files.set_is_enabled(is_enabled, index);
                        }
                    });
                    if is_select_all {
                        files.set_is_enabled(true, index);
                    }
                    if is_deselect_all {
                        files.set_is_enabled(false, index);
                    }

                    let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
                    ui.with_layout(layout, |ui| {
                        let src = files.get_src(index);
                        let descriptor = files.get_src_descriptor(index);
                        let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
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
                            check_file_shortcuts(ui, &mut files, index);
                        }
                        res.context_menu(|ui| {
                            render_file_context_menu(ui, folder.get_folder_path(), &mut files, index, is_not_busy);
                        });
                    });

                });
            }
        });
    });
}
