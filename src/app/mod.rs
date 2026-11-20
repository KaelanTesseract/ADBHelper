//! Desktop window: shared state, events, and the main frame.
//! Pages and dialogs live in the submodules.

mod action_settings;
mod daily_editor;
mod dashboard;
mod fishing_warning;
mod install_migration;
mod run;
mod settings_page;
mod updates;
mod widgets;

use action_settings::*;
use updates::*;
use widgets::*;

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
use crate::afk_journey::progression::scanner;
use crate::design_system::{
    self, card, primary_button, status_badge, ColorTokens, GeometryTokens, SizeTokens, SpaceTokens,
    TypeTokens,
};
use crate::localization::{tr, Language};
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
    ScanExport(StageAction, std::path::PathBuf),
    UpdateChecked(Result<Option<updater::Release>, String>),
    UpdateArtworkLoaded(updater::ReleaseArtwork, Option<egui::ColorImage>),
    UpdateCheckStarted,
    UpdateProgress(u8),
    UpdateReady(Result<(), String>),
    InstallerProgress(u8),
    InstallerReady(Result<std::path::PathBuf, String>),
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

#[derive(Clone)]
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
    guild_scan: scanner::GuildSettings,
}

impl RunSettings {
    fn mode(&self, action: StageAction) -> Option<GameMode> {
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
            StageAction::HeroScan => Some(GameMode::HeroScan),
            StageAction::GuildScan => Some(GameMode::GuildScan),
            StageAction::Fishing => Some(GameMode::Fishing),
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

pub struct PlayerApp {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    devices: Vec<Device>,
    selected_serial: Option<String>,
    logs: VecDeque<String>,
    log_file: crate::logfile::LogFile,
    log_folder_error: Option<String>,
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
    dura_sweep_when_available: bool,
    dream_spend_gold: bool,
    dream_battle_limit: u32,
    dream_battle_limit_error: Option<String>,
    guild_scan: scanner::GuildSettings,
    guild_access_error: Option<String>,
    scan_exports: Vec<(StageAction, std::path::PathBuf)>,
    active_action: Option<StageAction>,
    battle_options: [ActionBattleOptions; 6],
    arena_settings: ArenaSettings,
    supreme_settings: SupremeSettings,
    action_settings_dialog: Option<StageAction>,
    recovery_notice: Option<String>,
    daily_notice: Option<String>,
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
    fishing_warning_open: bool,
    fishing_warning_accepted: bool,
    runs_installed: bool,
    installer_progress: Option<u8>,
    installer_error: Option<String>,
    old_copy: Option<std::path::PathBuf>,
    old_copy_error: Option<String>,
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
            Err(error) => (
                dream_realm::Settings::default().max_battles,
                Some(error.to_string()),
            ),
        };
        let logo_image =
            image::load_from_memory(include_bytes!("../../assets/adbhelper-logo-v2.png"))
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
            log_file: crate::logfile::LogFile::open_default(),
            log_folder_error: None,
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
            dura_sweep_when_available: true,
            dream_spend_gold: false,
            dream_battle_limit,
            dream_battle_limit_error,
            guild_scan: {
                let mut settings = scanner::GuildSettings::default();
                settings.load_saved_access();
                settings
            },
            guild_access_error: None,
            scan_exports: Vec::new(),
            active_action: None,
            battle_options: [ActionBattleOptions::default(); 6],
            arena_settings: ArenaSettings::default(),
            supreme_settings: SupremeSettings::default(),
            action_settings_dialog: None,
            recovery_notice: None,
            daily_notice: None,
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
            fishing_warning_open: false,
            fishing_warning_accepted: false,
            runs_installed: crate::install_info::runs_installed(),
            installer_progress: None,
            installer_error: None,
            old_copy: crate::install_info::standalone_copy(),
            old_copy_error: None,
            update_requests,
        };
        app.log(format!(
            "ADBHelper {} · {}",
            updater::VERSION,
            std::env::consts::OS
        ));
        app.log(tr!("app-afk-journey-app-started-connect-an"));
        app.refresh_devices();
        let update_sender = app.sender.clone();
        let update_context = context.egui_ctx.clone();
        thread::spawn(move || {
            updater::watch_updates(update_receiver, |event| {
                let message = match event {
                    updater::CheckEvent::Started => Event::UpdateCheckStarted,
                    updater::CheckEvent::Finished(result) => Event::UpdateChecked(*result),
                };
                let _ = update_sender.send(message);
                update_context.request_repaint();
            })
        });
        design_system::apply_theme(&context.egui_ctx);
        app
    }

    fn log(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.log_file.write(&message);
        if self.logs.len() >= 300 {
            self.logs.pop_front();
        }
        self.logs.push_back(message);
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

    fn receive_events(&mut self, context: &egui::Context) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::DailyStep(index) => self.daily_step = Some(index),
                Event::ScanExport(action, path) => self.scan_exports.push((action, path)),
                Event::Devices(Ok(devices)) => {
                    if self
                        .selected_serial
                        .as_ref()
                        .is_none_or(|serial| !devices.iter().any(|device| &device.serial == serial))
                    {
                        self.selected_serial = devices.first().map(|device| device.serial.clone());
                    }
                    if devices.is_empty() {
                        self.log(tr!("app-no-authorized-adb-devices-found"));
                    } else {
                        self.log(tr!(
                            "app-found-devices-authorized-adb-device-s",
                            devices = devices.len()
                        ));
                    }
                    self.devices = devices;
                }
                Event::Devices(Err(error)) => {
                    self.log(tr!("app-adb-device-discovery-failed-error", error = error))
                }
                Event::Log(message) => self.log(message),
                Event::RecoveryStatus(message) => {
                    self.log(&message);
                    self.recovery_notice = Some(message);
                }
                Event::Activity(event) => {
                    if let ActivityEvent::InviteLetterOverBudget { price, budget } = event {
                        let notice = tr!(
                            "app-invite-letter-skipped-price-price-diamonds",
                            price = price,
                            budget = budget
                        );
                        self.log(&notice);
                        self.daily_notice = Some(notice);
                    }
                    if matches!(event, ActivityEvent::ModeEntered(_)) {
                        self.recovery_notice = None;
                    }
                    self.activity.apply(event);
                }
                Event::StagesFinished(action, error) => {
                    self.recovery_notice = None;
                    self.daily_step = None;
                    self.log(crate::timing::summary());
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
                Event::InstallerProgress(percent) => self.installer_progress = Some(percent),
                Event::InstallerReady(result) => self.installer_ready(result),
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
                                tr!("app-settings"),
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
                                                tr!("app-downloading-update")
                                            } else {
                                                tr!("app-automation-running")
                                            },
                                        );
                                        ui.add_space(SpaceTokens::SM);
                                    }
                                    if let Some(serial) = &self.selected_serial {
                                        status_badge(
                                            ui,
                                            &tr!("app-connected-serial", serial = serial),
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
        self.show_fishing_warning(ui.ctx());
        self.show_old_copy_notice(ui.ctx());
    }
}

#[derive(Clone, Copy)]
enum WindowButton {
    Minimize,
    Maximize { maximized: bool },
    Close,
}
