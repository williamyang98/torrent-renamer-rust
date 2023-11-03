use app::app_file::MutableAppFile;
use app::file_intent::Action;
use egui;
use lazy_static::lazy_static;
use open as cross_open;
use std::path::Path;
use tokio;

lazy_static! {
    static ref ACTION_SHORTCUTS: enum_map::EnumMap<Action, egui::KeyboardShortcut> = enum_map::enum_map!{
        Action::Delete => egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Delete),
        Action::Ignore => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::I),
        Action::Rename => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::R),
        Action::Whitelist => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::W),
        Action::Complete => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::C),
    };
}

pub fn check_file_shortcuts(ui: &mut egui::Ui, file: &mut MutableAppFile<'_>) {
    let current_action = file.get_action();
    for action in Action::iterator() {
        let action = *action;
        if action == current_action {
            continue;
        }
        let shortcut = &ACTION_SHORTCUTS[action];
        if ui.input_mut(|i| i.consume_shortcut(shortcut)) {
            file.set_action(action);
        }
    }
}

pub fn render_file_context_menu(
    ui: &mut egui::Ui,
    folder_path: &str, file: &mut MutableAppFile<'_>, is_not_busy: bool,
) {
    let current_action = file.get_action();
    if ui.button("Open file").clicked() {
        tokio::spawn({
            let src = file.get_src();
            let filename_path = Path::new(folder_path).join(src);
            let filename_path_str = filename_path.to_string_lossy().to_string();
            async move {
                cross_open::that(filename_path_str)
            }
        });
        ui.close_menu();
    }

    if ui.button("Open folder").clicked() {
        tokio::spawn({
            let src = file.get_src();
            let filename_path = Path::new(folder_path).join(src);
            let folder_path = filename_path.parent().unwrap_or(Path::new("."));
            let folder_path_str = folder_path.to_string_lossy().to_string();
            async move {
                cross_open::that(folder_path_str)
            }
        });
        ui.close_menu();
    }
    
    if !is_not_busy {
        return;
    }

    ui.separator();
    
    for action in Action::iterator() {
        let action = *action;
        if action == current_action {
            continue;
        }
        let shortcut = &ACTION_SHORTCUTS[action];
        let button = egui::Button::new(action.to_str())
            .shortcut_text(ui.ctx().format_shortcut(shortcut));
        if ui.add(button).clicked() {
            file.set_action(action);
            ui.close_menu();
        }
    }
}
