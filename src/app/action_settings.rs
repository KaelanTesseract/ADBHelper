//! The settings popup of a single action.

use super::*;

impl PlayerApp {
    pub(super) fn show_action_settings_dialog(&mut self, context: &egui::Context) {
        let Some(action) = self.action_settings_dialog else {
            return;
        };
        if action == StageAction::Daily {
            self.show_daily_settings(context);
            return;
        }
        let mut close = false;
        let response =
            egui::Modal::new(egui::Id::unique(("action-settings-dialog", action.index())))
                .frame(design_system::panel_frame())
                .show(context, |ui| {
                    ui.set_width((context.content_rect().width() - 64.0).clamp(260.0, 500.0));
                    ui.heading(action.label());
                    ui.label(tr!("app-action-settings-action-settings"));
                    ui.add_space(SpaceTokens::SM);
                    ui.separator();
                    ui.add_space(SpaceTokens::SM);
                    if self.busy {
                        ui.small(tr!(
                            "app-action-settings-action-settings-are-read-only-during"
                        ));
                        ui.add_space(SpaceTokens::SM);
                    }
                    egui::ScrollArea::vertical()
                        .id_salt(("action-settings-content", action.index()))
                        .max_height((context.content_rect().height() - 230.0).clamp(100.0, 540.0))
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!self.busy, |ui| {
                                match action {
                                    StageAction::AfkStages => {
                                        ui.strong(tr!("app-action-settings-stage-selection"));
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
                                        ui.small(tr!(
                                    "app-action-settings-when-both-are-selected-season-stages"
                                ));
                                    }
                                    StageAction::SeasonLegendTrial => {
                                        ui.strong(tr!("app-action-settings-faction-towers"));
                                        ui.label(tr!(
                                            "app-action-settings-towers-that-are-closed-today-are"
                                        ));
                                        egui::Grid::new("legend-trial-towers").num_columns(2).show(
                                            ui,
                                            |ui| {
                                                for (index, tower) in
                                                    LegendTower::ALL.into_iter().enumerate()
                                                {
                                                    design_system::active_checkbox(
                                                        ui,
                                                        &mut self.tower_selection.enabled[index],
                                                        tower.label(),
                                                    );
                                                    if index % 2 == 1 {
                                                        ui.end_row();
                                                    }
                                                }
                                            },
                                        );
                                        ui.small(tr!(
                                    "app-action-settings-order-lightbearer-wilder-graveborn-mauler"
                                ));
                                    }
                                    StageAction::DurasTrials => {
                                        ui.strong(tr!("app-action-settings-daily-sweeps"));
                                        design_system::active_checkbox(
                                    ui,
                                    &mut self.dura_sweep_when_available,
                                    tr!("app-action-settings-run-daily-sweeps-when-sweep-is"),
                                );
                                        ui.small(tr!(
                                    "app-action-settings-uses-daily-attempts-after-verifying-the"
                                ));
                                    }
                                    StageAction::UnionCampaign => {}
                                    StageAction::DreamRealm => {
                                        ui.label(tr!(
                                            "app-action-settings-uses-your-in-game-formation-and"
                                        ));
                                        ui.horizontal(|ui| {
                                            ui.label(tr!(
                                                "app-action-settings-maximum-battles-per-run"
                                            ));
                                            if ui
                                                .add(
                                                    egui::DragValue::new(
                                                        &mut self.dream_battle_limit,
                                                    )
                                                    .range(1..=dream_realm::MAX_BATTLES),
                                                )
                                                .changed()
                                            {
                                                self.dream_battle_limit_error =
                                                    dream_realm::save_battle_limit(
                                                        self.dream_battle_limit,
                                                    )
                                                    .err()
                                                    .map(|error| error.to_string());
                                            }
                                        });
                                        if let Some(error) = &self.dream_battle_limit_error {
                                            ui.colored_label(
                                                ColorTokens::CLOSE_HOVER,
                                                tr!(
                                            "app-action-settings-battle-limit-could-not-be-loaded",
                                            error = error
                                        ),
                                            );
                                        }
                                        design_system::active_checkbox(
                                            ui,
                                            &mut self.dream_spend_gold,
                                            tr!("app-action-settings-spend-gold-on-extra-attempts"),
                                        );
                                        ui.small(tr!(
                                    "app-action-settings-extra-attempts-require-a-verified-gold"
                                ));
                                        ui.small(tr!(
                                    "app-action-settings-in-daily-collect-yesterday-s-ranking"
                                ));
                                    }
                                    StageAction::Arena => {
                                        ui.strong(tr!("app-action-settings-daily-attempts"));
                                        ui.horizontal(|ui| {
                                            ui.label(tr!("app-action-settings-available-attempts"));
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut self.arena_settings.attempts,
                                                )
                                                .range(1..=5),
                                            );
                                        });
                                        design_system::active_checkbox(
                                            ui,
                                            &mut self.arena_settings.claim_free_attempts,
                                            tr!("app-action-settings-claim-up-to-two-extra-free"),
                                        );
                                        ui.small(tr!(
                                    "app-action-settings-extra-attempts-require-a-verified-price"
                                ));
                                        ui.add_space(SpaceTokens::MD);
                                        ui.strong(tr!("app-action-settings-opponent-and-team"));
                                        ui.label(tr!(
                                    "app-action-settings-selects-the-left-opponent-and-uses"
                                ));
                                    }
                                    StageAction::SupremeArena => {
                                        ui.strong(tr!("app-action-settings-daily-attempts"));
                                        ui.horizontal(|ui| {
                                            ui.label(tr!("app-action-settings-maximum-battles"));
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut self.supreme_settings.attempts,
                                                )
                                                .range(1..=10),
                                            );
                                        });
                                        ui.small(tr!(
                                    "app-action-settings-stops-when-your-available-attempts-run"
                                ));
                                        ui.add_space(SpaceTokens::MD);
                                        ui.strong(tr!("app-action-settings-opponent-selection"));
                                        ui.horizontal(|ui| {
                                            for position in OpponentPosition::ALL {
                                                ui.selectable_value(
                                                    &mut self.supreme_settings.opponent,
                                                    position,
                                                    position.label(),
                                                );
                                            }
                                        });
                                        ui.label(tr!(
                                            "app-action-settings-uses-your-three-in-game-teams"
                                        ));
                                    }
                                    // Started and set up on the Settings page.
                                    StageAction::HeroDatabase | StageAction::HeroBoxScan => {}
                                    StageAction::Fishing => {
                                        ui.label(tr!(
                                    "app-action-settings-uses-available-fishing-spots-on-the"
                                ));
                                    }
                                    StageAction::HeroScan => {
                                        ui.label(tr!(
                                    "app-action-settings-import-the-completed-json-backup-in"
                                ));
                                        ui.hyperlink_to(
                                            "AFKJ Tracker",
                                            "https://afkj-tracker.vercel.app/",
                                        );
                                    }
                                    StageAction::GuildScan => {
                                        ui.horizontal(|ui| {
                                            ui.label(tr!(
                                                "app-action-settings-past-dream-realm-days"
                                            ));
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut self.guild_scan.dream_days,
                                                )
                                                .range(0..=3),
                                            );
                                        });
                                        ui.checkbox(
                                            &mut self.guild_scan.activeness,
                                            tr!("app-action-settings-guild-activeness"),
                                        );
                                        ui.checkbox(
                                            &mut self.guild_scan.supreme_arena,
                                            "Supreme Arena",
                                        );
                                        ui.checkbox(
                                            &mut self.guild_scan.afk_stages,
                                            tr!("app-action-settings-current-season-afk-phase"),
                                        );
                                        ui.label(tr!(
                                            "app-action-settings-guild-manager-api-url-optional"
                                        ));
                                        let url = ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.guild_scan.guild_members_url,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                        if url.changed() {
                                            self.guild_scan.take_pasted_key();
                                        }
                                        ui.label(tr!("app-action-settings-guild-manager-api-key"));
                                        let key = ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.guild_scan.guild_api_key,
                                            )
                                            .password(true)
                                            .desired_width(f32::INFINITY),
                                        );
                                        if url.lost_focus() || key.lost_focus() {
                                            self.guild_access_error = self
                                                .guild_scan
                                                .save_access()
                                                .err()
                                                .map(String::from);
                                        }
                                        ui.small(tr!(
                                            "app-action-settings-a-key-pasted-inside-the-url"
                                        ));
                                        if (!self.guild_scan.guild_members_url.is_empty()
                                            || !self.guild_scan.guild_api_key.is_empty())
                                            && ui
                                                .button(tr!(
                                                    "app-action-settings-forget-saved-access"
                                                ))
                                                .clicked()
                                        {
                                            self.guild_access_error = self
                                                .guild_scan
                                                .forget_access()
                                                .err()
                                                .map(String::from);
                                        }
                                        if let Some(error) = &self.guild_access_error {
                                            ui.label(tr!(
                                        "app-action-settings-could-not-update-the-saved-access",
                                        error = error
                                    ));
                                        }
                                        ui.small(tr!(
                                    "app-action-settings-reads-the-roster-for-name-matching"
                                ));
                                        ui.hyperlink_to(
                                            "AFKJ Guild Manager",
                                            "https://afkj-guildmanager.vercel.app/",
                                        );
                                    }
                                    StageAction::Daily
                                    | StageAction::AfkRewards
                                    | StageAction::FreeHourglasses
                                    | StageAction::DiamondHourglasses
                                    | StageAction::StageRewards
                                    | StageAction::SeasonRankingRewards
                                    | StageAction::FriendRewards
                                    | StageAction::MailRewards
                                    | StageAction::BattlePassRewards
                                    | StageAction::DailyQuestRewards
                                    | StageAction::GuildQuestRewards
                                    | StageAction::CollectAffinity
                                    | StageAction::LevelUpHeroes
                                    | StageAction::EquipNewEquipment
                                    | StageAction::Emporium
                                    | StageAction::SinglePull
                                    | StageAction::EssenceSwap => {}
                                }
                                if matches!(
                                    action,
                                    StageAction::AfkStages
                                        | StageAction::SeasonLegendTrial
                                        | StageAction::DurasTrials
                                ) {
                                    ui.add_space(SpaceTokens::MD);
                                    ui.separator();
                                    ui.add_space(SpaceTokens::MD);
                                }
                                if matches!(
                                    action,
                                    StageAction::AfkStages
                                        | StageAction::SeasonLegendTrial
                                        | StageAction::DurasTrials
                                        | StageAction::UnionCampaign
                                ) {
                                    show_battle_options(
                                        ui,
                                        &mut self.battle_options[action.index()],
                                    );
                                }
                            });
                        });
                    ui.add_space(SpaceTokens::MD);
                    ui.separator();
                    ui.add_space(SpaceTokens::SM);
                    ui.small(tr!(
                        "app-action-settings-changes-apply-immediately-to-this-action"
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = ui.button(tr!("app-action-settings-close")).clicked();
                    });
                });
        if close || response.should_close() {
            self.action_settings_dialog = None;
        }
    }
}

pub(super) fn show_battle_options(ui: &mut egui::Ui, options: &mut ActionBattleOptions) {
    ui.strong(tr!(
        "app-action-settings-team-selection-and-battle-attempts"
    ));
    ui.add_space(SpaceTokens::SM);
    design_system::active_checkbox(
        ui,
        &mut options.suggested_formations,
        tr!("app-action-settings-copy-recommended-teams-from-the-leaderboards"),
    );
    design_system::active_checkbox(
        ui,
        &mut options.current_team_first,
        tr!("app-action-settings-try-the-current-team-first"),
    );
    design_system::active_checkbox(
        ui,
        &mut options.skip_manual_formations,
        tr!("app-action-settings-skip-manual-teams"),
    );
    ui.small(tr!(
        "app-action-settings-when-unchecked-manual-teams-are-tried"
    ));
    ui.add_space(SpaceTokens::MD);
    egui::Grid::new("battle-attempt-options")
        .num_columns(2)
        .spacing(egui::vec2(SpaceTokens::XL, SpaceTokens::SM))
        .show(ui, |ui| {
            ui.label(tr!("app-action-settings-recommended-teams"));
            ui.add_enabled(
                options.suggested_formations,
                egui::DragValue::new(&mut options.max_formations).range(1..=10),
            );
            ui.end_row();
            ui.label(tr!("app-action-settings-attempts-per-team"));
            ui.add(egui::DragValue::new(&mut options.attempts_per_formation).range(1..=10));
            ui.end_row();
        });
    ui.add_space(SpaceTokens::SM);
    ui.small(tr!("app-action-settings-locked-teams-are-skipped-gold-is"));
}

pub(super) fn settings_gear(ui: &mut egui::Ui, action: &str, active: bool) -> egui::Response {
    let label = tr!("app-action-settings-action-open-settings", action = action);
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
