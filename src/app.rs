use crate::activity::{ActivityEvent, GameMode, RunActivity};
use crate::adb::{self, Device};
use crate::afk_journey::afk_stages::StageSelection;
use crate::design_system::{
    self, card, primary_button, status_badge, ColorTokens, GeometryTokens, SpaceTokens, TypeTokens,
};
use crate::updater;
use eframe::egui;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

enum Event {
    Devices(Result<Vec<Device>, String>),
    Log(String),
    Busy(bool),
    Activity(ActivityEvent),
    StagesFinished(Option<String>),
    UpdateChecked(Result<Option<updater::Release>, String>),
    UpdateProgress(u8),
    UpdateReady(Result<(), String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Automations,
    Settings,
}

pub struct PlayerApp {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    devices: Vec<Device>,
    selected_serial: Option<String>,
    logs: VecDeque<String>,
    busy: bool,
    page: Page,
    claim_stage_rewards: bool,
    adb_path: String,
    stage_cancel: Option<Arc<AtomicBool>>,
    stage_selection: StageSelection,
    suggested_formations: bool,
    current_team_first: bool,
    max_formations: u32,
    attempts_per_formation: u32,
    logo: egui::TextureHandle,
    activity: RunActivity,
    stage_error: Option<String>,
    available_update: Option<updater::Release>,
    update_checking: bool,
    update_checked: bool,
    last_update_check: Option<Instant>,
    update_progress: Option<u8>,
    update_error: Option<String>,
    update_notice: Option<String>,
    close_for_update: bool,
}

impl PlayerApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let logo_image = image::load_from_memory(include_bytes!("../assets/adbhelper-logo-v2.png"))
            .expect("embedded ADBHelper logo should be a valid PNG")
            .to_rgba8();
        let (logo_width, logo_height) = logo_image.dimensions();
        let logo = context.egui_ctx.load_texture(
            "adbhelper-logo",
            egui::ColorImage::from_rgba_unmultiplied(
                [logo_width as usize, logo_height as usize],
                logo_image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        let mut app = Self {
            sender,
            receiver,
            devices: Vec::new(),
            selected_serial: None,
            logs: VecDeque::new(),
            busy: false,
            page: Page::Automations,
            claim_stage_rewards: false,
            adb_path: std::env::var("ADB_PATH")
                .or_else(|_| std::env::var("ADBUTILS_ADB_PATH"))
                .unwrap_or_else(|_| adb::suggested_adb_path()),
            stage_cancel: None,
            stage_selection: StageSelection::default(),
            suggested_formations: true,
            current_team_first: true,
            max_formations: 10,
            attempts_per_formation: 5,
            logo,
            activity: RunActivity::default(),
            stage_error: None,
            available_update: None,
            update_checking: false,
            update_checked: false,
            last_update_check: None,
            update_progress: None,
            update_error: None,
            update_notice: updater::startup_notice(),
            close_for_update: false,
        };
        app.log("AFK Journey · App gestartet. Verbinde ein Android-Gerät mit aktiviertem ADB-Debugging.");
        app.refresh_devices();
        app.check_updates();
        design_system::apply_theme(&context.egui_ctx);
        app
    }

    fn log(&mut self, message: impl Into<String>) {
        if self.logs.len() >= 300 {
            self.logs.pop_front();
        }
        self.logs.push_back(message.into());
    }

    fn check_updates(&mut self) {
        if self.update_checking || self.update_progress.is_some() {
            return;
        }
        self.update_checking = true;
        self.update_error = None;
        self.last_update_check = Some(Instant::now());
        let sender = self.sender.clone();
        thread::spawn(move || {
            let _ = sender.send(Event::UpdateChecked(updater::check(updater::VERSION)));
        });
    }

    fn install_update(&mut self) {
        if self.busy || self.update_progress.is_some() {
            return;
        }
        let Some(release) = self.available_update.clone() else {
            return;
        };
        self.busy = true;
        self.update_progress = Some(0);
        self.update_error = None;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = updater::download_and_handoff(&release, |percent| {
                let _ = sender.send(Event::UpdateProgress(percent));
            });
            let _ = sender.send(Event::UpdateReady(result));
        });
    }

    fn show_update_banner(&mut self, ui: &mut egui::Ui) {
        if let Some(notice) = self.update_notice.clone() {
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(notice);
                    if ui.small_button("Schließen").clicked() {
                        self.update_notice = None;
                    }
                });
            });
            ui.add_space(SpaceTokens::MD);
        }
        if let Some(release) = self.available_update.clone() {
            design_system::active_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Update {} verfügbar", release.version))
                            .color(ColorTokens::GLOW),
                    );
                    if ui
                        .add_enabled(!self.busy, primary_button("Updaten"))
                        .clicked()
                    {
                        self.install_update();
                    }
                    ui.hyperlink_to("Release auf GitHub", &release.url);
                });
                ui.label(format!(
                    "Installiert: {} · Neu: {}",
                    updater::VERSION,
                    release.version
                ));
                ui.strong("Patchnotes");
                egui::ScrollArea::vertical()
                    .id_salt("update-patchnotes")
                    .max_height(135.0)
                    .show(ui, |ui| {
                        for line in release.notes.lines() {
                            if line.starts_with('#') {
                                ui.label(
                                    egui::RichText::new(line.trim_start_matches('#').trim())
                                        .strong(),
                                );
                            } else {
                                ui.add(egui::Label::new(line).wrap());
                            }
                        }
                    });
                if let Some(percent) = self.update_progress {
                    ui.add(egui::ProgressBar::new(f32::from(percent) / 100.0).text(
                        if percent == 100 {
                            "Update wird geprüft und vorbereitet …".into()
                        } else {
                            format!("Download: {percent} %")
                        },
                    ));
                } else if self.busy {
                    ui.small("Das Update kann nach dem laufenden Vorgang installiert werden.");
                }
                if let Some(error) = &self.update_error {
                    ui.add(egui::Label::new(error).wrap());
                }
            });
            ui.add_space(SpaceTokens::MD);
        }
    }

    fn refresh_devices(&self) {
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = adb::devices().map_err(|error| error.to_string());
            let _ = sender.send(Event::Devices(result));
        });
    }

    fn selected_serial(&self) -> Option<String> {
        self.selected_serial.clone()
    }

    fn run_device_action<F>(&mut self, label: &'static str, action: F)
    where
        F: FnOnce(String) -> Result<String, String> + Send + 'static,
    {
        let Some(serial) = self.selected_serial() else {
            self.log("Bitte zuerst ein verbundenes Gerät auswählen.");
            return;
        };
        self.busy = true;
        self.log(format!("{label} …"));
        let sender = self.sender.clone();
        thread::spawn(move || {
            let message = match action(serial) {
                Ok(details) => format!("{label}: {details}"),
                Err(error) => format!("{label} fehlgeschlagen: {error}"),
            };
            let _ = sender.send(Event::Log(message));
            let _ = sender.send(Event::Busy(false));
        });
    }

    // Reserved for the combined action; no standalone dashboard entry for now.
    #[allow(dead_code)]
    fn start_claim_rewards(&mut self) {
        let Some(serial) = self.selected_serial() else {
            self.log("Bitte zuerst ein verbundenes Gerät auswählen.");
            return;
        };
        let claim_stage_rewards = self.claim_stage_rewards;
        self.activity.start(GameMode::AfkStages, Instant::now());
        self.busy = true;
        self.log("AFK-Belohnungen gestartet. AFK-Stufen im Spiel geöffnet lassen.");
        let sender = self.sender.clone();
        thread::spawn(move || {
            let _ = sender.send(Event::Activity(ActivityEvent::ModeEntered(
                GameMode::AfkStages,
            )));
            let result = (|| -> Result<(), String> {
                for round in 0..3 {
                    adb::tap(&serial, 520, 1400).map_err(|error| error.to_string())?;
                    thread::sleep(Duration::from_secs(1));
                    adb::tap(&serial, 550, 1080).map_err(|error| error.to_string())?;
                    let _ = sender.send(Event::Log(format!(
                        "AFK-Belohnungsschritt {}/3 eingesammelt.",
                        round + 1
                    )));
                    thread::sleep(Duration::from_secs(1));
                }
                thread::sleep(Duration::from_secs(1));
                adb::tap(&serial, 550, 1080).map_err(|error| error.to_string())?;
                if claim_stage_rewards {
                    for round in 0..5 {
                        adb::tap(&serial, 770, 500).map_err(|error| error.to_string())?;
                        thread::sleep(Duration::from_millis(300));
                        adb::tap(&serial, 770, 500).map_err(|error| error.to_string())?;
                        let _ = sender.send(Event::Log(format!(
                            "Stufenbelohnung {}/5 eingesammelt.",
                            round + 1
                        )));
                        thread::sleep(Duration::from_secs(1));
                    }
                }
                Ok(())
            })();
            let message = match result {
                Ok(()) => "AFK-Belohnungen abgeschlossen.".to_owned(),
                Err(error) => format!("AFK-Belohnungen abgebrochen: {error}"),
            };
            let _ = sender.send(Event::Log(message));
            let _ = sender.send(Event::Busy(false));
        });
    }

    fn start_stages(&mut self) {
        self.stage_error = None;
        let selection = self.stage_selection;
        let Some(mode) = selection.modes().first().copied() else {
            self.stage_error = Some("Bitte mindestens eine Stage-Art auswählen.".into());
            return;
        };
        let Some(serial) = self.selected_serial() else {
            self.stage_error = Some("Bitte zuerst unter Einstellungen ein Gerät verbinden.".into());
            return;
        };
        self.activity.start(mode, Instant::now());
        self.stage_error = None;
        let battle_settings = crate::afk_journey::afk_stages::BattleSettings {
            mode,
            use_suggested_formations: self.suggested_formations,
            current_team_first: self.current_team_first,
            max_formations: self.max_formations.clamp(1, 10),
            attempts_per_formation: self.attempts_per_formation.clamp(1, 10),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        self.stage_cancel = Some(cancelled.clone());
        self.busy = true;
        self.log(format!(
            "{} gestartet · läuft bis keine weitere Stufe geschafft werden kann.",
            mode.label()
        ));
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = crate::afk_journey::afk_stages::run_selected(
                &serial,
                battle_settings,
                selection,
                &cancelled,
                |message| {
                    let _ = sender.send(Event::Log(message));
                },
                |progress| {
                    let _ = sender.send(Event::Activity(progress));
                },
            );
            let message = match &result {
                Ok(cleared) => format!("AFK-Stages-Lauf abgeschlossen · {cleared} geschafft."),
                Err(error) => format!("AFK-Stages-Lauf beendet: {error}"),
            };
            let _ = sender.send(Event::Log(message));
            let error = if cancelled.load(Ordering::Relaxed) {
                None
            } else {
                result.err()
            };
            let _ = sender.send(Event::StagesFinished(error));
            let _ = sender.send(Event::Busy(false));
        });
    }

    fn receive_events(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Devices(Ok(devices)) => {
                    if self
                        .selected_serial
                        .as_ref()
                        .is_none_or(|serial| !devices.iter().any(|device| &device.serial == serial))
                    {
                        self.selected_serial = devices.first().map(|device| device.serial.clone());
                    }
                    if devices.is_empty() {
                        self.log("Keine autorisierten ADB-Geräte gefunden.");
                    } else {
                        self.log(format!(
                            "{} autorisierte(s) ADB-Gerät(e) gefunden.",
                            devices.len()
                        ));
                    }
                    self.devices = devices;
                }
                Event::Devices(Err(error)) => {
                    self.log(format!("ADB-Suche fehlgeschlagen: {error}"))
                }
                Event::Log(message) => self.log(message),
                Event::Activity(event) => self.activity.apply(event),
                Event::StagesFinished(error) => self.stage_error = error,
                Event::UpdateChecked(result) => {
                    self.update_checking = false;
                    self.update_checked = true;
                    match result {
                        Ok(release) => {
                            self.available_update = release;
                            self.update_error = None;
                        }
                        Err(error) => self.update_error = Some(error),
                    }
                }
                Event::UpdateProgress(percent) => self.update_progress = Some(percent),
                Event::UpdateReady(Ok(())) => self.close_for_update = true,
                Event::UpdateReady(Err(error)) => {
                    self.busy = false;
                    self.update_progress = None;
                    self.update_error = Some(error);
                }
                Event::Busy(busy) => {
                    self.busy = busy;
                    if !busy {
                        self.activity.finish(Instant::now());
                        self.stage_cancel = None;
                    }
                }
            }
        }
    }

    fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        let full_width = ui.available_width();
        let usable_width = (full_width - SpaceTokens::MD * 2.0).max(1.0);
        let left_width = usable_width * 0.34;
        let center_width = usable_width * 0.30;
        let activity_width = usable_width - left_width - center_width;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                egui::vec2(left_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.x = SpaceTokens::SM;
                    ui.heading(egui::RichText::new("Aktionen").color(ColorTokens::TEXT_PRIMARY));
                    ui.add_space(SpaceTokens::SM);
                    let active = self.stage_cancel.is_some();
                    design_system::state_card(ui, active, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("AFK-Stages").size(14.0).color(ColorTokens::TEXT_PRIMARY));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if active {
                                    status_badge(ui, "LÄUFT", true);
                                }
                            });
                        });
                        ui.label("Ausgewählte Stages pushen");
                        ui.add_space(SpaceTokens::SM);
                        ui.add_enabled_ui(!self.busy, |ui| {
                            ui.spacing_mut().interact_size.y = 20.0;
                            ui.horizontal(|ui| {
                            design_system::active_checkbox(ui, &mut self.stage_selection.season, "Season-Stages");
                            design_system::active_checkbox(ui, &mut self.stage_selection.normal, "Normale Stages");
                            });
                        });
                        if self.stage_selection.season && self.stage_selection.normal {
                            ui.small("Zuerst Season, dann Normal");
                        }
                        ui.add_space(SpaceTokens::SM);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!self.busy && (self.stage_selection.season || self.stage_selection.normal), primary_button("Starten")).clicked() {
                                self.start_stages();
                            }
                            if let Some(cancel) = self.stage_cancel.as_ref().filter(|_| active) {
                                let cancel = cancel.clone();
                                if ui.add(design_system::stop_button("Stop")).clicked() {
                                    cancel.store(true, Ordering::Relaxed);
                                    self.log("Stop für den gesamten AFK-Stages-Lauf angefordert.");
                                }
                            }
                        });
                        if let Some(error) = self.stage_error.as_ref() {
                            ui.add(egui::Label::new(egui::RichText::new(error).color(ColorTokens::DANGER_HOVER)).wrap());
                        }
                    });
                    ui.add_space(SpaceTokens::MD);

                },
            );

            ui.add_space(SpaceTokens::MD);
            ui.allocate_ui_with_layout(
                egui::vec2(center_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.x = SpaceTokens::SM;
                    ui.heading(egui::RichText::new("Statistiken").color(ColorTokens::TEXT_PRIMARY));
                    ui.add_space(SpaceTokens::SM);
                    card(ui, |ui| {
                        stat_value(ui, "LAUFZEIT", &format_duration(self.activity.elapsed(Instant::now())));
                        ui.small("Aktueller Lauf inkl. Navigation");
                        ui.add_space(SpaceTokens::SM);
                        crate::charts::stage_history(ui, &self.activity);
                    });
                    ui.add_space(SpaceTokens::MD);
                    card(ui, |ui| {
                        let rate = self.activity.success_rate().map(|r| format!("{r:.0} %")).unwrap_or_else(|| "—".into());
                        stat_value(ui, "ERFOLGSQUOTE", &rate);
                        let (wins, losses) = self.activity.outcomes();
                        ui.small(format!("{wins} Siege · {losses} Niederlagen"))
                            .on_hover_text("Siege / entschiedene Kämpfe. Laufende und unklare Kämpfe werden nicht eingerechnet.");
                    });
                    ui.add_space(SpaceTokens::MD);
                    card(ui, |ui| {
                        let average = self.activity.attempts_per_clear().map(|a| format!("{a:.1}").replace('.', ",")).unwrap_or_else(|| "—".into());
                        stat_value(ui, "Ø VERSUCHE PRO STAGE", &average);
                        ui.small("Nur abgeschlossene Stufen")
                            .on_hover_text("Versuche bis zur letzten geschafften Stufe / geschaffte Stufen. Versuche an einer noch offenen Stufe werden ausgeschlossen.");
                    });
                    ui.add_space(SpaceTokens::MD);
                    card(ui, |ui| crate::charts::mode_comparison(ui, &self.activity));
                },
            );

            ui.add_space(SpaceTokens::MD);
            ui.allocate_ui_with_layout(
                egui::vec2(activity_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.x = SpaceTokens::SM;
                    ui.heading(egui::RichText::new("Aktivität").color(ColorTokens::TEXT_PRIMARY));
                    ui.add_space(SpaceTokens::SM);
                    card(ui, |ui| {
                        ui.small(if self.activity.is_running() { "AKTUELLER SPIELMODUS" } else { "LETZTER SPIELMODUS" });
                        match self.activity.current_mode {
                            Some(mode) => { ui.label(egui::RichText::new(mode.label()).size(14.0).color(ColorTokens::GLOW)); }
                            None if self.activity.is_running() => {
                                ui.label("Spielmodus wird geöffnet …");
                                if let Some(target) = self.activity.target { ui.small(format!("Ziel: {}", target.label())); }
                            }
                            None => { ui.label("Noch kein Modus gestartet"); }
                        }
                    });
                    for (mode, stats) in &self.activity.modes {
                        ui.add_space(SpaceTokens::MD);
                        let active = self.activity.is_running() && self.activity.current_mode == Some(*mode);
                        design_system::state_card(ui, active, |ui| {
                            ui.label(egui::RichText::new(mode.label()).color(if active { ColorTokens::GLOW } else { ColorTokens::TEXT_PRIMARY }));
                            ui.add_space(SpaceTokens::SM);
                            ui.separator();
                            ui.horizontal(|ui| {
                                progress_metric(ui, "VERSUCHE", stats.attempts);
                                ui.add_space(SpaceTokens::XL);
                                progress_metric(ui, "STAGES GECLEART", stats.cleared);
                            });
                        });
                    }
                },
            );
        });
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(egui::RichText::new("Einstellungen").color(ColorTokens::TEXT_PRIMARY));
        ui.label("Verbindung und Ablauf der AFK-Journey-Automationen konfigurieren.");
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong("App-Updates");
            ui.label(format!("ADBHelper {}", updater::VERSION));
            ui.hyperlink_to("GitHub-Repository", updater::REPOSITORY_URL);
            ui.small("Automatische Prüfung beim Start und alle 6 Stunden.");
            let cooldown = self
                .last_update_check
                .is_some_and(|last| last.elapsed() < Duration::from_secs(30));
            if ui
                .add_enabled(
                    !self.update_checking && self.update_progress.is_none() && !cooldown,
                    egui::Button::new("Auf Updates prüfen"),
                )
                .clicked()
            {
                self.check_updates();
            }
            if self.update_checking {
                ui.label("Updates werden geprüft …");
            } else if self.update_checked
                && self.available_update.is_none()
                && self.update_error.is_none()
            {
                ui.label("Du verwendest die aktuelle Version.");
            }
            if let Some(error) = &self.update_error {
                ui.add(egui::Label::new(error).wrap());
            }
        });
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong("Geräteverbindung");
            ui.label("Gerät auswählen und Verbindung verwalten.");
            ui.add_space(SpaceTokens::SM);
            ui.horizontal(|ui| {
                if self.selected_serial.is_some() {
                    status_badge(ui, "VERBUNDEN", true);
                } else {
                    ui.colored_label(MUTED, "Kein Gerät verbunden");
                }
                if ui.small_button("Geräte suchen").clicked() {
                    self.refresh_devices();
                }
            });
            let selected_label = self
                .selected_serial
                .clone()
                .unwrap_or_else(|| "Gerät auswählen".to_owned());
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
            ui.strong("ADB-Pfad");
            ui.small("Leer lassen, um ADB aus PATH zu verwenden.");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.adb_path)
                        .desired_width((ui.available_width() - 150.0).max(120.0)),
                );
                if ui.button("Pfad übernehmen").clicked() {
                    if self.adb_path.trim().is_empty() {
                        std::env::remove_var("ADB_PATH");
                        self.log("ADB aus PATH wird verwendet.");
                    } else {
                        std::env::set_var("ADB_PATH", self.adb_path.trim());
                        self.log(format!("ADB-Pfad gesetzt: {}", self.adb_path.trim()));
                    }
                    self.refresh_devices();
                }
            });
            ui.add_space(SpaceTokens::SM);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("AFK Journey starten"))
                    .clicked()
                {
                    self.run_device_action("AFK Journey gestartet", |serial| {
                        adb::launch_afk_journey(&serial).map_err(|error| error.to_string())
                    });
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Spiel beenden"))
                    .clicked()
                {
                    self.run_device_action("AFK Journey beendet", |serial| {
                        adb::stop_afk_journey(&serial).map_err(|error| error.to_string())
                    });
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Zurück"))
                    .clicked()
                {
                    self.run_device_action("Zurück-Taste gesendet", |serial| {
                        adb::press_back(&serial)
                            .map(|_| "an das Gerät gesendet".to_owned())
                            .map_err(|error| error.to_string())
                    });
                }
            });
        });
        ui.add_space(SpaceTokens::GAP);
        card(ui, |ui| {
            ui.strong("AFK-Stages und Season-AFK-Stages");
            ui.label("Teamwahl und Kampfversuche.");
            ui.add_space(SpaceTokens::SM);
            design_system::active_checkbox(
                ui,
                &mut self.suggested_formations,
                "Empfohlene Teams aus den Ranglisten übernehmen",
            );
            design_system::active_checkbox(
                ui,
                &mut self.current_team_first,
                "Aktuelles Team zuerst versuchen",
            );
            ui.horizontal(|ui| {
                ui.label("Empfohlene Teams");
                ui.add(egui::DragValue::new(&mut self.max_formations).range(1..=10));
                ui.label("Versuche pro Team");
                ui.add(egui::DragValue::new(&mut self.attempts_per_formation).range(1..=10));
            });
            ui.small("Gesperrte Teams werden übersprungen. Gold wird nicht ausgegeben.");
        });
        ui.add_space(SpaceTokens::GAP);
        card(ui, |ui| {
            ui.strong("AFK-Belohnungen");
            design_system::active_checkbox(
                ui,
                &mut self.claim_stage_rewards,
                "Auch Stufenbelohnungen einsammeln",
            );
            ui.small("Gilt für die Automation „AFK-Belohnungen einsammeln“.");
        });
    }
}

const MUTED: egui::Color32 = design_system::ColorTokens::TEXT_SECONDARY;

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, ColorTokens::BG_VIEWPORT);
        egui::Frame::new()
            .inner_margin(egui::Margin::same(GeometryTokens::APP_INSET))
            .show(ui, |ui| {
                self.receive_events();
                let context = ui.ctx().clone();
                if self.close_for_update {
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                context.request_repaint_after(Duration::from_secs(1));
                if !self.update_checking
                    && self.update_progress.is_none()
                    && self
                        .last_update_check
                        .is_none_or(|last| last.elapsed() >= updater::CHECK_INTERVAL)
                {
                    self.check_updates();
                }
                if self.busy {
                    context.request_repaint_after(Duration::from_millis(100));
                }

                egui::Frame::new()
                    .fill(ColorTokens::BG_VIEWPORT)
                    .inner_margin(egui::Margin::symmetric(
                        GeometryTokens::TITLEBAR_X,
                        GeometryTokens::TITLEBAR_Y,
                    ))
                    .show(ui, |bar| {
                        bar.set_width(bar.available_width());
                        bar.horizontal(|ui| {
                            ui.add(egui::Image::new(&self.logo).fit_to_exact_size(egui::vec2(
                                GeometryTokens::BRAND_MARK_WIDTH,
                                GeometryTokens::BRAND_MARK_HEIGHT,
                            )));
                            ui.label(
                                egui::RichText::new("ADBHelper")
                                    .strong()
                                    .size(TypeTokens::BRAND),
                            );
                            ui.add_space(SpaceTokens::NAV_BRAND);
                            if nav_tab(
                                ui,
                                self.page == Page::Automations,
                                "Dashboard",
                                GeometryTokens::NAV_DASHBOARD_WIDTH,
                            ) {
                                self.page = Page::Automations;
                            }
                            if nav_tab(
                                ui,
                                self.page == Page::Settings,
                                "Einstellungen",
                                GeometryTokens::NAV_SETTINGS_WIDTH,
                            ) {
                                self.page = Page::Settings;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let context = ui.ctx().clone();
                                    if window_button(ui, WindowButton::Close).clicked() {
                                        context.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                    let maximized = context
                                        .input(|input| input.viewport().maximized.unwrap_or(false));
                                    if window_button(ui, WindowButton::Maximize { maximized })
                                        .clicked()
                                    {
                                        context.send_viewport_cmd(
                                            egui::ViewportCommand::Maximized(!maximized),
                                        );
                                    }
                                    if window_button(ui, WindowButton::Minimize).clicked() {
                                        context.send_viewport_cmd(
                                            egui::ViewportCommand::Minimized(true),
                                        );
                                    }
                                    if self.busy {
                                        ui.spinner();
                                        ui.colored_label(
                                            ColorTokens::TEXT_PRIMARY,
                                            if self.update_progress.is_some() {
                                                "Update wird geladen"
                                            } else {
                                                "Automation läuft"
                                            },
                                        );
                                        ui.add_space(SpaceTokens::SM);
                                    }
                                    if let Some(serial) = &self.selected_serial {
                                        status_badge(ui, &format!("VERBUNDEN · {serial}"), true);
                                        ui.add_space(SpaceTokens::MD);
                                    }
                                    let drag = ui.allocate_response(
                                        egui::vec2(
                                            ui.available_width().max(0.0),
                                            GeometryTokens::WINDOW_CONTROL_SIZE,
                                        ),
                                        egui::Sense::click_and_drag(),
                                    );
                                    if drag.drag_started() {
                                        context.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(SpaceTokens::TIGHT);
                ui.separator();
                ui.add_space(SpaceTokens::GAP);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.show_update_banner(ui);
                        match self.page {
                            Page::Automations => self.show_dashboard(ui),
                            Page::Settings => self.show_settings(ui),
                        }
                    });
            });
    }
}

fn nav_tab(ui: &mut egui::Ui, selected: bool, label: &str, width: f32) -> bool {
    let text = if selected {
        egui::RichText::new(label).color(ColorTokens::TEXT_PRIMARY)
    } else {
        egui::RichText::new(label).color(MUTED)
    };
    let light = ui.painter().add(egui::Shape::Noop);
    let response = ui.add_sized(
        egui::vec2(width, GeometryTokens::TITLEBAR_HEIGHT),
        egui::Button::new(text).frame(false),
    );
    if selected {
        ui.painter()
            .set(light, design_system::active_glow_mesh(response.rect));
        let y = response.rect.bottom() - 0.5;
        ui.painter().line_segment(
            [
                egui::pos2(response.rect.center().x - width * 0.28, y),
                egui::pos2(response.rect.center().x + width * 0.28, y),
            ],
            egui::Stroke::new(0.7, egui::Color32::from_rgb(40, 112, 122)),
        );
    }
    response.clicked()
}

fn progress_metric(ui: &mut egui::Ui, label: &str, value: u64) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(value.to_string())
                .size(TypeTokens::METRIC)
                .color(ColorTokens::GLOW),
        );
        ui.add_space(SpaceTokens::XS);
        ui.label(
            egui::RichText::new(label)
                .size(TypeTokens::KBD)
                .color(MUTED),
        );
    });
}

fn stat_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(TypeTokens::KBD)
            .color(MUTED),
    );
    ui.add_space(SpaceTokens::SM);
    ui.label(
        egui::RichText::new(value)
            .size(TypeTokens::METRIC)
            .color(ColorTokens::GLOW),
    );
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[derive(Clone, Copy)]
enum WindowButton {
    Minimize,
    Maximize { maximized: bool },
    Close,
}

fn window_button(ui: &mut egui::Ui, kind: WindowButton) -> egui::Response {
    let response = ui.add_sized(
        egui::vec2(
            GeometryTokens::WINDOW_CONTROL_SIZE,
            GeometryTokens::WINDOW_CONTROL_SIZE,
        ),
        egui::Button::new("").frame(false),
    );
    let color = if matches!(kind, WindowButton::Close) && response.hovered() {
        ColorTokens::CLOSE_HOVER
    } else if response.hovered() {
        ColorTokens::TEXT_PRIMARY
    } else {
        MUTED
    };
    let center = response.rect.center();
    let painter = ui.painter();
    match kind {
        WindowButton::Minimize => {
            painter.line_segment(
                [
                    egui::pos2(center.x - 5.0, center.y + 2.0),
                    egui::pos2(center.x + 5.0, center.y + 2.0),
                ],
                egui::Stroke::new(1.4, color),
            );
        }
        WindowButton::Maximize { maximized: false } => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center, egui::vec2(10.0, 10.0)),
                0.5,
                egui::Stroke::new(1.2, color),
                egui::StrokeKind::Inside,
            );
        }
        WindowButton::Maximize { maximized: true } => {
            let back = egui::Rect::from_min_size(
                egui::pos2(center.x - 3.0, center.y - 5.0),
                egui::vec2(8.0, 8.0),
            );
            let front = egui::Rect::from_min_size(
                egui::pos2(center.x - 5.0, center.y - 2.0),
                egui::vec2(8.0, 8.0),
            );
            painter.rect_stroke(
                back,
                0.5,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(front, 0.5, ColorTokens::BG_PANEL);
            painter.rect_stroke(
                front,
                0.5,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
        }
        WindowButton::Close => {
            painter.line_segment(
                [
                    egui::pos2(center.x - 4.0, center.y - 4.0),
                    egui::pos2(center.x + 4.0, center.y + 4.0),
                ],
                egui::Stroke::new(1.4, color),
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + 4.0, center.y - 4.0),
                    egui::pos2(center.x - 4.0, center.y + 4.0),
                ],
                egui::Stroke::new(1.4, color),
            );
        }
    }
    response
}
