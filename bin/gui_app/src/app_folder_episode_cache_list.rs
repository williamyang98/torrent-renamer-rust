use app::app_folder::AppFolder;
use app::tvdb_cache::EpisodeKey;
use egui;
use egui_extras::{Column, TableBuilder};
use std::sync::Arc;
use crate::fuzzy_search::{FuzzySearcher, render_search_bar};
use crate::clipped_selectable::ClippedSelectableLabel;

pub fn render_episode_cache_list(ui: &mut egui::Ui, searcher: &mut FuzzySearcher, folder: &Arc<AppFolder>) {
    let cache = folder.get_cache().blocking_read();
    let cache = match cache.as_ref() {
        Some(cache) => cache,
        None => {
            ui.label("No cache loaded");
            return;
        },
    };

    let episodes = &cache.episodes;
    if episodes.is_empty() {
        ui.label("No episodes available");
        return;
    }

    render_search_bar(ui, searcher);
    
    // Create a string that we can search for each episode
    let mut episode_name = String::new();
    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();
    let row_height = 18.0;
    let cell_layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(cell_layout)
        .column(Column::remainder().resizable(true).clip(true))
        .column(Column::auto().resizable(false))
        .header(row_height, |mut header| {
            header.col(|ui| { ui.strong("Name"); });
            header.col(|ui| { ui.strong("First Aired"); });
        })
        .body(|mut body| {
            for entry in episodes {
                use std::fmt::Write;
                episode_name.clear();
                let _ = write!(episode_name, "S{:02}E{:02}", entry.season, entry.episode);
                if let Some(name) = entry.name.as_deref() {
                    let _ = write!(episode_name, " {}", name);
                }
                if !searcher.search(episode_name.as_str()) {
                    continue;
                }

                body.row(row_height, |mut row| {
                    row.col(|ui| { 
                        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
                        ui.with_layout(layout, |ui| {
                            let descriptor = EpisodeKey { season: entry.season, episode: entry.episode };
                            let is_selected = Some(descriptor) == selected_descriptor;
                            let elem = ClippedSelectableLabel::new(is_selected, episode_name.as_str());
                            let res = ui.add(elem);
                            if res.clicked() {
                                if is_selected {
                                    *folder.get_selected_descriptor().blocking_write() = None;
                                } else {
                                    *folder.get_selected_descriptor().blocking_write() = Some(descriptor);
                                }
                            }
                        });
                    });
                    row.col(|ui| {
                        let label = entry.first_aired.as_deref().unwrap_or("Unknown");
                        ui.label(label);
                    });
                });
            }
        });
}
