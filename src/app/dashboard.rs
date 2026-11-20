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
                    ui.heading(egui::RichText::new(tr!("app-dashboard-actions")).color(ColorTokens::TEXT_PRIMARY));
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
                                ui.label(tr!("app-dashboard-push-selected-stages"));
                                ui.add_space(SpaceTokens::SM);
                                ui.add_enabled_ui(!self.busy, |ui| {
                                    ui.spacing_mut().interact_size.y = 20.0;
                                    ui.horizontal(|ui| {
                                        design_system::active_checkbox(
                                            ui,
                                            &mut self.stage_selection.season,
                                            tr!("app-action-settings-season-stages"),
                                        );
                                        design_system::active_checkbox(
                                            ui,
                                            &mut self.stage_selection.normal,
                                            tr!("app-action-settings-normal-stages"),
                                        );
                                    });
                                });
                                if self.stage_selection.season && self.stage_selection.normal {
                                    ui.small(tr!("app-dashboard-season-stages-first"));
                                }
                                ui.add_space(SpaceTokens::SM);
                                ui.separator();
                                ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !self.busy
                                        && (self.stage_selection.season
                                            || self.stage_selection.normal),
                                    primary_button(tr!("app-dashboard-start")),
                                )
                                .clicked()
                            {
                                self.start_action(StageAction::AfkStages);
                            }
                            if let Some(cancel) = self.stage_cancel.as_ref().filter(|_| active) {
                                let cancel = cancel.clone();
                                if ui.add(design_system::stop_button("Stop")).clicked() {
                                    cancel.store(true, Ordering::Relaxed);
                                    self.log(tr!("app-dashboard-stop-requested-for-the-afk-stages"));
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
                                ui.label(tr!("app-dashboard-push-available-faction-towers"));
                                ui.small(tr!("app-dashboard-selected-towers-selected", selected = self.tower_selection.selected().len()));
                                ui.add_space(SpaceTokens::SM);
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            !self.busy
                                                && !self.tower_selection.selected().is_empty(),
                                            primary_button(tr!("app-dashboard-start")),
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
                                            tr!("app-dashboard-stop-requested-for-the-legend-trial"),
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
                                ui.label(tr!("app-dashboard-push-guild-floors"));
                                ui.add_space(SpaceTokens::SM);
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(!self.busy, primary_button(tr!("app-dashboard-start")))
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
                                                    tr!("app-dashboard-stop-requested-for-union-campaign"),
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
                                ui.label(tr!("app-dashboard-push-stages-sweep-when-available"));
                                ui.add_space(SpaceTokens::SM);
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(!self.busy, primary_button(tr!("app-dashboard-start")))
                                        .clicked()
                                    {
                                        self.start_action(StageAction::DurasTrials);
                                    }
                                    if dura_active {
                                        if let Some(cancel) = self.stage_cancel.clone() {
                                            if ui.add(design_system::stop_button("Stop")).clicked()
                                            {
                                                cancel.store(true, Ordering::Relaxed);
                                                self.log(tr!("app-dashboard-stop-requested-for-dura-s-trials"));
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
                                tr!("app-dashboard-up-to-one-battle-per-run").to_owned()
                            } else {
                                tr!("app-dashboard-up-to-dream-battle-limit-battles", dream_battle_limit = self.dream_battle_limit)
                            };
                            self.show_battle_card(ui, StageAction::DreamRealm, &dream_description);
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::Arena, tr!("app-dashboard-play-daily-arena-battles"));
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::SupremeArena, tr!("app-dashboard-play-daily-supreme-arena-battles"));
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::HeroScan, tr!("app-dashboard-export-your-hero-roster-for-afkj"));
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::GuildScan, tr!("app-dashboard-export-guild-activity-and-rankings"));
                            ui.add_space(SpaceTokens::MD);
                            self.show_battle_card(ui, StageAction::Fishing, tr!("app-dashboard-walk-to-fishing-spots-and-fish"));
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
                    ui.heading(egui::RichText::new(tr!("app-dashboard-activity")).color(ColorTokens::TEXT_PRIMARY));
                    ui.add_space(SpaceTokens::SM);
                    card(ui, |ui| {
                        ui.small(if self.activity.is_running() {
                            tr!("app-dashboard-current-game-mode")
                        } else {
                            tr!("app-dashboard-last-game-mode")
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
                                ui.label(tr!("app-dashboard-opening-game-mode"));
                                if let Some(target) = self.activity.target {
                                    ui.small(tr!("app-dashboard-target-label", label = target.label()));
                                }
                            }
                            None => {
                                ui.label(tr!("app-dashboard-no-mode-started-yet"));
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
                                        GameMode::CollectAffinity => tr!("app-dashboard-heroes-visited"),
                                        GameMode::LevelUpHeroes => tr!("app-dashboard-levels-gained"),
                                        GameMode::EquipNewEquipment => tr!("app-dashboard-classes-equipped"),
                                        GameMode::Emporium => tr!("app-dashboard-purchases"),
                                        GameMode::SinglePull => tr!("app-dashboard-recruits"),
                                        GameMode::EssenceSwap => tr!("app-dashboard-swaps"),
                                        GameMode::HeroScan => tr!("app-dashboard-heroes-scanned"),
                                        GameMode::GuildScan => tr!("app-dashboard-rows-exported"),
                                        GameMode::Fishing => tr!("app-dashboard-caught"),
                                        _ => tr!("app-dashboard-collected"),
                                    };
                                    progress_metric(ui, label, stats.collected);
                                    return;
                                }
                                progress_metric(ui, tr!("app-dashboard-attempts"), stats.attempts);
                                ui.add_space(SpaceTokens::MD);
                                if *mode == GameMode::DreamRealm {
                                    progress_metric(ui, tr!("app-dashboard-completed"), stats.scored);
                                    ui.add_space(SpaceTokens::SM);
                                    progress_metric(ui, tr!("app-dashboard-collected"), stats.collected);
                                    return;
                                }
                                progress_metric(
                                    ui,
                                    if mode.is_pvp() { tr!("app-dashboard-wins") } else { tr!("app-dashboard-cleared") },
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
                    .add_enabled(!self.busy, primary_button(tr!("app-dashboard-start")))
                    .clicked()
                {
                    self.start_action(action);
                }
                if active {
                    if let Some(cancel) = self.stage_cancel.clone() {
                        if ui.add(design_system::stop_button("Stop")).clicked() {
                            cancel.store(true, Ordering::Relaxed);
                            self.log(tr!(
                                "app-dashboard-stop-requested-for-label",
                                label = action.label()
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
                    .small_button(tr!("app-dashboard-copy-export-path"))
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
                    ui.label(tr!(
                        "app-dashboard-step-index-steps-label",
                        index = index + 1,
                        steps = self.daily_routine.steps.len(),
                        label = step.label()
                    ));
                }
            } else {
                ui.label(tr!(
                    "app-dashboard-steps-steps-your-daily-routine",
                    steps = self.daily_routine.steps.len()
                ));
            }
            ui.add_space(SpaceTokens::SM);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.busy && !self.daily_routine.steps.is_empty(),
                        primary_button(tr!("app-dashboard-start")),
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
                            tr!("app-dashboard-recovering")
                        } else {
                            tr!("app-dashboard-running")
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
