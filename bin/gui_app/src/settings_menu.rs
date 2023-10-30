use eframe;
use egui;
use enum_map;
use crate::frame_history::FrameHistory;
use crate::helpers::render_invisible_width_widget;

pub struct GuiSettings {
    selected_option: GuiSettingsOption,
    frame_history: FrameHistory,
}

impl GuiSettings {
    pub fn new() -> Self {
        Self {
            selected_option: GuiSettingsOption::Settings,
            frame_history: FrameHistory::default(),
        }
    }

    pub fn update_frame(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        self.frame_history.on_new_frame(ctx.input(|i| i.time), frame.info().cpu_usage);
    }
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(PartialEq, Eq, Copy, Clone, enum_map::Enum)]
enum GuiSettingsOption {
    Settings,
    Inspection,
    Memory,
}

pub fn render_settings_menu(ui: &mut egui::Ui, ctx: &egui::Context, gui: &mut GuiSettings) {
    lazy_static::lazy_static! {
        static ref MENU_ITEMS: enum_map::EnumMap<GuiSettingsOption, &'static str> = enum_map::enum_map! {
            GuiSettingsOption::Settings => "🔧 Settings",
            GuiSettingsOption::Inspection => "🔍 Inspection",
            GuiSettingsOption::Memory => "📝 Memory",
        };
    }

    egui::SidePanel::left("settings_menu_items")
        .resizable(true)
        .show_inside(ui, |ui| {
            let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
            ui.with_layout(layout, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut render_label = |item: GuiSettingsOption| {
                        let label = MENU_ITEMS[item];
                        if ui.selectable_label(gui.selected_option == item, label).clicked() {
                            gui.selected_option = item;
                        }
                    };
                    render_label(GuiSettingsOption::Settings);
                    render_label(GuiSettingsOption::Inspection);
                    render_label(GuiSettingsOption::Memory);

                    ui.separator();

                    gui.frame_history.ui(ui);
                });
            });
        });
    
    egui::Frame::none().inner_margin(egui::Margin::symmetric(5.0, 0.0)).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            render_invisible_width_widget(ui);
            match gui.selected_option {
                GuiSettingsOption::Settings => ctx.settings_ui(ui),
                GuiSettingsOption::Inspection => ctx.inspection_ui(ui),
                GuiSettingsOption::Memory => ctx.memory_ui(ui),
            };
        });
    });

    egui::CentralPanel::default().show_inside(ui, |_| { });
}
