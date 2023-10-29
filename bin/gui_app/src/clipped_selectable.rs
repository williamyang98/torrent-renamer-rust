use egui;

pub struct ClippedSelectableLabel {
    selected: bool,
    text: egui::WidgetText,
}

impl ClippedSelectableLabel {
    pub fn new(selected: bool, text: impl Into<egui::WidgetText>) -> Self {
        ClippedSelectableLabel {
            selected,
            text: text.into(),
        }
    }
}

impl egui::Widget for ClippedSelectableLabel {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let button_padding = ui.spacing().button_padding;
        let total_extra = button_padding + button_padding;

        // Taken from egui::Label
        let valign = ui.layout().vertical_align();
        let max_text_width = ui.available_width() - total_extra.x;
        let mut text_job = self.text.into_text_job(ui.style(), egui::FontSelection::Default, valign);
        text_job.job.wrap.max_width = max_text_width; 
        text_job.job.wrap.max_rows = 1;
        text_job.job.wrap.break_anywhere = true;
        text_job.job.wrap.overflow_character = None;
        let text_galley = ui.fonts(|f| text_job.into_galley(f));

        // Rest is from egui::SelectableLabel
        let mut desired_size = total_extra + text_galley.size();
        desired_size.y = desired_size.y.max(ui.spacing().interact_size.y);
        let (rect, response) = ui.allocate_at_least(desired_size, egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, self.selected, text_galley.text())
        });

        if ui.is_rect_visible(response.rect) {
            let text_pos = ui
                .layout()
                .align_size_within_rect(text_galley.size(), rect.shrink2(button_padding))
                .min;

            let visuals = ui.style().interact_selectable(&response, self.selected);
            if self.selected || response.hovered() || response.highlighted() || response.has_focus() {
                let rect = rect.expand(visuals.expansion);
                ui.painter().rect(
                    rect,
                    visuals.rounding,
                    visuals.weak_bg_fill,
                    visuals.bg_stroke,
                );
            }
            text_galley.paint_with_visuals(ui.painter(), text_pos, &visuals);
        }
        response
    }
}
