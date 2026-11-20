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
                                    design_system::active_checkbox(ui, &mut self.dura_sweep_when_available,
                                        text("Run daily sweeps when Sweep is available", "Tägliche Sweeps ausführen, wenn Sweep verfügbar ist"));
                                    ui.small(text("Uses daily attempts after verifying the quantity and token cost.", "Verwendet tägliche Versuche nach Prüfung von Menge und Token-Kosten."));
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
                                StageAction::Fishing => {
                                    ui.label(text("Uses available fishing spots on the current region map.", "Nutzt verfügbare Angelstellen auf der Karte der aktuellen Region."));
                                }
                                StageAction::HeroScan => {
                                    ui.label(text("Import the completed JSON backup in AFKJ Tracker.", "Importiere die vollständige JSON-Sicherung in AFKJ Tracker."));
                                    ui.hyperlink_to("AFKJ Tracker", "https://afkj-tracker.vercel.app/");
                                }
                                StageAction::GuildScan => {
                                    ui.horizontal(|ui| {
                                        ui.label(text("Past Dream Realm days", "Vergangene Dream-Realm-Tage"));
                                        ui.add(egui::DragValue::new(&mut self.guild_scan.dream_days).range(0..=3));
                                    });
                                    ui.checkbox(&mut self.guild_scan.activeness, text("Guild activeness", "Gildenaktivität"));
                                    ui.checkbox(&mut self.guild_scan.supreme_arena, "Supreme Arena");
                                    ui.checkbox(&mut self.guild_scan.afk_stages, text("Current Season AFK phase", "Aktuelle Season-AFK-Phase"));
                                    ui.label(text("Guild Manager API URL (optional)", "Guild-Manager-API-URL (optional)"));
                                    let url = ui.add(egui::TextEdit::singleline(&mut self.guild_scan.guild_members_url).desired_width(f32::INFINITY));
                                    if url.changed() {
                                        self.guild_scan.take_pasted_key();
                                    }
                                    ui.label(text("Guild Manager API key", "Guild-Manager-API-Schlüssel"));
                                    let key = ui.add(egui::TextEdit::singleline(&mut self.guild_scan.guild_api_key).password(true).desired_width(f32::INFINITY));
                                    if url.lost_focus() || key.lost_focus() {
                                        self.guild_access_error = self.guild_scan.save_access().err().map(String::from);
                                    }
                                    ui.small(text("A key pasted inside the URL moves to the key field. URL and key are stored in the Windows Credential Manager and the key is sent only as a request header.", "Ein in der URL eingefügter Schlüssel wandert in das Schlüsselfeld. URL und Schlüssel liegen in der Windows-Anmeldeinformationsverwaltung, der Schlüssel wird nur im Header gesendet."));
                                    if (!self.guild_scan.guild_members_url.is_empty() || !self.guild_scan.guild_api_key.is_empty())
                                        && ui.button(text("Forget saved access", "Gespeicherten Zugang entfernen")).clicked()
                                    {
                                        self.guild_access_error = self.guild_scan.forget_access().err().map(String::from);
                                    }
                                    if let Some(error) = &self.guild_access_error {
                                        ui.label(format_text!("Could not update the saved access: {error}", "Der gespeicherte Zugang konnte nicht geändert werden: {error}"));
                                    }
                                    ui.small(text("Reads the roster for name matching. Exports are saved locally.", "Liest die Mitgliederliste zum Namensabgleich. Exporte werden lokal gespeichert."));
                                    ui.hyperlink_to("AFKJ Guild Manager", "https://afkj-guildmanager.vercel.app/");
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
}

pub(super) fn show_battle_options(ui: &mut egui::Ui, options: &mut ActionBattleOptions) {
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

pub(super) fn settings_gear(ui: &mut egui::Ui, action: &str, active: bool) -> egui::Response {
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
