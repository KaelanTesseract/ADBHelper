//! Warning shown every time before Fishing starts, because the action is still in progress.

use super::*;

impl PlayerApp {
    pub(super) fn show_fishing_warning(&mut self, context: &egui::Context) {
        if !self.fishing_warning_open {
            return;
        }
        let (mut start, mut cancel) = (false, false);
        let response = egui::Modal::new(egui::Id::unique("fishing-wip-warning"))
            .frame(design_system::panel_frame())
            .show(context, |ui| {
                ui.set_width((context.content_rect().width() - 64.0).clamp(260.0, 460.0));
                ui.label(
                    egui::RichText::new(tr!("app-fishing-warning-title"))
                        .strong()
                        .size(16.0)
                        .color(ColorTokens::SELECTION),
                );
                ui.add_space(SpaceTokens::SM);
                ui.label(tr!("app-fishing-warning-body"));
                ui.add_space(SpaceTokens::MD);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    start = ui.button(tr!("app-fishing-warning-start")).clicked();
                    cancel = ui.button(tr!("app-fishing-warning-cancel")).clicked();
                });
            });
        if start {
            self.fishing_warning_open = false;
            self.fishing_warning_accepted = true;
            self.start_action(StageAction::Fishing);
        } else if cancel || response.should_close() {
            self.fishing_warning_open = false;
        }
    }
}
