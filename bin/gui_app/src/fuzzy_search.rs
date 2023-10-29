use egui;

pub struct FuzzySearcher {
    search_edit_line: String,
    search_edit_line_filtered: String,
    input_edit_line_filtered: String,
    char_blacklist: Vec<char>,
}

impl Default for FuzzySearcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FuzzySearcher {
    pub fn new() -> Self {
        Self {
            search_edit_line: "".to_owned(),
            search_edit_line_filtered: "".to_owned(),
            input_edit_line_filtered: "".to_owned(),
            char_blacklist: vec!['.', '-', ' ', ',', '(', ')', '[', ']', ':'],
        }
    }

    fn update_search_filtered(&mut self) {
        self.search_edit_line_filtered.clear();
        for c in self.search_edit_line.chars() {
            if self.char_blacklist.contains(&c) {
                continue;
            }
            if c.is_ascii() {
                self.search_edit_line_filtered.push(c.to_ascii_lowercase());
            }
        }
    }

    pub fn search(&mut self, input: &str) -> bool {
        if self.search_edit_line_filtered.is_empty() {
            return true;
        }

        self.input_edit_line_filtered.clear();
        for c in input.chars() {
            if self.char_blacklist.contains(&c) {
                continue;
            }
            if c.is_ascii() {
                self.input_edit_line_filtered.push(c.to_ascii_lowercase());
            }
        }
        self.input_edit_line_filtered.contains(self.search_edit_line_filtered.as_str())
    }
}

pub fn render_search_bar(ui: &mut egui::Ui, search_bar: &mut FuzzySearcher) {
    let layout = egui::Layout::right_to_left(egui::Align::Min)
        .with_cross_justify(false)
        .with_main_justify(false)
        .with_main_wrap(false)
        .with_main_align(egui::Align::LEFT);

    ui.with_layout(layout, |ui| {
        if ui.button("Clear").clicked() {
            search_bar.search_edit_line.clear();
            search_bar.update_search_filtered();
        }
        let elem = egui::TextEdit::singleline(&mut search_bar.search_edit_line);
        let size = egui::vec2(
            ui.available_width(),
            ui.spacing().interact_size.y,
        );
        let res = ui.add_sized(size, elem);
        if res.changed() {
            search_bar.update_search_filtered();
        }
    });
}

