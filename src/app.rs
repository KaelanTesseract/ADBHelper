use crate::activity::{ActivityEvent, GameMode, LegendTower, RunActivity};
use crate::adb::{self, Device};
use crate::afk_journey::afk_stages::StageSelection;
use crate::afk_journey::legend_trial::TowerSelection;
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
    StagesFinished(StageAction, Option<String>),
    UpdateChecked(Result<Option<updater::Release>, String>),
    UpdateCheckStarted,
    UpdateProgress(u8),
    UpdateReady(Result<(), String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Automations,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StageAction {
    AfkStages,
    SeasonLegendTrial,
    UnionCampaign,
    DurasTrials,
}

impl StageAction {
    fn index(self) -> usize {
        match self {
            Self::AfkStages => 0,
            Self::SeasonLegendTrial => 1,
            Self::UnionCampaign => 2,
            Self::DurasTrials => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::AfkStages => "AFK-Stages",
            Self::SeasonLegendTrial => "Season Legend Trial",
            Self::UnionCampaign => "Union Campaign",
            Self::DurasTrials => "Dura’s Trials",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActionBattleOptions {
    suggested_formations: bool,
    current_team_first: bool,
    skip_manual_formations: bool,
    max_formations: u32,
    attempts_per_formation: u32,
}

impl Default for ActionBattleOptions {
    fn default() -> Self {
        Self {
            suggested_formations: true,
            current_team_first: true,
            skip_manual_formations: false,
            max_formations: 10,
            attempts_per_formation: 5,
        }
    }
}

impl ActionBattleOptions {
    fn battle_settings(self, mode: GameMode) -> crate::afk_journey::afk_stages::BattleSettings {
        crate::afk_journey::afk_stages::BattleSettings {
            mode,
            use_suggested_formations: self.suggested_formations,
            current_team_first: self.current_team_first,
            skip_manual_formations: self.skip_manual_formations,
            max_formations: self.max_formations.clamp(1, 10),
            attempts_per_formation: self.attempts_per_formation.clamp(1, 10),
        }
    }
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
    tower_selection: TowerSelection,
    dura_sweep_after_cap: bool,
    active_action: Option<StageAction>,
    battle_options: [ActionBattleOptions; 4],
    action_settings_dialog: Option<StageAction>,
    logo: egui::TextureHandle,
    activity: RunActivity,
    action_error: Option<(StageAction, String)>,
    available_update: Option<updater::Release>,
    update_checking: bool,
    update_checked: bool,
    last_update_check: Option<Instant>,
    update_progress: Option<u8>,
    update_error: Option<String>,
    update_notice: Option<String>,
    close_for_update: bool,
    update_dialog_open: bool,
    update_requests: Sender<()>,
}

impl PlayerApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (update_requests, update_receiver) = mpsc::channel();
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
            tower_selection: TowerSelection::default(),
            dura_sweep_after_cap: true,
            active_action: None,
            battle_options: [ActionBattleOptions::default(); 4],
            action_settings_dialog: None,
            logo,
            activity: RunActivity::default(),
            action_error: None,
            available_update: None,
            update_checking: true,
            update_checked: false,
            last_update_check: None,
            update_progress: None,
            update_error: None,
            update_notice: updater::startup_notice(),
            close_for_update: false,
            update_dialog_open: false,
            update_requests,
        };
        app.log("AFK Journey · App gestartet. Verbinde ein Android-Gerät mit aktiviertem ADB-Debugging.");
        app.refresh_devices();
        let update_sender = app.sender.clone();
        let update_context = context.egui_ctx.clone();
        thread::spawn(move || {
            updater::watch_updates(update_receiver, |event| {
                let message = match event {
                    updater::CheckEvent::Started => Event::UpdateCheckStarted,
                    updater::CheckEvent::Finished(result) => Event::UpdateChecked(result),
                };
                let _ = update_sender.send(message);
                update_context.request_repaint();
            })
        });
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
        if self.update_requests.send(()).is_err() {
            self.update_checking = false;
            self.update_error = Some("Der Update-Dienst ist nicht verfügbar.".into());
        }
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

    fn show_update_notice(&mut self, ui: &mut egui::Ui) {
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
    }

    fn show_update_dialog(&mut self, context: &egui::Context) {
        if !self.update_dialog_open {
            return;
        }
        let Some(release) = self.available_update.clone() else {
            self.update_dialog_open = false;
            return;
        };
        let mut close = false;
        let response = egui::Modal::new(egui::Id::unique("app-update-dialog"))
            .frame(design_system::panel_frame())
            .show(context, |ui| {
                ui.set_width((context.content_rect().width() - 64.0).clamp(260.0, 560.0));
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Update {} verfügbar", release.version))
                            .color(ColorTokens::GLOW),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                });
                ui.label(format!(
                    "Installiert: {} · Neu: {}",
                    updater::VERSION,
                    release.version
                ));
                ui.strong("Patchnotes");
                egui::ScrollArea::vertical()
                    .id_salt("update-patchnotes")
                    .max_height((context.content_rect().height() - 260.0).clamp(80.0, 360.0))
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
                ui.add_space(SpaceTokens::MD);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.busy, primary_button("Herunterladen und installieren"))
                        .clicked()
                    {
                        self.install_update();
                    }
                    ui.hyperlink_to("Release auf GitHub", &release.url);
                });
                ui.small("Nach der Installation startet ADBHelper automatisch neu.");
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
        if close || response.should_close() {
            self.update_dialog_open = false;
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

    fn start_action(&mut self, action: StageAction) {
        if self.busy {
            return;
        }
        self.action_error = None;
        let selection = self.stage_selection;
        let towers = self.tower_selection;
        let dura_sweep_after_cap = self.dura_sweep_after_cap;
        let mode = match action {
            StageAction::AfkStages => selection.modes().first().copied(),
            StageAction::SeasonLegendTrial => towers
                .selected()
                .first()
                .copied()
                .map(GameMode::SeasonLegendTrial),
            StageAction::UnionCampaign => Some(GameMode::UnionCampaign),
            StageAction::DurasTrials => Some(GameMode::DurasTrials),
        };
        let Some(mode) = mode else {
            self.action_error = Some((
                action,
                match action {
                    StageAction::AfkStages => "Bitte mindestens eine Stage-Art auswählen.",
                    StageAction::SeasonLegendTrial => {
                        "Bitte über das Zahnrad mindestens einen Turm auswählen."
                    }
                    StageAction::UnionCampaign => "Union Campaign ist nicht verfügbar.",
                    StageAction::DurasTrials => "Dura’s Trials sind nicht verfügbar.",
                }
                .into(),
            ));
            return;
        };
        let Some(serial) = self.selected_serial() else {
            self.action_error = Some((
                action,
                "Bitte zuerst unter Einstellungen ein Gerät verbinden.".into(),
            ));
            return;
        };
        self.activity.start(mode, Instant::now());
        let battle_settings = self.battle_options[action.index()].battle_settings(mode);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.stage_cancel = Some(cancelled.clone());
        self.active_action = Some(action);
        self.busy = true;
        self.log(format!("{} gestartet.", action.label()));
        let sender = self.sender.clone();
        thread::spawn(move || {
            let log = |message| {
                let _ = sender.send(Event::Log(message));
            };
            let progress = |progress| {
                let _ = sender.send(Event::Activity(progress));
            };
            let result = match action {
                StageAction::AfkStages => crate::afk_journey::afk_stages::run_selected(
                    &serial,
                    battle_settings,
                    selection,
                    &cancelled,
                    log,
                    progress,
                ),
                StageAction::SeasonLegendTrial => crate::afk_journey::legend_trial::run(
                    &serial,
                    battle_settings,
                    towers,
                    &cancelled,
                    log,
                    progress,
                ),
                StageAction::UnionCampaign => crate::afk_journey::union_campaign::run(
                    &serial,
                    battle_settings,
                    &cancelled,
                    log,
                    progress,
                ),
                StageAction::DurasTrials => crate::afk_journey::duras_trials::run(
                    &serial,
                    battle_settings,
                    dura_sweep_after_cap,
                    &cancelled,
                    log,
                    progress,
                ),
            };
            let message = match &result {
                Ok(cleared) => format!("{} abgeschlossen · {cleared} geschafft.", action.label()),
                Err(error) => format!("{} beendet: {error}", action.label()),
            };
            let _ = sender.send(Event::Log(message));
            let error = if cancelled.load(Ordering::Relaxed) {
                None
            } else {
                result.err()
            };
            let _ = sender.send(Event::StagesFinished(action, error));
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
                Event::StagesFinished(action, error) => {
                    self.action_error = error.map(|error| (action, error))
                }
                Event::UpdateChecked(result) => {
                    self.update_checking = false;
                    self.update_checked = true;
                    if self.update_progress.is_none() {
                        match result {
                            Ok(release) => {
                                self.available_update = release;
                                self.update_error = None;
                            }
                            Err(error) => self.update_error = Some(error),
                        }
                    }
                }
                Event::UpdateCheckStarted => {
                    self.update_checking = true;
                    self.last_update_check = Some(Instant::now());
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
                        self.active_action = None;
                    }
                }
            }
        }
    }

    fn show_dashboard(&mut self, ui: &mut egui::Ui, dashboard_height: f32) {
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
                    let active = self.active_action == Some(StageAction::AfkStages)
                        && self.stage_cancel.is_some();
                    design_system::state_card(ui, active, |ui| {
                        self.show_action_header(ui, StageAction::AfkStages, active);
                        ui.label("Ausgewählte Stages pushen");
                        ui.add_space(SpaceTokens::SM);
                        ui.add_enabled_ui(!self.busy, |ui| {
                            ui.spacing_mut().interact_size.y = 20.0;
                            ui.horizontal(|ui| {
                                design_system::active_checkbox(
                                    ui,
                                    &mut self.stage_selection.season,
                                    "Season-Stages",
                                );
                                design_system::active_checkbox(
                                    ui,
                                    &mut self.stage_selection.normal,
                                    "Normale Stages",
                                );
                            });
                        });
                        if self.stage_selection.season && self.stage_selection.normal {
                            ui.small("Zuerst Season, dann Normal");
                        }
                        ui.add_space(SpaceTokens::SM);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !self.busy
                                        && (self.stage_selection.season
                                            || self.stage_selection.normal),
                                    primary_button("Starten"),
                                )
                                .clicked()
                            {
                                self.start_action(StageAction::AfkStages);
                            }
                            if let Some(cancel) = self.stage_cancel.as_ref().filter(|_| active) {
                                let cancel = cancel.clone();
                                if ui.add(design_system::stop_button("Stop")).clicked() {
                                    cancel.store(true, Ordering::Relaxed);
                                    self.log("Stop für den gesamten AFK-Stages-Lauf angefordert.");
                                }
                            }
                        });
                        if let Some((StageAction::AfkStages, error)) = self.action_error.as_ref() {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(error).color(ColorTokens::DANGER_HOVER),
                                )
                                .wrap(),
                            );
                        }
                    });
                    ui.add_space(SpaceTokens::MD);
                    let legend_active = self.active_action == Some(StageAction::SeasonLegendTrial)
                        && self.stage_cancel.is_some();
                    design_system::state_card(ui, legend_active, |ui| {
                        self.show_action_header(ui, StageAction::SeasonLegendTrial, legend_active);
                        ui.label("Verfügbare Fraktionstürme pushen");
                        ui.small(format!(
                            "{} Türme ausgewählt",
                            self.tower_selection.selected().len()
                        ));
                        ui.add_space(SpaceTokens::SM);
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !self.busy && !self.tower_selection.selected().is_empty(),
                                    primary_button("Starten"),
                                )
                                .clicked()
                            {
                                self.start_action(StageAction::SeasonLegendTrial);
                            }
                            if legend_active {
                                if let Some(cancel) = self.stage_cancel.clone() {
                                    if ui.add(design_system::stop_button("Stop")).clicked() {
                                        cancel.store(true, Ordering::Relaxed);
                                        self.log(
                                            "Stop für den gesamten Legend-Trial-Lauf angefordert.",
                                        );
                                    }
                                }
                            }
                        });
                        if let Some((StageAction::SeasonLegendTrial, error)) =
                            self.action_error.as_ref()
                        {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(error).color(ColorTokens::DANGER_HOVER),
                                )
                                .wrap(),
                            );
                        }
                    });
                    ui.add_space(SpaceTokens::MD);
                    let union_active = self.active_action == Some(StageAction::UnionCampaign)
                        && self.stage_cancel.is_some();
                    design_system::state_card(ui, union_active, |ui| {
                        self.show_action_header(ui, StageAction::UnionCampaign, union_active);
                        ui.label("Gilden-Etagen pushen");
                        ui.add_space(SpaceTokens::SM);
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!self.busy, primary_button("Starten"))
                                .clicked()
                            {
                                self.start_action(StageAction::UnionCampaign);
                            }
                            if union_active {
                                if let Some(cancel) = self.stage_cancel.clone() {
                                    if ui.add(design_system::stop_button("Stop")).clicked() {
                                        cancel.store(true, Ordering::Relaxed);
                                        self.log("Stop für die Union Campaign angefordert.");
                                    }
                                }
                            }
                        });
                        if let Some((StageAction::UnionCampaign, error)) =
                            self.action_error.as_ref()
                        {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(error).color(ColorTokens::DANGER_HOVER),
                                )
                                .wrap(),
                            );
                        }
                    });
                    ui.add_space(SpaceTokens::MD);
                    let dura_active = self.active_action == Some(StageAction::DurasTrials)
                        && self.stage_cancel.is_some();
                    design_system::state_card(ui, dura_active, |ui| {
                        self.show_action_header(ui, StageAction::DurasTrials, dura_active);
                        ui.label("Stufen pushen · Sweep nur am bestätigten Maximum");
                        ui.add_space(SpaceTokens::SM);
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!self.busy, primary_button("Starten"))
                                .clicked()
                            {
                                self.start_action(StageAction::DurasTrials);
                            }
                            if dura_active {
                                if let Some(cancel) = self.stage_cancel.clone() {
                                    if ui.add(design_system::stop_button("Stop")).clicked() {
                                        cancel.store(true, Ordering::Relaxed);
                                        self.log("Stop für Dura’s Trials angefordert.");
                                    }
                                }
                            }
                        });
                        if let Some((StageAction::DurasTrials, error)) = self.action_error.as_ref()
                        {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(error).color(ColorTokens::DANGER_HOVER),
                                )
                                .wrap(),
                            );
                        }
                    });
                },
            );

            ui.add_space(SpaceTokens::MD);
            ui.allocate_ui_with_layout(
                egui::vec2(center_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    crate::charts::statistics(ui, &self.activity, dashboard_height);
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
                        ui.small(if self.activity.is_running() {
                            "AKTUELLER SPIELMODUS"
                        } else {
                            "LETZTER SPIELMODUS"
                        });
                        match self.activity.current_mode {
                            Some(mode) => {
                                ui.label(
                                    egui::RichText::new(mode.label())
                                        .size(14.0)
                                        .color(ColorTokens::GLOW),
                                );
                            }
                            None if self.activity.is_running() => {
                                ui.label("Spielmodus wird geöffnet …");
                                if let Some(target) = self.activity.target {
                                    ui.small(format!("Ziel: {}", target.label()));
                                }
                            }
                            None => {
                                ui.label("Noch kein Modus gestartet");
                            }
                        }
                    });
                    for (mode, stats) in &self.activity.modes {
                        ui.add_space(SpaceTokens::MD);
                        let active =
                            self.activity.is_running() && self.activity.current_mode == Some(*mode);
                        design_system::state_card(ui, active, |ui| {
                            ui.label(egui::RichText::new(mode.label()).color(if active {
                                ColorTokens::GLOW
                            } else {
                                ColorTokens::TEXT_PRIMARY
                            }));
                            ui.add_space(SpaceTokens::SM);
                            ui.separator();
                            ui.horizontal(|ui| {
                                progress_metric(ui, "VERSUCHE", stats.attempts);
                                ui.add_space(SpaceTokens::MD);
                                progress_metric(ui, "GECLEART", stats.cleared);
                                if *mode == GameMode::DurasTrials {
                                    ui.add_space(SpaceTokens::SM);
                                    progress_metric(ui, "SWEEPS", stats.sweeps);
                                }
                            });
                        });
                    }
                },
            );
        });
    }

    fn show_action_header(&mut self, ui: &mut egui::Ui, action: StageAction, active: bool) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(action.label())
                    .size(14.0)
                    .color(ColorTokens::TEXT_PRIMARY),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if settings_gear(
                    ui,
                    action.label(),
                    self.action_settings_dialog == Some(action),
                )
                .clicked()
                {
                    self.update_dialog_open = false;
                    self.action_settings_dialog = Some(action);
                }
                if active {
                    status_badge(ui, "LÄUFT", true);
                }
            });
        });
    }

    fn show_action_settings_dialog(&mut self, context: &egui::Context) {
        let Some(action) = self.action_settings_dialog else {
            return;
        };
        let mut close = false;
        let response = egui::Modal::new(egui::Id::unique(("action-settings-dialog", action.index())))
            .frame(design_system::panel_frame())
            .show(context, |ui| {
                ui.set_width((context.content_rect().width() - 64.0).clamp(260.0, 500.0));
                ui.heading(action.label());
                ui.label("Aktionseinstellungen");
                ui.add_space(SpaceTokens::SM);
                ui.separator();
                ui.add_space(SpaceTokens::SM);
                if self.busy {
                    ui.small("Während eines laufenden Vorgangs können die Einstellungen nur angesehen werden.");
                    ui.add_space(SpaceTokens::SM);
                }
                egui::ScrollArea::vertical()
                    .id_salt(("action-settings-content", action.index()))
                    .max_height((context.content_rect().height() - 230.0).clamp(100.0, 540.0))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!self.busy, |ui| {
                            match action {
                                StageAction::AfkStages => {
                                    ui.strong("Stage-Auswahl");
                                    ui.horizontal(|ui| {
                                        design_system::active_checkbox(ui, &mut self.stage_selection.season, "Season-Stages");
                                        design_system::active_checkbox(ui, &mut self.stage_selection.normal, "Normale Stages");
                                    });
                                    ui.small("Bei gemeinsamer Auswahl zuerst Season, danach Normal.");
                                }
                                StageAction::SeasonLegendTrial => {
                                    ui.strong("Fraktionstürme");
                                    ui.label("Heute geschlossene Türme werden automatisch übersprungen.");
                                    egui::Grid::new("legend-trial-towers").num_columns(2).show(ui, |ui| {
                                        for (index, tower) in LegendTower::ALL.into_iter().enumerate() {
                                            design_system::active_checkbox(ui, &mut self.tower_selection.enabled[index], tower.label());
                                            if index % 2 == 1 { ui.end_row(); }
                                        }
                                    });
                                    ui.small("Reihenfolge: Lightbearer, Wilder, Graveborn, Mauler.");
                                }
                                StageAction::DurasTrials => {
                                    ui.strong("Tägliche Sweeps");
                                    design_system::active_checkbox(ui, &mut self.dura_sweep_after_cap,
                                        "Nach bestätigter Maximalstufe tägliche Sweeps ausführen");
                                    ui.small("Nur an der eindeutig bestätigten aktuellen Maximalstufe. Bei unklarer Erkennung wird nicht gesweept. Zusätzliche Vorratstoken werden nicht verwendet.");
                                }
                                StageAction::UnionCampaign => {}
                            }
                            if action != StageAction::UnionCampaign {
                                ui.add_space(SpaceTokens::MD);
                                ui.separator();
                                ui.add_space(SpaceTokens::MD);
                            }
                            show_battle_options(ui, &mut self.battle_options[action.index()]);
                        });
                    });
                ui.add_space(SpaceTokens::MD);
                ui.separator();
                ui.add_space(SpaceTokens::SM);
                ui.small("Änderungen gelten direkt für diese Aktion.");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.button("Schließen").clicked();
                });
            });
        if close || response.should_close() {
            self.action_settings_dialog = None;
        }
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(egui::RichText::new("Einstellungen").color(ColorTokens::TEXT_PRIMARY));
        ui.label("Geräteverbindung und App-Updates verwalten.");
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            ui.strong("App-Updates");
            ui.label(format!("ADBHelper {}", updater::VERSION));
            ui.hyperlink_to("GitHub-Repository", updater::REPOSITORY_URL);
            ui.small(
                "Automatische Prüfung beim Start und alle 5 Minuten, auch bei minimierter App.",
            );
            if self.available_update.is_some()
                && ui.button("Update und Patchnotes anzeigen").clicked()
            {
                self.update_dialog_open = true;
            }
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
                            if let Some(release) = &self.available_update {
                                if update_icon(ui, &release.version).clicked() {
                                    self.update_dialog_open = true;
                                }
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
                let dashboard_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let notice_top = ui.cursor().top();
                        self.show_update_notice(ui);
                        let remaining_height = dashboard_height - (ui.cursor().top() - notice_top);
                        match self.page {
                            Page::Automations => self.show_dashboard(ui, remaining_height),
                            Page::Settings => self.show_settings(ui),
                        }
                    });
            });
        if self.update_dialog_open {
            self.action_settings_dialog = None;
            self.show_update_dialog(ui.ctx());
        } else {
            self.show_action_settings_dialog(ui.ctx());
        }
    }
}

fn show_battle_options(ui: &mut egui::Ui, options: &mut ActionBattleOptions) {
    ui.strong("Teamwahl und Kampfversuche");
    ui.add_space(SpaceTokens::SM);
    design_system::active_checkbox(
        ui,
        &mut options.suggested_formations,
        "Empfohlene Teams aus den Ranglisten übernehmen",
    );
    design_system::active_checkbox(
        ui,
        &mut options.current_team_first,
        "Aktuelles Team zuerst versuchen",
    );
    design_system::active_checkbox(
        ui,
        &mut options.skip_manual_formations,
        "Manuelle Teams überspringen",
    );
    ui.small("Ohne Haken werden manuelle Teams nach den automatischen Vorschlägen versucht.");
    ui.add_space(SpaceTokens::MD);
    egui::Grid::new("battle-attempt-options")
        .num_columns(2)
        .spacing(egui::vec2(SpaceTokens::XL, SpaceTokens::SM))
        .show(ui, |ui| {
            ui.label("Empfohlene Teams");
            ui.add_enabled(
                options.suggested_formations,
                egui::DragValue::new(&mut options.max_formations).range(1..=10),
            );
            ui.end_row();
            ui.label("Versuche pro Team");
            ui.add(egui::DragValue::new(&mut options.attempts_per_formation).range(1..=10));
            ui.end_row();
        });
    ui.add_space(SpaceTokens::SM);
    ui.small("Gesperrte Teams werden übersprungen. Gold wird nicht ausgegeben.");
}

fn settings_gear(ui: &mut egui::Ui, action: &str, active: bool) -> egui::Response {
    let label = format!("{action}: Einstellungen öffnen");
    let response = ui
        .add_sized(egui::vec2(28.0, 28.0), egui::Button::new("").frame(false))
        .on_hover_text(&label)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(response.rect, 3.0, ColorTokens::BG_FIELD);
    }
    if active || response.has_focus() {
        design_system::paint_active_glow(ui, response.rect, 3);
    }
    let color = if active || response.hovered() || response.has_focus() {
        ColorTokens::GLOW
    } else {
        ColorTokens::TEXT_SECONDARY
    };
    let center = response.rect.center();
    // Draw the gear as vectors so it does not depend on font glyph coverage.
    let points = (0..32)
        .map(|index| {
            let angle = std::f32::consts::TAU * index as f32 / 32.0;
            let radius = if matches!(index % 4, 1 | 2) { 8.5 } else { 6.7 };
            center + egui::vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    ui.painter().add(egui::Shape::closed_line(
        points,
        egui::Stroke::new(1.3, color),
    ));
    ui.painter()
        .circle_stroke(center, 2.7, egui::Stroke::new(1.3, color));
    response
}

fn update_icon(ui: &mut egui::Ui, version: &str) -> egui::Response {
    let response = ui
        .add_sized(
            egui::vec2(32.0, GeometryTokens::TITLEBAR_HEIGHT),
            egui::Button::new("").frame(false),
        )
        .on_hover_text(format!("Update {version} verfügbar – Patchnotes anzeigen"));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("Update {version} verfügbar"),
        )
    });
    let color = if response.hovered() {
        egui::Color32::from_rgb(146, 245, 176)
    } else {
        egui::Color32::from_rgb(89, 216, 134)
    };
    let center = response.rect.center();
    let points: Vec<_> = (0..=28)
        .map(|i| {
            let angle = -0.7 + i as f32 / 28.0 * 5.2;
            center + egui::vec2(angle.cos(), angle.sin()) * 7.0
        })
        .collect();
    let end = *points.last().unwrap();
    ui.painter()
        .add(egui::Shape::line(points, egui::Stroke::new(1.6, color)));
    let tangent = egui::vec2(-4.5_f32.sin(), 4.5_f32.cos());
    let normal = egui::vec2(-tangent.y, tangent.x);
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            end + tangent * 3.0,
            end - tangent * 2.0 + normal * 3.0,
            end - tangent * 2.0 - normal * 3.0,
        ],
        color,
        egui::Stroke::NONE,
    ));
    response
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
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(TypeTokens::KBD)
                .color(MUTED),
        );
        ui.label(
            egui::RichText::new(value.to_string())
                .size(18.0)
                .color(ColorTokens::GLOW),
        );
    });
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
