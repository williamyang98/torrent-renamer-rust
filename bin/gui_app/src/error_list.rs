use egui;

pub fn render_errors_list(ui: &mut egui::Ui, errors: &mut Vec<String>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
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

