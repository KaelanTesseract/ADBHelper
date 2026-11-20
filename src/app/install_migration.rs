//! Moving a standalone EXE to an installed copy, and pointing to the old file afterwards.

use super::*;
use crate::install_info;

impl PlayerApp {
    /// Settings card, shown only in a standalone copy.
    pub(super) fn show_install_offer(&mut self, ui: &mut egui::Ui) {
        if self.runs_installed {
            return;
        }
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong(tr!("app-install-title"));
            ui.label(tr!("app-install-body"));
            ui.add_space(SpaceTokens::SM);
            let idle = self.installer_progress.is_none() && self.update_progress.is_none();
            if ui
                .add_enabled(
                    idle && !self.busy,
                    egui::Button::new(tr!("app-install-now")),
                )
                .clicked()
            {
                self.download_installer();
            }
            if self.busy && idle {
                ui.small(tr!("app-install-stop-first"));
            }
            if let Some(percent) = self.installer_progress {
                ui.add(
                    egui::ProgressBar::new(f32::from(percent) / 100.0)
                        .text(tr!("app-install-downloading", percent = percent)),
                );
            }
            if let Some(error) = &self.installer_error {
                ui.add(egui::Label::new(error).wrap());
            }
        });
    }

    fn download_installer(&mut self) {
        self.installer_progress = Some(0);
        self.installer_error = None;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = updater::download_installer(|percent| {
                let _ = sender.send(Event::InstallerProgress(percent));
            });
            let _ = sender.send(Event::InstallerReady(result));
        });
    }

    /// The verified installer is ready: note this copy, start the wizard, close.
    pub(super) fn installer_ready(&mut self, result: Result<std::path::PathBuf, String>) {
        self.installer_progress = None;
        let started = result.and_then(|path| {
            install_info::remember_standalone_copy().map_err(|e| e.to_string())?;
            updater::launch_installer(&path)
        });
        match started {
            Ok(()) => self.close_for_update = true,
            Err(error) => self.installer_error = Some(error),
        }
    }

    /// Shown once in the installed app while the old standalone EXE still exists.
    pub(super) fn show_old_copy_notice(&mut self, context: &egui::Context) {
        let Some(old) = self.old_copy.clone() else {
            return;
        };
        let (mut delete, mut open, mut keep) = (false, false, false);
        let response = egui::Modal::new(egui::Id::unique("old-standalone-copy"))
            .frame(design_system::panel_frame())
            .show(context, |ui| {
                ui.set_width((context.content_rect().width() - 64.0).clamp(260.0, 520.0));
                ui.label(
                    egui::RichText::new(tr!("app-install-done-title"))
                        .strong()
                        .size(16.0)
                        .color(ColorTokens::GLOW),
                );
                ui.add_space(SpaceTokens::SM);
                ui.label(tr!("app-install-done-body"));
                ui.add(
                    egui::Label::new(egui::RichText::new(old.display().to_string()).monospace())
                        .wrap(),
                );
                if let Some(error) = &self.old_copy_error {
                    ui.add(egui::Label::new(error).wrap());
                }
                ui.add_space(SpaceTokens::MD);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    delete = ui.button(tr!("app-install-delete-old")).clicked();
                    open = ui.button(tr!("app-install-open-folder")).clicked();
                    keep = ui.button(tr!("app-install-keep-old")).clicked();
                });
            });
        if delete {
            match std::fs::remove_file(&old) {
                Ok(()) => {
                    install_info::forget_standalone_copy();
                    self.old_copy = None;
                }
                Err(error) => {
                    self.old_copy_error = Some(tr!("app-install-delete-failed", error = error));
                }
            }
        } else if open {
            let _ = std::process::Command::new("explorer")
                .arg(format!("/select,{}", old.display()))
                .spawn();
        } else if keep || response.should_close() {
            install_info::forget_standalone_copy();
            self.old_copy = None;
        }
    }
}
