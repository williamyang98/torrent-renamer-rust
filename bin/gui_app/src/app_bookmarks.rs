use app::bookmarks::Bookmark;
use egui;

pub fn render_file_bookmarks(ui: &mut egui::Ui, bookmark: &mut Bookmark) -> bool {
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
