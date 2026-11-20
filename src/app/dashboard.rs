//! The automations page: action cards, Daily card, and progress statistics.

use super::*;

impl PlayerApp {
    pub(super) fn show_dashboard(&mut self, ui: &mut egui::Ui, dashboard_height: f32) {
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
                                ui.label(text("Push stages · Sweep when available", "Stufen pushen · Sweep bei Verfügbarkeit"));
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
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::HeroScan, text("Export your hero roster for AFKJ Tracker", "Heldenkader für AFKJ Tracker exportieren"));
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::GuildScan, text("Export guild activity and rankings", "Gildenaktivität und Ranglisten exportieren"));
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::Fishing, text("Walk to fishing spots and fish", "Angelstellen anlaufen und angeln"));
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
                                        GameMode::CollectAffinity => text("HEROES VISITED", "HELDEN BESUCHT"),
                                        GameMode::LevelUpHeroes => text("LEVELS GAINED", "STUFEN GEWONNEN"),
                                        GameMode::EquipNewEquipment => text("CLASSES EQUIPPED", "KLASSEN AUSGERÜSTET"),
                                        GameMode::Emporium => text("PURCHASES", "KÄUFE"),
                                        GameMode::SinglePull => text("RECRUITS", "REKRUTIERUNGEN"),
                                        GameMode::EssenceSwap => text("SWAPS", "TAUSCHE"),
                                        GameMode::HeroScan => text("HEROES SCANNED", "HELDEN ERFASST"),
                                        GameMode::GuildScan => text("ROWS EXPORTED", "ZEILEN EXPORTIERT"),
                                        GameMode::Fishing => text("CAUGHT", "GEFANGEN"),
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
                            if let Some(reason) = stats.hourglass_stop {
                                ui.label(reason.message());
                            }
                        });
                    }
                        });
                },
            );
        });
    }

    pub(super) fn show_battle_card(
        &mut self,
        ui: &mut egui::Ui,
        action: StageAction,
        description: &str,
    ) {
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
            for (_, path) in self
                .scan_exports
                .iter()
                .filter(|(owner, _)| *owner == action)
            {
                if ui
                    .small_button(text("Copy export path", "Exportpfad kopieren"))
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    ui.ctx().copy_text(path.display().to_string());
                }
                if let Some(name) = path.file_name() {
                    ui.small(name.to_string_lossy());
                }
            }
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

    pub(super) fn show_daily_card(&mut self, ui: &mut egui::Ui) {
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
            if let Some(notice) = &self.daily_notice {
                ui.colored_label(ColorTokens::TEXT_SECONDARY, notice);
            }
        });
    }

    pub(super) fn show_action_header(
        &mut self,
        ui: &mut egui::Ui,
        action: StageAction,
        active: bool,
    ) {
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
}

pub(super) fn progress_metric(ui: &mut egui::Ui, label: &str, value: u64) {
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
