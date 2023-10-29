use egui;

pub fn render_invisible_width_widget(ui: &mut egui::Ui) {
    let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
    ui.with_layout(layout, |ui| {
        ui.add_visible_ui(false, |ui| {
            ui.separator();
        });
    });
}

