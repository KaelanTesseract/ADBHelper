//! App updates: checks, download, patch notes dialog, and the title bar badge.

use super::*;

impl PlayerApp {
    pub(super) fn check_updates(&mut self) {
        if self.update_checking || self.update_progress.is_some() {
            return;
        }
        self.update_checking = true;
        self.update_error = None;
        self.last_update_check = Some(Instant::now());
        if self.update_requests.send(()).is_err() {
            self.update_checking = false;
            self.update_error = Some(tr!("app-updates-the-update-service-is-unavailable").into());
        }
    }

    pub(super) fn install_update(&mut self) {
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

    pub(super) fn show_update_notice(&mut self, ui: &mut egui::Ui) {
        if let Some(notice) = self.update_notice.clone() {
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(notice);
                    if ui.small_button(tr!("app-action-settings-close")).clicked() {
                        self.update_notice = None;
                    }
                });
            });
            ui.add_space(SpaceTokens::MD);
        }
    }

    pub(super) fn request_update_artwork(
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

    pub(super) fn show_update_dialog(&mut self, context: &egui::Context) {
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
                        egui::RichText::new(tr!(
                            "app-updates-update-version-available",
                            version = release.version
                        ))
                        .color(ColorTokens::GLOW),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr!("app-action-settings-close")).clicked() {
                            close = true;
                        }
                    });
                });
                ui.label(tr!(
                    "app-updates-installed-version-new-version2",
                    version = updater::VERSION,
                    version2 = release.version
                ));
                ui.strong(tr!("app-updates-patch-notes"));
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
                                        ui.small(tr!("app-updates-loading-release-graphic"));
                                    }
                                    _ => {
                                        ui.hyperlink_to(
                                            tr!("app-updates-view-release-graphic-on-github"),
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
                            primary_button(tr!("app-updates-download-and-install")),
                        )
                        .clicked()
                    {
                        self.install_update();
                    }
                    ui.hyperlink_to(tr!("app-updates-release-on-github"), &release.url);
                });
                ui.small(tr!(
                    "app-updates-adbhelper-will-restart-automatically-after-installation"
                ));
                if let Some(percent) = self.update_progress {
                    ui.add(egui::ProgressBar::new(f32::from(percent) / 100.0).text(
                        if percent == 100 {
                            tr!("app-updates-verifying-and-preparing-the-update").into()
                        } else {
                            tr!("app-updates-download-percent", percent = percent)
                        },
                    ));
                } else if self.busy {
                    ui.small(tr!("app-updates-the-update-can-be-installed-after"));
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
}

pub(super) fn update_icon(ui: &mut egui::Ui, version: &str) -> egui::Response {
    let response = ui
        .add_sized(
            egui::vec2(32.0, GeometryTokens::TITLEBAR_HEIGHT),
            egui::Button::new("").frame(false),
        )
        .on_hover_text(tr!(
            "app-updates-update-version-available-view-patch-notes",
            version = version
        ));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            tr!("app-updates-update-version-available", version = version),
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
