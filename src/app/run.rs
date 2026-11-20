//! Running actions on the device thread and reporting their progress.

use super::*;

impl PlayerApp {
    pub(super) fn run_device_action<F>(&mut self, label: &'static str, action: F)
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

    pub(super) fn start_action(&mut self, action: StageAction) {
        if self.busy {
            return;
        }
        self.action_error = None;
        self.recovery_notice = None;
        if action == StageAction::Daily {
            self.daily_notice = None;
        }
        self.daily_step = None;
        if matches!(action, StageAction::HeroScan | StageAction::GuildScan) {
            self.scan_exports.retain(|(owner, _)| *owner != action);
        }
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
            dura_sweep: self.dura_sweep_when_available,
            dream_spend_gold: self.dream_spend_gold,
            dream_battle_limit: self.dream_battle_limit,
            battle: self.battle_options,
            arena: self.arena_settings,
            supreme: self.supreme_settings,
            diamond_hourglasses: self.daily_routine.diamond_hourglasses,
            emporium: self.daily_routine.emporium,
            guild_scan: self.guild_scan.clone(),
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
                    &settings,
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
}

pub(super) fn run_step(
    serial: &str,
    action: StageAction,
    daily: bool,
    settings: &RunSettings,
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
    if matches!(mode, GameMode::HeroScan | GameMode::GuildScan) {
        return scanner::run(
            serial,
            mode,
            &settings.guild_scan,
            cancelled,
            &mut log,
            &mut |event| {
                let _ = sender.send(Event::Activity(event));
            },
            &mut |path| {
                let _ = sender.send(Event::ScanExport(action, path));
            },
        );
    }
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
                StageAction::Fishing => {
                    crate::afk_journey::fishing::run(serial, cancelled, &mut log, report)
                }
                StageAction::HeroScan | StageAction::GuildScan => unreachable!(),
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
