use egui;
use tvdb::models::{Series, Episode};
use open as cross_open;

const IMDB_PREFIX: &'static str = "https://www.imdb.com/title";

pub fn render_series_table(ui: &mut egui::Ui, series: &Series) {
    let layout = egui::Layout::left_to_right(egui::Align::Min)
        .with_main_justify(true)
        .with_main_wrap(true);
    ui.with_layout(layout, |ui| {
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

                if let Some(id) = series.imdb_id.as_ref() {
                    if !id.is_empty() {
                        ui.strong("IMDB");
                        let link_url = format!("{}/{}", IMDB_PREFIX, id);
                        if ui.link(link_url.as_str()).clicked() {
                            tokio::spawn(async move {
                                cross_open::that(link_url)
                            });
                        }
                        ui.end_row();
                    }
                }
            });
    });
}

pub fn render_episode_table(ui: &mut egui::Ui, episode: &Episode) {
    let layout = egui::Layout::left_to_right(egui::Align::Min)
        .with_main_justify(true)
        .with_main_wrap(true);
    ui.with_layout(layout, |ui| {
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

                if let Some(id) = episode.imdb_id.as_ref() {
                    if !id.is_empty() {
                        ui.strong("IMDB");
                        let link_url = format!("{}/{}", IMDB_PREFIX, id);
                        if ui.link(link_url.as_str()).clicked() {
                            tokio::spawn(async move {
                                cross_open::that(link_url)
                            });
                        }
                        ui.end_row();
                    }
                }
            });
    });
}
