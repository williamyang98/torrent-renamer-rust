use app::app_folder::AppFolder;
use app::file_intent::Action;
use std::sync::Arc;

use crate::app_folder_basic_list::render_files_basic_list;
use crate::app_folder_conflict_list::render_files_conflicts_list;
use crate::app_folder_delete_list::render_files_delete_list;
use crate::app_folder_rename_list::render_files_rename_list;
use crate::fuzzy_search::FuzzySearcher;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum FileTab {
    FileAction(Action),
    Conflicts,
}

lazy_static::lazy_static! {
    static ref FILE_TABS: [FileTab;6] = [
        FileTab::FileAction(Action::Complete), 
        FileTab::FileAction(Action::Rename), 
        FileTab::FileAction(Action::Delete), 
        FileTab::FileAction(Action::Ignore), 
        FileTab::FileAction(Action::Whitelist), 
        FileTab::Conflicts
    ];
}

fn render_files_tab_bar(ui: &mut egui::Ui, selected_tab: &mut FileTab, folder: &Arc<AppFolder>) {
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

    ui.horizontal(|ui| {
        let old_selected_tab = *selected_tab;
        for tab in FILE_TABS.iter() {
            let tab = *tab;
            let label = match tab {
                FileTab::Conflicts => format!("Conflicts {}", total_conflicts),
                FileTab::FileAction(action) => {
                    let count = file_tracker.get_action_count()[action];
                    format!("{} {}", action.to_str(), count)
                },
            };

            let is_selected = tab == old_selected_tab;
            if ui.selectable_label(is_selected,label).clicked() {
                *selected_tab = tab;
            }
        }
    });
}

pub fn render_files_tab_list(
    ui: &mut egui::Ui, runtime: &tokio::runtime::Runtime,
    selected_tab: &mut FileTab, searcher: &mut FuzzySearcher, folder: &Arc<AppFolder>,
) {
    render_files_tab_bar(ui, selected_tab, folder);
    ui.separator();
    
    let id = match selected_tab {
        FileTab::FileAction(action) => format!("file_list_{}", action.to_str().to_lowercase()),
        FileTab::Conflicts => "file_list_conflicts".to_string(),
    };
    
    ui.push_id(id, |ui| {
        match selected_tab {
            FileTab::FileAction(action) => match action {
                Action::Rename => render_files_rename_list(ui, runtime, searcher, folder),
                Action::Delete => render_files_delete_list(ui, runtime, searcher, folder),
                _ => render_files_basic_list(ui, runtime, searcher, *action, folder),
            },
            FileTab::Conflicts => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    render_files_conflicts_list(ui, runtime, folder);
                });
            },
        };
    });

    folder.flush_file_changes_blocking();
}
