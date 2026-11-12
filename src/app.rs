use crate::activity::{ActivityEvent, GameMode, LegendTower, RunActivity};
use crate::adb::{self, Device};
use crate::afk_journey::afk_stages::StageSelection;
use crate::afk_journey::arena::{
    ArenaSettings, OpponentPosition, Settings as PvpSettings, SupremeSettings,
};
use crate::afk_journey::dream_realm;
use crate::afk_journey::legend_trial::TowerSelection;
use crate::afk_journey::menu_rewards;
use crate::afk_journey::progression;
use crate::design_system::{
    self, card, primary_button, status_badge, ColorTokens, GeometryTokens, SizeTokens, SpaceTokens,
    TypeTokens,
};
use crate::localization::{format_text, text, Language};
use crate::routine::{Action as StageAction, DailyCategory, Routine};
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
    RecoveryStatus(String),
    StagesFinished(StageAction, Option<String>),
    DailyStep(usize),
    UpdateChecked(Result<Option<updater::Release>, String>),
    UpdateArtworkLoaded(updater::ReleaseArtwork, Option<egui::ColorImage>),
    UpdateCheckStarted,
    UpdateProgress(u8),
    UpdateReady(Result<(), String>),
}

enum ArtworkState {
    Loading,
    Ready(egui::TextureHandle),
    Unavailable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Automations,
    Settings,
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

#[derive(Clone, Copy)]
struct RunSettings {
    selection: StageSelection,
    towers: TowerSelection,
    dura_sweep: bool,
    dream_spend_gold: bool,
    dream_battle_limit: u32,
    battle: [ActionBattleOptions; 6],
    arena: ArenaSettings,
    supreme: SupremeSettings,
    diamond_hourglasses: crate::routine::DiamondHourglassSettings,
    emporium: crate::routine::EmporiumSettings,
}

impl RunSettings {
    fn mode(self, action: StageAction) -> Option<GameMode> {
        match action {
            StageAction::AfkStages => self.selection.modes().first().copied(),
            StageAction::SeasonLegendTrial => self
                .towers
                .selected()
                .first()
                .copied()
                .map(GameMode::SeasonLegendTrial),
            StageAction::UnionCampaign => Some(GameMode::UnionCampaign),
            StageAction::DurasTrials => Some(GameMode::DurasTrials),
            StageAction::Arena => Some(GameMode::Arena),
            StageAction::SupremeArena => Some(GameMode::SupremeArena),
            StageAction::DreamRealm => Some(GameMode::DreamRealm),
            StageAction::FriendRewards => Some(GameMode::FriendRewards),
            StageAction::MailRewards => Some(GameMode::MailRewards),
            StageAction::BattlePassRewards => Some(GameMode::BattlePassRewards),
            StageAction::DailyQuestRewards => Some(GameMode::DailyQuestRewards),
            StageAction::GuildQuestRewards => Some(GameMode::GuildQuestRewards),
            StageAction::CollectAffinity => Some(GameMode::CollectAffinity),
            StageAction::LevelUpHeroes => Some(GameMode::LevelUpHeroes),
            StageAction::EquipNewEquipment => Some(GameMode::EquipNewEquipment),
            StageAction::Emporium => Some(GameMode::Emporium),
            StageAction::SinglePull => Some(GameMode::SinglePull),
            StageAction::EssenceSwap => Some(GameMode::EssenceSwap),
            StageAction::AfkRewards => Some(GameMode::AfkRewards),
            StageAction::FreeHourglasses => Some(GameMode::FreeHourglasses),
            StageAction::DiamondHourglasses => Some(GameMode::DiamondHourglasses),
            StageAction::StageRewards => Some(GameMode::StageRewards),
            StageAction::SeasonRankingRewards => Some(GameMode::SeasonRankingRewards(
                crate::activity::RankingScope::District,
            )),
            StageAction::Daily => None,
        }
    }
}

fn run_step(
    serial: &str,
    action: StageAction,
    daily: bool,
    settings: RunSettings,
    progression: &mut progression::RunState,
    cancelled: &AtomicBool,
    sender: &Sender<Event>,
) -> Result<u32, String> {
    let mode = settings.mode(action).ok_or_else(|| {
        format_text!(
            "Select stages or towers for {} in its settings.",
            "Wähle Stufen oder Türme für {} in den Einstellungen.",
            action.label()
        )
    })?;
    let mut log = |message| {
        let _ = sender.send(Event::Log(message));
    };
    let mut collection = crate::afk_journey::daily::Cursor::default();
    let mut extras = crate::afk_journey::daily_extras::Cursor::default();
    let mut ranking = crate::afk_journey::phase_rewards::Cursor::default();
    let mut dream_cursor = dream_realm::Cursor::default();
    let mut menu_cursor = menu_rewards::Cursor::default();
    crate::afk_journey::recovery::run(
        serial,
        cancelled,
        |checkpoint, report| {
            let battle = || settings.battle[action.index()].battle_settings(mode);
            match action {
                StageAction::AfkStages => crate::afk_journey::afk_stages::run_selected(
                    serial,
                    battle(),
                    checkpoint.stages(settings.selection),
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::SeasonLegendTrial => crate::afk_journey::legend_trial::run(
                    serial,
                    battle(),
                    checkpoint.towers(settings.towers),
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::UnionCampaign => crate::afk_journey::union_campaign::run(
                    serial,
                    battle(),
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::DurasTrials => crate::afk_journey::duras_trials::run(
                    serial,
                    battle(),
                    settings.dura_sweep,
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::Arena => crate::afk_journey::arena::run(
                    serial,
                    PvpSettings::Arena(settings.arena),
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::SupremeArena => crate::afk_journey::arena::run(
                    serial,
                    PvpSettings::Supreme(settings.supreme),
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::DreamRealm => dream_realm::run(
                    serial,
                    dream_realm::Settings {
                        daily,
                        spend_gold: settings.dream_spend_gold,
                        max_battles: settings.dream_battle_limit,
                    },
                    &mut dream_cursor,
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::AfkRewards | StageAction::FreeHourglasses => {
                    let kind = if action == StageAction::AfkRewards {
                        crate::afk_journey::daily::Collection::AfkRewards
                    } else {
                        crate::afk_journey::daily::Collection::FreeHourglasses
                    };
                    crate::afk_journey::daily::run(
                        serial,
                        kind,
                        &mut collection,
                        cancelled,
                        &mut log,
                        report,
                    )
                }
                StageAction::DiamondHourglasses | StageAction::StageRewards => {
                    crate::afk_journey::daily_extras::run(
                        serial,
                        mode,
                        settings.diamond_hourglasses,
                        &mut extras,
                        cancelled,
                        &mut log,
                        report,
                    )
                }
                StageAction::SeasonRankingRewards => crate::afk_journey::phase_rewards::run(
                    serial,
                    &mut ranking,
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::FriendRewards
                | StageAction::MailRewards
                | StageAction::BattlePassRewards
                | StageAction::DailyQuestRewards
                | StageAction::GuildQuestRewards => {
                    menu_rewards::run(serial, mode, &mut menu_cursor, cancelled, &mut log, report)
                }
                StageAction::CollectAffinity
                | StageAction::LevelUpHeroes
                | StageAction::EquipNewEquipment
                | StageAction::Emporium
                | StageAction::SinglePull
                | StageAction::EssenceSwap => progression::run(
                    serial,
                    mode,
                    settings.emporium,
                    progression,
                    cancelled,
                    &mut log,
                    report,
                ),
                StageAction::Daily => Err("A Daily routine cannot contain itself.".into()),
            }
        },
        |message| {
            let _ = sender.send(Event::RecoveryStatus(message));
        },
        |event| {
            let _ = sender.send(Event::Activity(event));
        },
    )
}

pub struct PlayerApp {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    devices: Vec<Device>,
    selected_serial: Option<String>,
    logs: VecDeque<String>,
    busy: bool,
    page: Page,
    language_save_error: Option<String>,
    daily_routine: Routine,
    daily_category: DailyCategory,
    daily_settings_error: Option<String>,
    daily_step: Option<usize>,
    adb_path: String,
    stage_cancel: Option<Arc<AtomicBool>>,
    stage_selection: StageSelection,
    tower_selection: TowerSelection,
    dura_sweep_after_cap: bool,
    dream_spend_gold: bool,
    dream_battle_limit: u32,
    dream_battle_limit_error: Option<String>,
    active_action: Option<StageAction>,
    battle_options: [ActionBattleOptions; 6],
    arena_settings: ArenaSettings,
    supreme_settings: SupremeSettings,
    action_settings_dialog: Option<StageAction>,
    recovery_notice: Option<String>,
    logo: egui::TextureHandle,
    activity: RunActivity,
    action_error: Option<(StageAction, String)>,
    available_update: Option<updater::Release>,
    update_artwork: Option<(updater::ReleaseArtwork, ArtworkState)>,
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
        let (daily_routine, daily_settings_error) = match Routine::load() {
            Ok(routine) => (routine, None),
            Err(error) => (Routine::default(), Some(error)),
        };
        let (dream_battle_limit, dream_battle_limit_error) = match dream_realm::load_battle_limit()
        {
            Ok(limit) => (limit, None),
            Err(error) => (dream_realm::Settings::default().max_battles, Some(error)),
        };
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
            language_save_error: None,
            daily_routine,
            daily_category: DailyCategory::Rewards,
            daily_settings_error,
            daily_step: None,
            adb_path: std::env::var("ADB_PATH")
                .or_else(|_| std::env::var("ADBUTILS_ADB_PATH"))
                .unwrap_or_else(|_| adb::suggested_adb_path()),
            stage_cancel: None,
            stage_selection: StageSelection::default(),
            tower_selection: TowerSelection::default(),
            dura_sweep_after_cap: true,
            dream_spend_gold: false,
            dream_battle_limit,
            dream_battle_limit_error,
            active_action: None,
            battle_options: [ActionBattleOptions::default(); 6],
            arena_settings: ArenaSettings::default(),
            supreme_settings: SupremeSettings::default(),
            action_settings_dialog: None,
            recovery_notice: None,
            logo,
            activity: RunActivity::default(),
            action_error: None,
            available_update: None,
            update_artwork: None,
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
        app.log(text("AFK Journey · App started. Connect an Android device with ADB debugging enabled.", "AFK Journey · App gestartet. Verbinde ein Android-Gerät mit aktiviertem ADB-Debugging."));
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
            self.update_error = Some(
                text(
                    "The update service is unavailable.",
                    "Der Update-Dienst ist nicht verfügbar.",
                )
                .into(),
            );
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
                    if ui.small_button(text("Close", "Schließen")).clicked() {
                        self.update_notice = None;
                    }
                });
            });
            ui.add_space(SpaceTokens::MD);
        }
    }

    fn request_update_artwork(
        &mut self,
        artwork: &updater::ReleaseArtwork,
        context: &egui::Context,
    ) {
        if self
            .update_artwork
            .as_ref()
            .is_some_and(|(cached, _)| cached == artwork)
        {
            return;
        }
        self.update_artwork = Some((artwork.clone(), ArtworkState::Loading));
        let artwork = artwork.clone();
        let sender = self.sender.clone();
        let context = context.clone();
        thread::spawn(move || {
            let image = artwork.load().ok().map(|image| {
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                )
            });
            let _ = sender.send(Event::UpdateArtworkLoaded(artwork, image));
            context.request_repaint();
        });
    }

    fn show_update_dialog(&mut self, context: &egui::Context) {
        if !self.update_dialog_open {
            return;
        }
        let Some(release) = self.available_update.clone() else {
            self.update_dialog_open = false;
            return;
        };
        if let Some(artwork) = &release.artwork {
            self.request_update_artwork(artwork, context);
        }
        let mut close = false;
        let response = egui::Modal::new(egui::Id::unique("app-update-dialog"))
            .frame(design_system::panel_frame())
            .show(context, |ui| {
                ui.set_width((context.content_rect().width() - 64.0).clamp(260.0, 560.0));
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format_text!(
                            "Update {} available",
                            "Update {} verfügbar",
                            release.version
                        ))
                        .color(ColorTokens::GLOW),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(text("Close", "Schließen")).clicked() {
                            close = true;
                        }
                    });
                });
                ui.label(format_text!(
                    "Installed: {} · New: {}",
                    "Installiert: {} · Neu: {}",
                    updater::VERSION,
                    release.version
                ));
                ui.strong(text("Patch notes", "Patchnotes"));
                egui::ScrollArea::vertical()
                    .id_salt("update-patchnotes")
                    .max_height((context.content_rect().height() - 260.0).clamp(80.0, 360.0))
                    .show(ui, |ui| {
                        for line in release.notes.lines() {
                            if let Some((alt, url)) = updater::patch_note_image(line) {
                                let artwork = self.update_artwork.as_ref().filter(|(cached, _)| {
                                    cached.markdown_url == url
                                        && release.artwork.as_ref() == Some(cached)
                                });
                                match artwork {
                                    Some((_, ArtworkState::Ready(texture))) => {
                                        let size = texture.size_vec2();
                                        let scale = (ui.available_width() / size.x)
                                            .min(280.0 / size.y)
                                            .min(1.0);
                                        ui.add(
                                            egui::Image::new(texture)
                                                .fit_to_exact_size(size * scale)
                                                .alt_text(alt),
                                        );
                                    }
                                    Some((_, ArtworkState::Loading)) => {
                                        ui.small(text(
                                            "Loading release graphic …",
                                            "Release-Grafik wird geladen …",
                                        ));
                                    }
                                    _ => {
                                        ui.hyperlink_to(
                                            text(
                                                "View release graphic on GitHub",
                                                "Release-Grafik auf GitHub ansehen",
                                            ),
                                            &release.url,
                                        );
                                    }
                                }
                            } else if line.starts_with('#') {
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
                        .add_enabled(
                            !self.busy,
                            primary_button(text(
                                "Download and install",
                                "Herunterladen und installieren",
                            )),
                        )
                        .clicked()
                    {
                        self.install_update();
                    }
                    ui.hyperlink_to(
                        text("Release on GitHub", "Release auf GitHub"),
                        &release.url,
                    );
                });
                ui.small(text(
                    "ADBHelper will restart automatically after installation.",
                    "Nach der Installation startet ADBHelper automatisch neu.",
                ));
                if let Some(percent) = self.update_progress {
                    ui.add(egui::ProgressBar::new(f32::from(percent) / 100.0).text(
                        if percent == 100 {
                            text(
                                "Verifying and preparing the update …",
                                "Update wird geprüft und vorbereitet …",
                            )
                            .into()
                        } else {
                            format_text!("Download: {percent}%", "Download: {percent} %")
                        },
                    ));
                } else if self.busy {
                    ui.small(text(
                        "The update can be installed after the current action finishes.",
                        "Das Update kann nach dem laufenden Vorgang installiert werden.",
                    ));
                }
                if let Some(error) = &self.update_error {
                    ui.add(egui::Label::new(error).wrap());
                }
            });
        if close || response.should_close() {
            self.update_dialog_open = false;
            if matches!(&self.update_artwork, Some((_, ArtworkState::Unavailable))) {
                self.update_artwork = None;
            }
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
            self.log(text(
                "Select a connected device first.",
                "Bitte zuerst ein verbundenes Gerät auswählen.",
            ));
            return;
        };
        self.busy = true;
        self.log(format!("{label} …"));
        let sender = self.sender.clone();
        thread::spawn(move || {
            let message = match action(serial) {
                Ok(details) => format!("{label}: {details}"),
                Err(error) => {
                    format_text!("{label} failed: {error}", "{label} fehlgeschlagen: {error}")
                }
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
        self.recovery_notice = None;
        self.daily_step = None;
        let steps = if action == StageAction::Daily {
            self.daily_routine.steps.clone()
        } else {
            vec![action]
        };
        if steps.is_empty() {
            self.action_error = Some((
                action,
                text(
                    "Add at least one step in the Daily settings.",
                    "Füge in den Daily-Einstellungen mindestens einen Schritt hinzu.",
                )
                .into(),
            ));
            return;
        }
        let settings = RunSettings {
            selection: self.stage_selection,
            towers: self.tower_selection,
            dura_sweep: self.dura_sweep_after_cap,
            dream_spend_gold: self.dream_spend_gold,
            dream_battle_limit: self.dream_battle_limit,
            battle: self.battle_options,
            arena: self.arena_settings,
            supreme: self.supreme_settings,
            diamond_hourglasses: self.daily_routine.diamond_hourglasses,
            emporium: self.daily_routine.emporium,
        };
        for step in &steps {
            if settings.mode(*step).is_none() {
                self.action_error = Some((
                    action,
                    format_text!(
                        "Select stages or towers for {} in its settings.",
                        "Wähle Stufen oder Türme für {} in den Einstellungen.",
                        step.label()
                    ),
                ));
                return;
            }
        }
        let Some(serial) = self.selected_serial() else {
            self.action_error = Some((
                action,
                text(
                    "Connect a device in Settings first.",
                    "Bitte zuerst unter Einstellungen ein Gerät verbinden.",
                )
                .into(),
            ));
            return;
        };
        self.activity.start(
            settings.mode(steps[0]).expect("steps validated"),
            Instant::now(),
        );
        let cancelled = Arc::new(AtomicBool::new(false));
        self.stage_cancel = Some(cancelled.clone());
        self.active_action = Some(action);
        self.busy = true;
        self.log(format_text!("{} started.", "{} gestartet.", action.label()));
        let sender = self.sender.clone();
        thread::spawn(move || {
            let mut progression = progression::RunState::default();
            let result = Routine {
                steps,
                diamond_hourglasses: settings.diamond_hourglasses,
                emporium: settings.emporium,
            }
            .run(&cancelled, |index, step| {
                if action == StageAction::Daily {
                    let _ = sender.send(Event::DailyStep(index));
                }
                run_step(
                    &serial,
                    step,
                    action == StageAction::Daily,
                    settings,
                    &mut progression,
                    &cancelled,
                    &sender,
                )
                .map_err(|error| format!("{} · {error}", step.label()))
            });
            let message = match &result {
                Ok(_) => format_text!("{} finished.", "{} abgeschlossen.", action.label()),
                Err(error) => {
                    format_text!("{} stopped: {error}", "{} beendet: {error}", action.label())
                }
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

    fn receive_events(&mut self, context: &egui::Context) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::DailyStep(index) => self.daily_step = Some(index),
                Event::Devices(Ok(devices)) => {
                    if self
                        .selected_serial
                        .as_ref()
                        .is_none_or(|serial| !devices.iter().any(|device| &device.serial == serial))
                    {
                        self.selected_serial = devices.first().map(|device| device.serial.clone());
                    }
                    if devices.is_empty() {
                        self.log(text(
                            "No authorized ADB devices found.",
                            "Keine autorisierten ADB-Geräte gefunden.",
                        ));
                    } else {
                        self.log(format_text!(
                            "Found {} authorized ADB device(s).",
                            "{} autorisierte(s) ADB-Gerät(e) gefunden.",
                            devices.len()
                        ));
                    }
                    self.devices = devices;
                }
                Event::Devices(Err(error)) => self.log(format_text!(
                    "ADB device discovery failed: {error}",
                    "ADB-Suche fehlgeschlagen: {error}"
                )),
                Event::Log(message) => self.log(message),
                Event::RecoveryStatus(message) => {
                    self.log(&message);
                    self.recovery_notice = Some(message);
                }
                Event::Activity(event) => {
                    if matches!(event, ActivityEvent::ModeEntered(_)) {
                        self.recovery_notice = None;
                    }
                    self.activity.apply(event);
                }
                Event::StagesFinished(action, error) => {
                    self.recovery_notice = None;
                    self.daily_step = None;
                    self.action_error = error.map(|error| (action, error))
                }
                Event::UpdateChecked(result) => {
                    self.update_checking = false;
                    self.update_checked = true;
                    if self.update_progress.is_none() {
                        match result {
                            Ok(release) => {
                                if self.update_artwork.as_ref().is_some_and(|(cached, _)| {
                                    release
                                        .as_ref()
                                        .and_then(|release| release.artwork.as_ref())
                                        != Some(cached)
                                }) {
                                    self.update_artwork = None;
                                }
                                self.available_update = release;
                                self.update_error = None;
                            }
                            Err(error) => self.update_error = Some(error),
                        }
                    }
                }
                Event::UpdateArtworkLoaded(artwork, image) => {
                    if let Some((cached, state)) = &mut self.update_artwork {
                        if *cached == artwork {
                            *state = match image {
                                Some(image) => ArtworkState::Ready(context.load_texture(
                                    "update-release-artwork",
                                    image,
                                    egui::TextureOptions::LINEAR,
                                )),
                                None => ArtworkState::Unavailable,
                            };
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
                // Use the dashboard viewport height measured before the outer scroll area.
                // The horizontal row can report too little height before its children are laid out.
                egui::vec2(left_width, dashboard_height.max(0.0)),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.x = SpaceTokens::SM;
                    ui.heading(egui::RichText::new(text("Actions", "Aktionen")).color(ColorTokens::TEXT_PRIMARY));
                    ui.add_space(SpaceTokens::SM);
                    egui::ScrollArea::vertical()
                        .id_salt("dashboard-actions")
                        .max_height(ui.available_height())
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            self.show_daily_card(ui);
                            ui.add_space(SpaceTokens::MD);
                            let active = self.active_action == Some(StageAction::AfkStages)
                                && self.stage_cancel.is_some();
                            design_system::state_card(ui, active, |ui| {
                                self.show_action_header(ui, StageAction::AfkStages, active);
                                ui.label(text("Push selected stages", "Ausgewählte Stages pushen"));
                                ui.add_space(SpaceTokens::SM);
                                ui.add_enabled_ui(!self.busy, |ui| {
                                    ui.spacing_mut().interact_size.y = 20.0;
                                    ui.horizontal(|ui| {
                                        design_system::active_checkbox(
                                            ui,
                                            &mut self.stage_selection.season,
                                            text("Season Stages", "Season-Stages"),
                                        );
                                        design_system::active_checkbox(
                                            ui,
                                            &mut self.stage_selection.normal,
                                            text("Normal Stages", "Normale Stages"),
                                        );
                                    });
                                });
                                if self.stage_selection.season && self.stage_selection.normal {
                                    ui.small(text("Season Stages first", "Zuerst Season, dann Normal"));
                                }
                                ui.add_space(SpaceTokens::SM);
                                ui.separator();
                                ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !self.busy
                                        && (self.stage_selection.season
                                            || self.stage_selection.normal),
                                    primary_button(text("Start", "Starten")),
                                )
                                .clicked()
                            {
                                self.start_action(StageAction::AfkStages);
                            }
                            if let Some(cancel) = self.stage_cancel.as_ref().filter(|_| active) {
                                let cancel = cancel.clone();
                                if ui.add(design_system::stop_button("Stop")).clicked() {
                                    cancel.store(true, Ordering::Relaxed);
                                    self.log(text("Stop requested for the AFK Stages run.", "Stop für den gesamten AFK-Stages-Lauf angefordert."));
                                }
                            }
                        });
                                if let Some((StageAction::AfkStages, error)) =
                                    self.action_error.as_ref()
                                {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(error)
                                                .color(ColorTokens::DANGER_HOVER),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                            ui.add_space(SpaceTokens::MD);
                            let legend_active = self.active_action
                                == Some(StageAction::SeasonLegendTrial)
                                && self.stage_cancel.is_some();
                            design_system::state_card(ui, legend_active, |ui| {
                                self.show_action_header(
                                    ui,
                                    StageAction::SeasonLegendTrial,
                                    legend_active,
                                );
                                ui.label(text("Push available faction towers", "Verfügbare Fraktionstürme pushen"));
                                ui.small(format_text!("{} towers selected", "{} Türme ausgewählt",
                                    self.tower_selection.selected().len()
                                ));
                                ui.add_space(SpaceTokens::SM);
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            !self.busy
                                                && !self.tower_selection.selected().is_empty(),
                                            primary_button(text("Start", "Starten")),
                                        )
                                        .clicked()
                                    {
                                        self.start_action(StageAction::SeasonLegendTrial);
                                    }
                                    if legend_active {
                                        if let Some(cancel) = self.stage_cancel.clone() {
                                            if ui.add(design_system::stop_button("Stop")).clicked()
                                            {
                                                cancel.store(true, Ordering::Relaxed);
                                                self.log(
                                            text("Stop requested for the Legend Trial run.", "Stop für den gesamten Legend-Trial-Lauf angefordert."),
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
                                            egui::RichText::new(error)
                                                .color(ColorTokens::DANGER_HOVER),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                            ui.add_space(SpaceTokens::MD);
                            let union_active = self.active_action
                                == Some(StageAction::UnionCampaign)
                                && self.stage_cancel.is_some();
                            design_system::state_card(ui, union_active, |ui| {
                                self.show_action_header(
                                    ui,
                                    StageAction::UnionCampaign,
                                    union_active,
                                );
                                ui.label(text("Push guild floors", "Gilden-Etagen pushen"));
                                ui.add_space(SpaceTokens::SM);
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(!self.busy, primary_button(text("Start", "Starten")))
                                        .clicked()
                                    {
                                        self.start_action(StageAction::UnionCampaign);
                                    }
                                    if union_active {
                                        if let Some(cancel) = self.stage_cancel.clone() {
                                            if ui.add(design_system::stop_button("Stop")).clicked()
                                            {
                                                cancel.store(true, Ordering::Relaxed);
                                                self.log(
                                                    text("Stop requested for Union Campaign.", "Stop für die Union Campaign angefordert."),
                                                );
                                            }
                                        }
                                    }
                                });
                                if let Some((StageAction::UnionCampaign, error)) =
                                    self.action_error.as_ref()
                                {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(error)
                                                .color(ColorTokens::DANGER_HOVER),
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
                                ui.label(text("Push stages · Sweep at the verified cap", "Stufen pushen · Sweep nur am bestätigten Maximum"));
                                ui.add_space(SpaceTokens::SM);
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(!self.busy, primary_button(text("Start", "Starten")))
                                        .clicked()
                                    {
                                        self.start_action(StageAction::DurasTrials);
                                    }
                                    if dura_active {
                                        if let Some(cancel) = self.stage_cancel.clone() {
                                            if ui.add(design_system::stop_button("Stop")).clicked()
                                            {
                                                cancel.store(true, Ordering::Relaxed);
                                                self.log(text("Stop requested for Dura’s Trials.", "Stop für Dura’s Trials angefordert."));
                                            }
                                        }
                                    }
                                });
                                if let Some((StageAction::DurasTrials, error)) =
                                    self.action_error.as_ref()
                                {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(error)
                                                .color(ColorTokens::DANGER_HOVER),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                            ui.add_space(SpaceTokens::MD);
                            let dream_description = if self.dream_battle_limit == 1 {
                                text("Up to one battle per run", "Maximal ein Kampf pro Lauf").to_owned()
                            } else {
                                format_text!("Up to {} battles per run", "Bis zu {} Kämpfe pro Lauf", self.dream_battle_limit)
                            };
                            self.show_battle_card(ui, StageAction::DreamRealm, &dream_description);
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::Arena, text("Play daily Arena battles", "Tägliche Arena-Kämpfe spielen"));
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::SupremeArena, text("Play daily Supreme Arena battles", "Tägliche Supreme-Arena-Kämpfe spielen"));
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
                    ui.heading(egui::RichText::new(text("Activity", "Aktivität")).color(ColorTokens::TEXT_PRIMARY));
                    ui.add_space(SpaceTokens::SM);
                    card(ui, |ui| {
                        ui.small(if self.activity.is_running() {
                            text("CURRENT GAME MODE", "AKTUELLER SPIELMODUS")
                        } else {
                            text("LAST GAME MODE", "LETZTER SPIELMODUS")
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
                                ui.label(text("Opening game mode …", "Spielmodus wird geöffnet …"));
                                if let Some(target) = self.activity.target {
                                    ui.small(format_text!("Target: {}", "Ziel: {}", target.label()));
                                }
                            }
                            None => {
                                ui.label(text("No mode started yet", "Noch kein Modus gestartet"));
                            }
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("activity-mode-counters")
                        .max_height(ui.available_height())
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
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
                                if mode.is_collection() {
                                    let label = match mode {
                                        GameMode::CollectAffinity => text("HEROES WITH GAINS", "HELDEN MIT ZUWACHS"),
                                        GameMode::LevelUpHeroes => text("LEVELS GAINED", "STUFEN GEWONNEN"),
                                        GameMode::EquipNewEquipment => text("CLASSES EQUIPPED", "KLASSEN AUSGERÜSTET"),
                                        GameMode::Emporium => text("PURCHASES", "KÄUFE"),
                                        GameMode::SinglePull => text("RECRUITS", "REKRUTIERUNGEN"),
                                        GameMode::EssenceSwap => text("SWAPS", "TAUSCHE"),
                                        _ => text("COLLECTED", "ABGEHOLT"),
                                    };
                                    progress_metric(ui, label, stats.collected);
                                    return;
                                }
                                progress_metric(ui, text("ATTEMPTS", "VERSUCHE"), stats.attempts);
                                ui.add_space(SpaceTokens::MD);
                                if *mode == GameMode::DreamRealm {
                                    progress_metric(ui, text("COMPLETED", "ABGESCHLOSSEN"), stats.scored);
                                    ui.add_space(SpaceTokens::SM);
                                    progress_metric(ui, text("COLLECTED", "ABGEHOLT"), stats.collected);
                                    return;
                                }
                                progress_metric(
                                    ui,
                                    if mode.is_pvp() { text("WINS", "SIEGE") } else { text("CLEARED", "GECLEART") },
                                    if mode.is_pvp() {
                                        stats.wins
                                    } else {
                                        stats.cleared
                                    },
                                );
                                if *mode == GameMode::DurasTrials {
                                    ui.add_space(SpaceTokens::SM);
                                    progress_metric(ui, "SWEEPS", stats.sweeps);
                                }
                            });
                        });
                    }
                        });
                },
            );
        });
    }

    fn show_battle_card(&mut self, ui: &mut egui::Ui, action: StageAction, description: &str) {
        let active = self.active_action == Some(action) && self.stage_cancel.is_some();
        design_system::state_card(ui, active, |ui| {
            self.show_action_header(ui, action, active);
            ui.label(description);
            ui.add_space(SpaceTokens::SM);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy, primary_button(text("Start", "Starten")))
                    .clicked()
                {
                    self.start_action(action);
                }
                if active {
                    if let Some(cancel) = self.stage_cancel.clone() {
                        if ui.add(design_system::stop_button("Stop")).clicked() {
                            cancel.store(true, Ordering::Relaxed);
                            self.log(format_text!(
                                "Stop requested for {}.",
                                "Stop für {} angefordert.",
                                action.label()
                            ));
                        }
                    }
                }
            });
            if let Some((failed, error)) = self
                .action_error
                .as_ref()
                .filter(|(failed, _)| *failed == action)
            {
                let _ = failed;
                ui.add(
                    egui::Label::new(egui::RichText::new(error).color(ColorTokens::DANGER_HOVER))
                        .wrap(),
                );
            }
        });
    }

    fn show_daily_card(&mut self, ui: &mut egui::Ui) {
        let active = self.active_action == Some(StageAction::Daily) && self.stage_cancel.is_some();
        design_system::state_card(ui, active, |ui| {
            self.show_action_header(ui, StageAction::Daily, active);
            if let Some(index) = self.daily_step.filter(|_| active) {
                if let Some(step) = self.daily_routine.steps.get(index) {
                    ui.label(format_text!(
                        "Step {}/{} · {}",
                        "Schritt {}/{} · {}",
                        index + 1,
                        self.daily_routine.steps.len(),
                        step.label()
                    ));
                }
            } else {
                ui.label(format_text!(
                    "{} steps · Your daily routine",
                    "{} Schritte · Deine tägliche Routine",
                    self.daily_routine.steps.len()
                ));
            }
            ui.add_space(SpaceTokens::SM);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.busy && !self.daily_routine.steps.is_empty(),
                        primary_button(text("Start", "Starten")),
                    )
                    .clicked()
                {
                    self.start_action(StageAction::Daily);
                }
                if active && ui.add(design_system::stop_button("Stop")).clicked() {
                    if let Some(cancelled) = &self.stage_cancel {
                        cancelled.store(true, Ordering::Relaxed);
                    }
                }
            });
            if let Some((StageAction::Daily, error)) = &self.action_error {
                ui.colored_label(ColorTokens::CLOSE_HOVER, error);
            }
        });
    }

    fn show_daily_settings(&mut self, context: &egui::Context) {
        let previous = self.daily_routine.clone();
        let mut diamond_settings = self.daily_routine.diamond_hourglasses;
        let mut emporium_settings = self.daily_routine.emporium;
        let mut close = false;
        let mut placement = None;
        let mut remove = None;
        let reveal_id = egui::Id::unique("daily-reveal-last-step");
        let reveal_last =
            context.data_mut(|data| data.remove_temp::<bool>(reveal_id)) == Some(true);
        let response = egui::Modal::new(egui::Id::unique("daily-routine-editor"))
            .frame(design_system::panel_frame())
            .show(context, |ui| {
                ui.set_width((context.content_rect().width() - 64.0).clamp(520.0, 760.0));
                ui.heading(text("Daily routine", "Daily-Routine"));
                ui.label(text(
                    "Drag actions to add or reorder them.",
                    "Ziehe Aktionen zum Hinzufügen oder Sortieren.",
                ));
                ui.small(text(
                    "Each step uses the settings from its action card.",
                    "Jeder Schritt verwendet die Einstellungen seiner Aktionskachel.",
                ));
                ui.add_space(SpaceTokens::MD);
                if self.busy {
                    ui.small(text("Stop the current run to edit the routine.", "Beende den aktuellen Lauf, um die Routine zu bearbeiten."));
                }
                let list_height = (context.content_rect().height() - 320.0).clamp(110.0, 462.0);
                let count = self.daily_routine.steps.len();
                ui.columns(2, |columns| {
                    let available = &mut columns[0];
                    available.strong(text("Available actions", "Verfügbare Aktionen"));
                    available.add_space(SpaceTokens::SM);
                    let tabs = available.horizontal_wrapped(|ui| {
                        for category in DailyCategory::ALL {
                            let selected = self.daily_category == category;
                            let tab = ui.add(egui::Button::selectable(selected, category.label())
                                .min_size(egui::vec2(0.0, SizeTokens::FIELD)));
                            if selected {
                                let underline = tab.rect.shrink2(egui::vec2(SpaceTokens::XS, 1.0));
                                ui.painter().line_segment(
                                    [underline.left_bottom(), underline.right_bottom()],
                                    egui::Stroke::new(1.0, ColorTokens::GLOW),
                                );
                            }
                            if tab.clicked() {
                                self.daily_category = category;
                                ui.ctx().request_repaint();
                            }
                        }
                    });
                    available.add_space(SpaceTokens::SM);
                    let category = self.daily_category;
                    available.add_enabled_ui(!self.busy, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("daily-available-actions", category))
                            .max_height((list_height - tabs.response.rect.height() - SpaceTokens::SM).max(SizeTokens::ROW))
                            .show(ui, |ui| {
                                let mut actions = category.actions().iter().copied()
                                    .filter(|action| !self.daily_routine.steps.contains(action))
                                    .peekable();
                                if actions.peek().is_none() {
                                    ui.label(text(
                                        "All actions in this category are in your routine.",
                                        "Alle Aktionen dieser Kategorie sind bereits in deiner Routine.",
                                    ));
                                }
                                for action in actions {
                                    design_system::panel_frame().show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.dnd_drag_source(egui::Id::unique(("daily-source", action.index())), action, |ui| {
                                                daily_drag_label(ui, action.label());
                                            });
                                            if ui.small_button("+").on_hover_text(text("Add to end", "Ans Ende hinzufügen")).clicked() {
                                                placement = Some((action, count));
                                            }
                                        });
                                    });
                                    ui.add_space(SpaceTokens::SM);
                                }
                            });
                    });
                    columns[1].add_enabled_ui(!self.busy, |routine| {
                        routine.strong(text("Run in this order", "In dieser Reihenfolge ausführen"));
                        routine.add_space(SpaceTokens::SM);
                        egui::ScrollArea::vertical()
                            .id_salt("daily-routine-steps")
                            .max_height(list_height)
                            .auto_shrink([false, false])
                            .animated(false)
                            .show_viewport(routine, |ui, viewport| {
                                if count == 0 {
                                    ui.label(text("Your routine is empty.", "Deine Routine ist leer."));
                                }
                                for (index, action) in self.daily_routine.steps.iter().copied().enumerate() {
                                    let row = design_system::panel_frame().show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.dnd_drag_source(egui::Id::unique(("daily-step", action.index())), action, |ui| {
                                            daily_drag_label(ui, &format!("{}  {}", index + 1, action.label()));
                                        });
                                        if action == StageAction::DiamondHourglasses {
                                            ui.horizontal(|ui| {
                                                ui.label(text("Purchases", "Käufe"));
                                                ui.add(egui::DragValue::new(&mut diamond_settings.purchases).range(1..=10));
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label(text("Diamond budget", "Diamantenbudget"));
                                                ui.add(egui::DragValue::new(&mut diamond_settings.budget).range(0..=5000).speed(10));
                                            });
                                            ui.small(text("Total limit per run. Free hourglasses should run first.", "Gesamtlimit pro Lauf. Kostenlose Sanduhren sollten zuerst laufen."));
                                        }
                                        if action == StageAction::Emporium {
                                            for (label, budget) in [
                                                (text("Diamonds · Invite Letter", "Diamanten · Invite Letter"), &mut emporium_settings.diamonds),
                                                (text("Pal-Coins · Affinity", "Pal-Coins · Affinität"), &mut emporium_settings.pal_coins),
                                                (text("Dream Fragments · Essences", "Traumfragmente · Essenzen"), &mut emporium_settings.dream_fragments),
                                            ] {
                                                ui.horizontal(|ui| {
                                                    ui.label(label);
                                                    ui.add(egui::DragValue::new(budget).range(0..=100_000));
                                                });
                                            }
                                            ui.checkbox(&mut emporium_settings.full_price_affinity, text("Include full-price affinity items", "Auch Affinitätsartikel zum Vollpreis"));
                                            ui.small(text("Budgets apply per run. A budget of 0 disables the purchase.", "Budgets gelten pro Lauf. 0 deaktiviert den Kauf."));
                                        }
                                        if action == StageAction::EssenceSwap {
                                            ui.small(text("Place after Emporium to exchange essences bought in this run.", "Nach Emporium einfügen, um in diesem Lauf gekaufte Essenzen zu tauschen."));
                                        }
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.add_enabled(index > 0, egui::Button::new(text("Up", "Hoch")).small()).clicked() {
                                                placement = Some((action, index - 1));
                                            }
                                            if ui.add_enabled(index + 1 < count, egui::Button::new(text("Down", "Runter")).small()).clicked() {
                                                placement = Some((action, index + 2));
                                            }
                                            if ui.add_enabled(index + 1 < count, egui::Button::new(text("To end", "Ans Ende")).small()).clicked() {
                                                placement = Some((action, count));
                                            }
                                            if ui.small_button(text("Remove", "Entfernen")).clicked() { remove = Some(index); }
                                        });
                                    }).response;
                                    if reveal_last && index + 1 == count {
                                        row.scroll_to_me(Some(egui::Align::BOTTOM));
                                    }
                                    if ui.is_enabled() && row.dnd_hover_payload::<StageAction>().is_some() {
                                        let before = context.pointer_latest_pos().is_none_or(|p| p.y < row.rect.center().y);
                                        let y = if before { row.rect.top() } else { row.rect.bottom() };
                                        ui.painter().line_segment(
                                            [egui::pos2(row.rect.left(), y), egui::pos2(row.rect.right(), y)],
                                            egui::Stroke::new(2.0, ColorTokens::GLOW),
                                        );
                                        if let Some(action) = row.dnd_release_payload::<StageAction>() {
                                            placement = Some((*action, index + usize::from(!before)));
                                        }
                                    }
                                    ui.add_space(SpaceTokens::SM);
                                }
                                scroll_daily_drag(ui, viewport);
                            });
                        routine.add_space(SpaceTokens::SM);
                        let mut drop_area = design_system::panel_frame()
                            .fill(ColorTokens::BG_SECTION)
                            .inner_margin(egui::Margin::symmetric(12, 6))
                            .begin(routine);
                        let ui = &mut drop_area.content_ui;
                        ui.add_sized(
                            [ui.available_width(), 30.0],
                            egui::Label::new(text("Drop here to place last", "Hier ans Ende ziehen"))
                                .halign(egui::Align::Center),
                        );
                        let target = drop_area.allocate_space(routine);
                        if routine.is_enabled() && target.dnd_hover_payload::<StageAction>().is_some() {
                            drop_area.frame.fill = ColorTokens::BG_ACTIVE;
                            drop_area.frame.stroke = egui::Stroke::new(1.0, ColorTokens::GLOW);
                            if let Some(action) = target.dnd_release_payload::<StageAction>() {
                                placement = Some((*action, count));
                            }
                        }
                        drop_area.paint(routine);
                    });
                });
                ui.add_space(SpaceTokens::MD);
                ui.separator();
                if let Some(error) = &self.daily_settings_error {
                    ui.colored_label(ColorTokens::CLOSE_HOVER, format_text!(
                        "Daily settings could not be saved or loaded: {error}",
                        "Daily-Einstellungen konnten nicht gespeichert oder geladen werden: {error}"
                    ));
                }
                ui.small(text("The routine is saved automatically.", "Die Routine wird automatisch gespeichert."));
                close = ui.button(text("Close", "Schließen")).clicked();
            });
        if let Some(index) = remove {
            self.daily_routine.steps.remove(index);
        }
        self.daily_routine.diamond_hourglasses = diamond_settings;
        self.daily_routine.emporium = emporium_settings;
        if let Some((action, index)) = placement {
            self.daily_routine.place(action, index);
            if self.daily_routine.steps.last() == Some(&action) {
                context.data_mut(|data| data.insert_temp(reveal_id, true));
                context.request_repaint();
            }
        }
        if self.daily_routine != previous {
            self.daily_settings_error = self.daily_routine.save().err();
        }
        if close || response.should_close() {
            self.action_settings_dialog = None;
        }
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
                    status_badge(
                        ui,
                        if self.recovery_notice.is_some() {
                            text("RECOVERING", "NEUSTART")
                        } else {
                            text("RUNNING", "LÄUFT")
                        },
                        true,
                    );
                }
            });
        });
    }

    fn show_action_settings_dialog(&mut self, context: &egui::Context) {
        let Some(action) = self.action_settings_dialog else {
            return;
        };
        if action == StageAction::Daily {
            self.show_daily_settings(context);
            return;
        }
        let mut close = false;
        let response = egui::Modal::new(egui::Id::unique(("action-settings-dialog", action.index())))
            .frame(design_system::panel_frame())
            .show(context, |ui| {
                ui.set_width((context.content_rect().width() - 64.0).clamp(260.0, 500.0));
                ui.heading(action.label());
                ui.label(text("Action settings", "Aktionseinstellungen"));
                ui.add_space(SpaceTokens::SM);
                ui.separator();
                ui.add_space(SpaceTokens::SM);
                if self.busy {
                    ui.small(text("Action settings are read-only during a run.", "Während eines laufenden Vorgangs können die Einstellungen nur angesehen werden."));
                    ui.add_space(SpaceTokens::SM);
                }
                egui::ScrollArea::vertical()
                    .id_salt(("action-settings-content", action.index()))
                    .max_height((context.content_rect().height() - 230.0).clamp(100.0, 540.0))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!self.busy, |ui| {
                            match action {
                                StageAction::AfkStages => {
                                    ui.strong(text("Stage selection", "Stage-Auswahl"));
                                    ui.horizontal(|ui| {
                                        design_system::active_checkbox(ui, &mut self.stage_selection.season, text("Season Stages", "Season-Stages"));
                                        design_system::active_checkbox(ui, &mut self.stage_selection.normal, text("Normal Stages", "Normale Stages"));
                                    });
                                    ui.small(text("When both are selected, Season Stages run first.", "Bei gemeinsamer Auswahl zuerst Season, danach Normal."));
                                }
                                StageAction::SeasonLegendTrial => {
                                    ui.strong(text("Faction towers", "Fraktionstürme"));
                                    ui.label(text("Towers that are closed today are skipped automatically.", "Heute geschlossene Türme werden automatisch übersprungen."));
                                    egui::Grid::new("legend-trial-towers").num_columns(2).show(ui, |ui| {
                                        for (index, tower) in LegendTower::ALL.into_iter().enumerate() {
                                            design_system::active_checkbox(ui, &mut self.tower_selection.enabled[index], tower.label());
                                            if index % 2 == 1 { ui.end_row(); }
                                        }
                                    });
                                    ui.small(text("Order: Lightbearer, Wilder, Graveborn, Mauler.", "Reihenfolge: Lightbearer, Wilder, Graveborn, Mauler."));
                                }
                                StageAction::DurasTrials => {
                                    ui.strong(text("Daily sweeps", "Tägliche Sweeps"));
                                    design_system::active_checkbox(ui, &mut self.dura_sweep_after_cap,
                                        text("Run daily sweeps once the stage cap is verified", "Nach bestätigter Maximalstufe tägliche Sweeps ausführen"));
                                    ui.small(text("Sweeps require a verified current stage cap and use daily attempts only.", "Nur an der eindeutig bestätigten aktuellen Maximalstufe. Bei unklarer Erkennung wird nicht gesweept. Zusätzliche Vorratstoken werden nicht verwendet."));
                                }
                                StageAction::UnionCampaign => {}
                                StageAction::DreamRealm => {
                                    ui.label(text("Uses your in-game formation and available free chances.", "Verwendet deine Aufstellung im Spiel und die verfügbaren kostenlosen Versuche."));
                                    ui.horizontal(|ui| {
                                        ui.label(text("Maximum battles per run", "Maximale Kämpfe pro Lauf"));
                                        if ui.add(egui::DragValue::new(&mut self.dream_battle_limit).range(1..=dream_realm::MAX_BATTLES)).changed() {
                                            self.dream_battle_limit_error = dream_realm::save_battle_limit(self.dream_battle_limit)
                                                .err().map(|error| error.to_string());
                                        }
                                    });
                                    if let Some(error) = &self.dream_battle_limit_error {
                                        ui.colored_label(ColorTokens::CLOSE_HOVER, format_text!("Battle limit could not be loaded or saved: {error}", "Kampflimit konnte nicht geladen oder gespeichert werden: {error}"));
                                    }
                                    design_system::active_checkbox(ui, &mut self.dream_spend_gold,
                                        text("Spend Gold on extra attempts", "Gold für zusätzliche Versuche ausgeben"));
                                    ui.small(text("Extra attempts require a verified Gold offer.", "Zusatzversuche werden nur bei einem bestätigten Goldangebot gekauft."));
                                    ui.small(text("In Daily, collect yesterday’s ranking reward and stop once today’s ranking is recorded.", "In der Daily-Routine wird die Ranglistenbelohnung vom Vortag abgeholt und bei bestätigter heutiger Platzierung gestoppt."));
                                }
                                StageAction::Arena => {
                                    ui.strong(text("Daily attempts", "Tägliche Versuche"));
                                    ui.horizontal(|ui| {
                                        ui.label(text("Available attempts", "Vorhandene Versuche"));
                                        ui.add(egui::DragValue::new(&mut self.arena_settings.attempts).range(1..=5));
                                    });
                                    design_system::active_checkbox(ui,&mut self.arena_settings.claim_free_attempts,text("Claim up to two extra free attempts", "Bis zu zwei zusätzliche kostenlose Versuche abholen"));
                                    ui.small(text("Extra attempts require a verified price of zero diamonds.", "Zusätzliche Versuche nur bei eindeutig bestätigten Kosten von null Diamanten. Keine kostenpflichtigen Käufe."));
                                    ui.add_space(SpaceTokens::MD);
                                    ui.strong(text("Opponent and team", "Gegner und Team"));
                                    ui.label(text("Selects the left opponent and uses your in-game Arena team, following ADBAutoPlayer.", "Wie im ADBAutoPlayer wird der linke Gegner gewählt. Es wird dein im Spiel eingestelltes Arena-Team verwendet."));
                                }
                                StageAction::SupremeArena => {
                                    ui.strong(text("Daily attempts", "Tägliche Versuche"));
                                    ui.horizontal(|ui| {
                                        ui.label(text("Maximum battles", "Maximale Kämpfe"));
                                        ui.add(egui::DragValue::new(&mut self.supreme_settings.attempts).range(1..=10));
                                    });
                                    ui.small(text("Stops when your available attempts run out.", "Endet vorher, wenn keine vorhandenen Versuche mehr verfügbar sind. Es werden keine Versuche gekauft."));
                                    ui.add_space(SpaceTokens::MD);
                                    ui.strong(text("Opponent selection", "Gegnerauswahl"));
                                    ui.horizontal(|ui| {
                                        for position in OpponentPosition::ALL {
                                            ui.selectable_value(&mut self.supreme_settings.opponent,position,position.label());
                                        }
                                    });
                                    ui.label(text("Uses your three in-game teams. Selects Next twice, then Battle.", "Verwendet deine drei im Spiel eingestellten Teams. Kampfablauf: zweimal Weiter, dann Kampf."));
                                }
                                StageAction::Daily | StageAction::AfkRewards | StageAction::FreeHourglasses | StageAction::DiamondHourglasses | StageAction::StageRewards | StageAction::SeasonRankingRewards
                                | StageAction::FriendRewards | StageAction::MailRewards | StageAction::BattlePassRewards
                                | StageAction::DailyQuestRewards | StageAction::GuildQuestRewards
                                | StageAction::CollectAffinity | StageAction::LevelUpHeroes | StageAction::EquipNewEquipment
                                | StageAction::Emporium | StageAction::SinglePull | StageAction::EssenceSwap => {}
                            }
                            if matches!(action, StageAction::AfkStages | StageAction::SeasonLegendTrial | StageAction::DurasTrials) {
                                ui.add_space(SpaceTokens::MD);
                                ui.separator();
                                ui.add_space(SpaceTokens::MD);
                            }
                            if matches!(action, StageAction::AfkStages | StageAction::SeasonLegendTrial | StageAction::DurasTrials | StageAction::UnionCampaign) {
                                show_battle_options(ui, &mut self.battle_options[action.index()]);
                            }
                        });
                    });
                ui.add_space(SpaceTokens::MD);
                ui.separator();
                ui.add_space(SpaceTokens::SM);
                ui.small(text("Changes apply immediately to this action.", "Änderungen gelten direkt für diese Aktion."));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.button(text("Close", "Schließen")).clicked();
                });
            });
        if close || response.should_close() {
            self.action_settings_dialog = None;
        }
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
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

const MUTED: egui::Color32 = design_system::ColorTokens::TEXT_SECONDARY;

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, ColorTokens::BG_VIEWPORT);
        egui::Frame::new()
            .inner_margin(egui::Margin::same(GeometryTokens::APP_INSET))
            .show(ui, |ui| {
                let context = ui.ctx().clone();
                self.receive_events(&context);
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
                                text("Settings", "Einstellungen"),
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
                                                text("Downloading update", "Update wird geladen")
                                            } else {
                                                text("Automation running", "Automation läuft")
                                            },
                                        );
                                        ui.add_space(SpaceTokens::SM);
                                    }
                                    if let Some(serial) = &self.selected_serial {
                                        status_badge(
                                            ui,
                                            &format_text!(
                                                "CONNECTED · {serial}",
                                                "VERBUNDEN · {serial}"
                                            ),
                                            true,
                                        );
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

fn daily_drag_label(ui: &mut egui::Ui, label: &str) {
    ui.horizontal(|ui| {
        let (grip, _) = ui.allocate_exact_size(egui::vec2(12.0, 16.0), egui::Sense::hover());
        for x in [-2.0, 2.0] {
            for y in [-4.0, 0.0, 4.0] {
                ui.painter()
                    .circle_filled(grip.center() + egui::vec2(x, y), 1.15, MUTED);
            }
        }
        ui.label(label);
    });
}

fn scroll_daily_drag(ui: &egui::Ui, viewport: egui::Rect) {
    let context = ui.ctx();
    if !ui.is_enabled() || !egui::DragAndDrop::has_payload_of_type::<StageAction>(context) {
        return;
    }
    let area = viewport.translate(ui.max_rect().min.to_vec2());
    let Some(pointer) = context
        .pointer_latest_pos()
        .filter(|_| ui.rect_contains_pointer(area))
    else {
        return;
    };
    let edge = (area.height() * 0.25).min(44.0);
    let from_top = pointer.y - area.top();
    let from_bottom = area.bottom() - pointer.y;
    let content_height = ui.min_rect().bottom() - ui.max_rect().top();
    let amount = if from_top < edge && viewport.top() > 0.5 {
        1.0 - from_top / edge
    } else if from_bottom < edge && viewport.bottom() < content_height - 0.5 {
        from_bottom / edge - 1.0
    } else {
        return;
    };
    let dt = ui.input(|input| {
        if input.pointer.primary_down() {
            input.stable_dt.min(1.0 / 30.0)
        } else {
            0.0
        }
    });
    if dt == 0.0 {
        return;
    }
    ui.scroll_with_delta_animation(
        egui::vec2(0.0, 500.0 * amount * dt),
        egui::style::ScrollAnimation::none(),
    );
    context.request_repaint();
}

fn show_battle_options(ui: &mut egui::Ui, options: &mut ActionBattleOptions) {
    ui.strong(text(
        "Team selection and battle attempts",
        "Teamwahl und Kampfversuche",
    ));
    ui.add_space(SpaceTokens::SM);
    design_system::active_checkbox(
        ui,
        &mut options.suggested_formations,
        text(
            "Copy recommended teams from the leaderboards",
            "Empfohlene Teams aus den Ranglisten übernehmen",
        ),
    );
    design_system::active_checkbox(
        ui,
        &mut options.current_team_first,
        text(
            "Try the current team first",
            "Aktuelles Team zuerst versuchen",
        ),
    );
    design_system::active_checkbox(
        ui,
        &mut options.skip_manual_formations,
        text("Skip manual teams", "Manuelle Teams überspringen"),
    );
    ui.small(text(
        "When unchecked, manual teams are tried after automatic suggestions.",
        "Ohne Haken werden manuelle Teams nach den automatischen Vorschlägen versucht.",
    ));
    ui.add_space(SpaceTokens::MD);
    egui::Grid::new("battle-attempt-options")
        .num_columns(2)
        .spacing(egui::vec2(SpaceTokens::XL, SpaceTokens::SM))
        .show(ui, |ui| {
            ui.label(text("Recommended teams", "Empfohlene Teams"));
            ui.add_enabled(
                options.suggested_formations,
                egui::DragValue::new(&mut options.max_formations).range(1..=10),
            );
            ui.end_row();
            ui.label(text("Attempts per team", "Versuche pro Team"));
            ui.add(egui::DragValue::new(&mut options.attempts_per_formation).range(1..=10));
            ui.end_row();
        });
    ui.add_space(SpaceTokens::SM);
    ui.small(text(
        "Locked teams are skipped. Gold is never spent.",
        "Gesperrte Teams werden übersprungen. Gold wird nicht ausgegeben.",
    ));
}

fn settings_gear(ui: &mut egui::Ui, action: &str, active: bool) -> egui::Response {
    let label = format_text!("{action}: Open settings", "{action}: Einstellungen öffnen");
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
        .on_hover_text(format_text!(
            "Update {version} available – View patch notes",
            "Update {version} verfügbar – Patchnotes anzeigen"
        ));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format_text!("Update {version} available", "Update {version} verfügbar"),
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
