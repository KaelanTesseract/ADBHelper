//! The Settings page: language, log files, device connection, updates.

use super::*;

impl PlayerApp {
    pub(super) fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(
            egui::RichText::new(text("Settings", "Einstellungen")).color(ColorTokens::TEXT_PRIMARY),
        );
        ui.label(text(
            "Manage your language, device connection, and app updates.",
            "Geräteverbindung und App-Updates verwalten.",
        ));
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            let current = Language::current();
            let mut selected = current;
            ui.horizontal(|ui| {
                ui.strong(text("Language", "Sprache"));
                egui::ComboBox::from_id_salt("app-language")
                    .selected_text(current.name())
                    .show_ui(ui, |ui| {
                        for language in Language::ALL {
                            ui.selectable_value(&mut selected, language, language.name());
                        }
                    });
            });
            if selected != current {
                self.language_save_error = selected.select().err().map(|error| error.to_string());
                ui.ctx().request_repaint();
            }
            if let Some(error) = &self.language_save_error {
                ui.label(format_text!(
                    "Could not save the language preference: {error}",
                    "Die Spracheinstellung konnte nicht gespeichert werden: {error}"
                ));
            }
        });
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong(text("Log files", "Log-Dateien"));
            ui.label(format_text!(
                "ADBHelper writes one log file per day and keeps the last {} days. Attach the file when you report a problem.",
                "ADBHelper schreibt pro Tag eine Log-Datei und behält die letzten {} Tage. Hänge die Datei an, wenn du einen Fehler meldest.",
                crate::logfile::KEEP_DAYS
            ));
            if let Ok(directory) = crate::logfile::directory() {
                ui.small(directory.display().to_string());
            }
            if ui
                .button(text("Open log folder", "Log-Ordner öffnen"))
                .clicked()
            {
                self.log_folder_error = crate::logfile::open_folder().err();
            }
            if let Some(error) = &self.log_folder_error {
                ui.label(format_text!(
                    "Could not open the log folder: {error}",
                    "Der Log-Ordner konnte nicht geöffnet werden: {error}"
                ));
            }
        });
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong(text("App updates", "App-Updates"));
            ui.label(format!("ADBHelper {}", updater::VERSION));
            ui.hyperlink_to(
                text("GitHub repository", "GitHub-Repository"),
                updater::REPOSITORY_URL,
            );
            ui.small(
                text("Checks automatically at startup and every five minutes, including while minimized.", "Automatische Prüfung beim Start und alle 5 Minuten, auch bei minimierter App."),
            );
            if self.available_update.is_some()
                && ui
                    .button(text(
                        "Show update and patch notes",
                        "Update und Patchnotes anzeigen",
                    ))
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
                    egui::Button::new(text("Check for updates", "Auf Updates prüfen")),
                )
                .clicked()
            {
                self.check_updates();
            }
            if self.update_checking {
                ui.label(text("Checking for updates …", "Updates werden geprüft …"));
            } else if self.update_checked
                && self.available_update.is_none()
                && self.update_error.is_none()
            {
                ui.label(text(
                    "You’re using the latest version.",
                    "Du verwendest die aktuelle Version.",
                ));
            }
            if let Some(error) = &self.update_error {
                ui.add(egui::Label::new(error).wrap());
            }
        });
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong(text("Device connection", "Geräteverbindung"));
            ui.label(text(
                "Select a device and manage its connection.",
                "Gerät auswählen und Verbindung verwalten.",
            ));
            ui.add_space(SpaceTokens::SM);
            ui.horizontal(|ui| {
                if self.selected_serial.is_some() {
                    status_badge(ui, text("CONNECTED", "VERBUNDEN"), true);
                } else {
                    ui.colored_label(MUTED, text("No device connected", "Kein Gerät verbunden"));
                }
                if ui
                    .small_button(text("Find devices", "Geräte suchen"))
                    .clicked()
                {
                    self.refresh_devices();
                }
            });
            let selected_label = self
                .selected_serial
                .clone()
                .unwrap_or_else(|| text("Select device", "Gerät auswählen").to_owned());
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
            ui.strong(text("ADB path", "ADB-Pfad"));
            ui.small(text(
                "Leave blank to use ADB from PATH.",
                "Leer lassen, um ADB aus PATH zu verwenden.",
            ));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.adb_path)
                        .desired_width((ui.available_width() - 150.0).max(120.0)),
                );
                if ui.button(text("Apply path", "Pfad übernehmen")).clicked() {
                    if self.adb_path.trim().is_empty() {
                        std::env::remove_var("ADB_PATH");
                        self.log(text("Using ADB from PATH.", "ADB aus PATH wird verwendet."));
                    } else {
                        std::env::set_var("ADB_PATH", self.adb_path.trim());
                        self.log(format_text!(
                            "ADB path set: {}",
                            "ADB-Pfad gesetzt: {}",
                            self.adb_path.trim()
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
                        egui::Button::new(text("Launch AFK Journey", "AFK Journey starten")),
                    )
                    .clicked()
                {
                    self.run_device_action(
                        text("AFK Journey launched", "AFK Journey gestartet"),
                        |serial| {
                            adb::launch_afk_journey(&serial).map_err(|error| error.to_string())
                        },
                    );
                }
                if ui
                    .add_enabled(
                        !self.busy,
                        egui::Button::new(text("Close game", "Spiel beenden")),
                    )
                    .clicked()
                {
                    self.run_device_action(
                        text("AFK Journey closed", "AFK Journey beendet"),
                        |serial| adb::stop_afk_journey(&serial).map_err(|error| error.to_string()),
                    );
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new(text("Back", "Zurück")))
                    .clicked()
                {
                    self.run_device_action(
                        text("Back button sent", "Zurück-Taste gesendet"),
                        |serial| {
                            adb::press_back(&serial)
                                .map(|_| {
                                    text("sent to the device", "an das Gerät gesendet").to_owned()
                                })
                                .map_err(|error| error.to_string())
                        },
                    );
                }
            });
        });
    }
}
