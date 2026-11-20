//! The Daily routine editor with drag-and-drop ordering.

use super::*;

impl PlayerApp {
    pub(super) fn show_daily_settings(&mut self, context: &egui::Context) {
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
                ui.heading(tr!("app-daily-editor-daily-routine"));
                ui.label(tr!("app-daily-editor-drag-actions-to-add-or-reorder"));
                ui.small(tr!("app-daily-editor-each-step-uses-the-settings-from"));
                ui.add_space(SpaceTokens::MD);
                if self.busy {
                    ui.small(tr!("app-daily-editor-stop-the-current-run-to-edit"));
                }
                let list_height = (context.content_rect().height() - 320.0).clamp(110.0, 462.0);
                let count = self.daily_routine.steps.len();
                ui.columns(2, |columns| {
                    let available = &mut columns[0];
                    available.strong(tr!("app-daily-editor-available-actions"));
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
                                    ui.label(tr!("app-daily-editor-all-actions-in-this-category-are"));
                                }
                                for action in actions {
                                    design_system::panel_frame().show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.dnd_drag_source(egui::Id::unique(("daily-source", action.index())), action, |ui| {
                                                daily_drag_label(ui, action.label());
                                            });
                                            if ui.small_button("+").on_hover_text(tr!("app-daily-editor-add-to-end")).clicked() {
                                                placement = Some((action, count));
                                            }
                                        });
                                    });
                                    ui.add_space(SpaceTokens::SM);
                                }
                            });
                    });
                    columns[1].add_enabled_ui(!self.busy, |routine| {
                        routine.strong(tr!("app-daily-editor-run-in-this-order"));
                        routine.add_space(SpaceTokens::SM);
                        egui::ScrollArea::vertical()
                            .id_salt("daily-routine-steps")
                            .max_height(list_height)
                            .auto_shrink([false, false])
                            .animated(false)
                            .show_viewport(routine, |ui, viewport| {
                                if count == 0 {
                                    ui.label(tr!("app-daily-editor-your-routine-is-empty"));
                                }
                                for (index, action) in self.daily_routine.steps.iter().copied().enumerate() {
                                    let row = design_system::panel_frame().show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.dnd_drag_source(egui::Id::unique(("daily-step", action.index())), action, |ui| {
                                            daily_drag_label(ui, &format!("{}  {}", index + 1, action.label()));
                                        });
                                        if action == StageAction::DiamondHourglasses {
                                            ui.horizontal(|ui| {
                                                ui.label(tr!("app-daily-editor-daily-purchases"));
                                                ui.add(egui::DragValue::new(&mut diamond_settings.purchases).range(1..=10));
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label(tr!("app-daily-editor-daily-diamond-budget"));
                                                ui.add(egui::DragValue::new(&mut diamond_settings.budget).range(0..=5000).speed(10));
                                            });
                                            ui.small(tr!("app-daily-editor-earlier-purchases-today-count-toward-both"));
                                        }
                                        if action == StageAction::Emporium {
                                            for (label, budget) in [
                                                (tr!("app-daily-editor-invite-letter-diamond-budget"), &mut emporium_settings.diamonds),
                                                (tr!("app-daily-editor-pal-coins-affinity"), &mut emporium_settings.pal_coins),
                                                (tr!("app-daily-editor-dream-fragments-essences"), &mut emporium_settings.dream_fragments),
                                            ] {
                                                ui.horizontal(|ui| {
                                                    ui.label(label);
                                                    ui.add(egui::DragValue::new(budget).range(0..=100_000));
                                                });
                                            }
                                            ui.checkbox(&mut emporium_settings.full_price_affinity, tr!("app-daily-editor-include-full-price-affinity-items"));
                                            ui.small(tr!("app-daily-editor-enter-the-maximum-amount-of-each"));
                                        }
                                        if action == StageAction::EssenceSwap {
                                            ui.small(tr!("app-daily-editor-place-after-emporium-to-exchange-essences"));
                                        }
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.add_enabled(index > 0, egui::Button::new(tr!("app-daily-editor-up")).small()).clicked() {
                                                placement = Some((action, index - 1));
                                            }
                                            if ui.add_enabled(index + 1 < count, egui::Button::new(tr!("app-daily-editor-down")).small()).clicked() {
                                                placement = Some((action, index + 2));
                                            }
                                            if ui.add_enabled(index + 1 < count, egui::Button::new(tr!("app-daily-editor-to-end")).small()).clicked() {
                                                placement = Some((action, count));
                                            }
                                            if ui.small_button(tr!("app-daily-editor-remove")).clicked() { remove = Some(index); }
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
                            egui::Label::new(tr!("app-daily-editor-drop-here-to-place-last"))
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
                    ui.colored_label(ColorTokens::CLOSE_HOVER, tr!("app-daily-editor-daily-settings-could-not-be-saved", error = error));
                }
                ui.small(tr!("app-daily-editor-the-routine-is-saved-automatically"));
                close = ui.button(tr!("app-action-settings-close")).clicked();
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
}

pub(super) fn daily_drag_label(ui: &mut egui::Ui, label: &str) {
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

pub(super) fn scroll_daily_drag(ui: &egui::Ui, viewport: egui::Rect) {
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
