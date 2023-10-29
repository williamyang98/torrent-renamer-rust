use app::app_file::AppFileMutableContext;
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

pub fn check_file_shortcuts(ui: &mut egui::Ui, files: &mut AppFileMutableContext<'_>, index: usize) {
    let current_action = files.get_action(index);
    for action in Action::iterator() {
        let action = *action;
        if action == current_action {
            continue;
        }
        let shortcut = &ACTION_SHORTCUTS[action];
        if ui.input_mut(|i| i.consume_shortcut(shortcut)) {
            files.set_action(action, index);
        }
    }
}

pub fn render_file_context_menu(
    ui: &mut egui::Ui, runtime: &tokio::runtime::Runtime,
    folder_path: &str, files: &mut AppFileMutableContext<'_>, index: usize, is_not_busy: bool,
) {
    let current_action = files.get_action(index);
    if ui.button("Open file").clicked() {
        runtime.spawn({
            let src = files.get_src(index);
            let filename_path = Path::new(folder_path).join(src);
            let filename_path_str = filename_path.to_string_lossy().to_string();
            async move {
                cross_open::that(filename_path_str)
            }
        });
        ui.close_menu();
    }

    if ui.button("Open folder").clicked() {
        runtime.spawn({
            let src = files.get_src(index);
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
            files.set_action(action, index);
            ui.close_menu();
        }
    }
}
