//! The Settings page: language, log files, device connection, updates.

use super::*;

impl PlayerApp {
    pub(super) fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(egui::RichText::new(tr!("app-settings")).color(ColorTokens::TEXT_PRIMARY));
        ui.label(tr!(
            "app-settings-page-manage-your-language-device-connection-and"
        ));
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            let current = Language::current();
            let mut selected = current;
            ui.horizontal(|ui| {
                ui.strong(tr!("app-settings-page-language"));
                egui::ComboBox::from_id_salt("app-language")
                    .selected_text(current.name())
                    .show_ui(ui, |ui| {
                        for language in Language::all() {
                            ui.selectable_value(&mut selected, language, language.name());
                        }
                    });
            });
            if selected != current {
                self.language_save_error = selected.select().err().map(|error| error.to_string());
                ui.ctx().request_repaint();
            }
            if let Some(error) = &self.language_save_error {
                ui.label(tr!(
                    "app-settings-page-could-not-save-the-language-preference",
                    error = error
                ));
            }
        });
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong(tr!("app-settings-page-log-files"));
            ui.label(tr!(
                "app-settings-page-adbhelper-writes-one-log-file-per",
                keep_days = crate::logfile::KEEP_DAYS
            ));
            if let Ok(directory) = crate::logfile::directory() {
                ui.small(directory.display().to_string());
            }
            if ui
                .button(tr!("app-settings-page-open-log-folder"))
                .clicked()
            {
                self.log_folder_error = crate::logfile::open_folder().err();
            }
            if let Some(error) = &self.log_folder_error {
                ui.label(tr!(
                    "app-settings-page-could-not-open-the-log-folder",
                    error = error
                ));
            }
        });
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong(tr!("app-settings-page-app-updates"));
            ui.label(format!("ADBHelper {}", updater::VERSION));
            ui.hyperlink_to(
                tr!("app-settings-page-github-repository"),
                updater::REPOSITORY_URL,
            );
            ui.hyperlink_to(
                tr!("app-settings-page-third-party-notices"),
                format!("{}/blob/main/THIRD_PARTY.md", updater::REPOSITORY_URL),
            );
            ui.small(tr!(
                "app-settings-page-checks-automatically-at-startup-and-every"
            ));
            if self.available_update.is_some()
                && ui
                    .button(tr!("app-settings-page-show-update-and-patch-notes"))
                    .clicked()
            {
                self.update_dialog_open = true;
            }
            let cooldown = self
                .last_update_check
                .is_some_and(|last| last.elapsed() < Duration::from_secs(30));
            if ui
                .add_enabled(
                    !self.update_checking && self.update_progress.is_none() && !cooldown,
                    egui::Button::new(tr!("app-settings-page-check-for-updates")),
                )
                .clicked()
            {
                self.check_updates();
            }
            if self.update_checking {
                ui.label(tr!("app-settings-page-checking-for-updates"));
            } else if self.update_checked
                && self.available_update.is_none()
                && self.update_error.is_none()
            {
                ui.label(tr!("app-settings-page-you-re-using-the-latest-version"));
            }
            if let Some(error) = &self.update_error {
                ui.add(egui::Label::new(error).wrap());
            }
        });
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong(tr!("app-settings-page-device-connection"));
            ui.label(tr!("app-settings-page-select-a-device-and-manage-its"));
            ui.add_space(SpaceTokens::SM);
            ui.horizontal(|ui| {
                if self.selected_serial.is_some() {
                    status_badge(ui, tr!("app-settings-page-connected"), true);
                } else {
                    ui.colored_label(MUTED, tr!("app-settings-page-no-device-connected"));
                }
                if ui
                    .small_button(tr!("app-settings-page-find-devices"))
                    .clicked()
                {
                    self.refresh_devices();
                }
            });
            let selected_label = self
                .selected_serial
                .clone()
                .unwrap_or_else(|| tr!("app-settings-page-select-device").to_owned());
            egui::ComboBox::from_id_salt("device-picker-settings")
                .width(ui.available_width())
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    for device in &self.devices {
                        ui.selectable_value(
                            &mut self.selected_serial,
                            Some(device.serial.clone()),
                            format!("{} · {}", device.serial, device.details),
                        );
                    }
                });
            ui.add_space(SpaceTokens::SM);
            ui.strong(tr!("app-settings-page-adb-path"));
            ui.small(tr!("app-settings-page-leave-blank-to-use-adb-from"));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.adb_path)
                        .desired_width((ui.available_width() - 150.0).max(120.0)),
                );
                if ui.button(tr!("app-settings-page-apply-path")).clicked() {
                    if self.adb_path.trim().is_empty() {
                        std::env::remove_var("ADB_PATH");
                        self.log(tr!("app-settings-page-using-adb-from-path"));
                    } else {
                        std::env::set_var("ADB_PATH", self.adb_path.trim());
                        self.log(tr!(
                            "app-settings-page-adb-path-set-trim",
                            trim = self.adb_path.trim()
                        ));
                    }
                    self.refresh_devices();
                }
            });
            ui.add_space(SpaceTokens::SM);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.busy,
                        egui::Button::new(tr!("app-settings-page-launch-afk-journey")),
                    )
                    .clicked()
                {
                    self.run_device_action(
                        tr!("app-settings-page-afk-journey-launched"),
                        |serial| {
                            adb::launch_afk_journey(&serial).map_err(|error| error.to_string())
                        },
                    );
                }
                if ui
                    .add_enabled(
                        !self.busy,
                        egui::Button::new(tr!("app-settings-page-close-game")),
                    )
                    .clicked()
                {
                    self.run_device_action(tr!("app-settings-page-afk-journey-closed"), |serial| {
                        adb::stop_afk_journey(&serial).map_err(|error| error.to_string())
                    });
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new(tr!("app-settings-page-back")))
                    .clicked()
                {
                    self.run_device_action(tr!("app-settings-page-back-button-sent"), |serial| {
                        adb::press_back(&serial)
                            .map(|_| tr!("app-settings-page-sent-to-the-device").to_owned())
                            .map_err(|error| error.to_string())
                    });
                }
            });
        });
    }
}
