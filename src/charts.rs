use crate::activity::RunActivity;
use crate::design_system::{card, ColorTokens as C, SpaceTokens, TypeTokens};
use crate::localization::{format_text, text};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, Sense, Shape, Stroke};
use std::time::Instant;

fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

/// Confirmed clears are discrete: the curve stays flat between successes.
pub fn stage_history(ui: &mut egui::Ui, run: &RunActivity, height: f32) {
    ui.label(
        egui::RichText::new(if run.is_pvp_run() {
            text("WIN HISTORY", "SIEG-VERLAUF")
        } else {
            text("STAGE HISTORY", "STAGE-VERLAUF")
        })
        .size(TypeTokens::KBD)
        .color(C::TEXT_SECONDARY),
    );
    ui.small(if run.is_pvp_run() {
        text(
            "Confirmed wins over time",
            "Bestätigte Siege über die Laufzeit",
        )
    } else {
        text(
            "Stages cleared over time",
            "Geschaffte Stages über die Laufzeit",
        )
    });
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let plot = Rect::from_min_max(rect.min + vec2(29.0, 12.0), rect.max - vec2(5.0, 22.0));
    let painter = ui.painter().with_clip_rect(rect);
    let font = FontId::proportional(10.0);
    let elapsed = run.elapsed(Instant::now()).as_secs_f64();
    let span = elapsed.max(1.0);
    let maximum = run
        .stage_history
        .last()
        .map(|p| p.cleared)
        .unwrap_or(0)
        .max(1);
    for fraction in [0.0, 0.5, 1.0] {
        let y = plot.bottom() - plot.height() * fraction;
        painter.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(0.5, C::BORDER),
        );
    }
    for value in [0, maximum] {
        let y = plot.bottom() - plot.height() * value as f32 / maximum as f32;
        painter.text(
            pos2(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            value.to_string(),
            font.clone(),
            C::TEXT_SECONDARY,
        );
    }
    painter.text(
        pos2(plot.left(), rect.bottom()),
        Align2::LEFT_BOTTOM,
        "0:00",
        font.clone(),
        C::TEXT_SECONDARY,
    );
    painter.text(
        pos2(plot.right(), rect.bottom()),
        Align2::RIGHT_BOTTOM,
        clock(span),
        font.clone(),
        C::TEXT_SECONDARY,
    );
    if run.stage_history.is_empty() {
        painter.text(
            plot.center(),
            Align2::CENTER_CENTER,
            text(
                "History appears when a run starts",
                "Verlauf beginnt mit dem Lauf",
            ),
            font,
            C::TEXT_SECONDARY,
        );
        return;
    }
    let project = |seconds: f64, value: u64| {
        pos2(
            plot.left() + plot.width() * (seconds / span).clamp(0.0, 1.0) as f32,
            plot.bottom() - plot.height() * value as f32 / maximum as f32,
        )
    };
    let mut points = vec![project(0.0, 0)];
    let mut previous = 0;
    for sample in &run.stage_history {
        points.push(project(sample.elapsed.as_secs_f64(), previous));
        points.push(project(sample.elapsed.as_secs_f64(), sample.cleared));
        previous = sample.cleared;
    }
    let end = project(elapsed, previous);
    points.push(end);
    painter.add(Shape::line(
        points.clone(),
        Stroke::new(5.0, Color32::from_rgba_unmultiplied(71, 227, 224, 15)),
    ));
    painter.add(Shape::line(points, Stroke::new(1.3, C::GLOW)));
    painter.circle_filled(end, 2.5, C::GLOW);
    if response.hovered() {
        if let Some(pointer) = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| plot.contains(*p))
        {
            let time = ((pointer.x - plot.left()) / plot.width()) as f64 * span;
            let value = run
                .stage_history
                .iter()
                .rev()
                .find(|p| p.elapsed.as_secs_f64() <= time)
                .map(|p| p.cleared)
                .unwrap_or(0);
            painter.line_segment(
                [pos2(pointer.x, plot.top()), pos2(pointer.x, plot.bottom())],
                Stroke::new(0.7, C::TEXT_SECONDARY),
            );
            response.on_hover_text(format!(
                "{} · {} {}",
                clock(time),
                value,
                if run.is_pvp_run() {
                    text("Wins", "Siege")
                } else {
                    text("Stages cleared", "Stages gecleart")
                }
            ));
        }
    }
}

/// One shared categorical plot; its height never depends on the number of modes.
pub fn mode_comparison(ui: &mut egui::Ui, run: &RunActivity, height: f32) {
    let data: Vec<_> = run
        .modes
        .iter()
        .filter(|(mode, _)| !mode.is_collection())
        .map(|(mode, stats)| ComparisonPoint {
            label: mode.label().to_owned(),
            attempts: stats.attempts,
            cleared: if mode.is_pvp() {
                stats.wins
            } else {
                stats.cleared
            },
            pvp: mode.is_pvp(),
        })
        .collect();
    comparison_plot(ui, &data, height);
}

pub(crate) struct ComparisonPoint {
    pub label: String,
    pub attempts: u64,
    pub cleared: u64,
    pub pvp: bool,
}

const ATTEMPTS_COLOR: Color32 = Color32::from_rgb(154, 161, 184);

pub(crate) fn comparison_plot(ui: &mut egui::Ui, data: &[ComparisonPoint], height: f32) {
    ui.label(
        egui::RichText::new(text("GAME MODE COMPARISON", "VERGLEICH DER SPIELMODI"))
            .size(TypeTokens::KBD)
            .color(C::TEXT_SECONDARY),
    );
    ui.horizontal(|ui| {
        for (label, color) in [
            (text("Attempts", "Versuche"), ATTEMPTS_COLOR),
            (
                if data.iter().any(|p| p.pvp) {
                    text("Clears / wins", "Clears / Siege")
                } else {
                    text("Cleared", "Gecleart")
                },
                C::GLOW,
            ),
        ] {
            let (mark, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().rect_filled(mark, 1, color);
            ui.small(label);
            ui.add_space(SpaceTokens::XS);
        }
    });
    ui.add_space(SpaceTokens::XS);
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let plot = Rect::from_min_max(rect.min + vec2(28.0, 16.0), rect.max - vec2(4.0, 25.0));
    let painter = ui.painter().with_clip_rect(rect);
    let font = FontId::proportional(10.0);
    let maximum = data
        .iter()
        .map(|d| d.attempts.max(d.cleared))
        .max()
        .unwrap_or(0)
        .max(1) as f64;
    // Familiar integer tick intervals, with a shared zero baseline for both series.
    let raw_step = (maximum / 4.0).max(1.0);
    let power = 10.0_f64.powf(raw_step.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .find(|s| *s * power >= raw_step)
        .unwrap()
        * power;
    let top = (maximum / step).ceil() * step;
    for tick in 0..=(top / step).round() as u32 {
        let value = tick as f64 * step;
        let y = plot.bottom() - plot.height() * (value / top) as f32;
        painter.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(0.5, C::BORDER),
        );
        painter.text(
            pos2(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            compact_count(value as u64),
            font.clone(),
            C::TEXT_SECONDARY,
        );
    }
    if data.is_empty() {
        painter.text(
            plot.center(),
            Align2::CENTER_CENTER,
            text("No mode data yet", "Noch keine Modusdaten"),
            font,
            C::TEXT_SECONDARY,
        );
        return;
    }
    let slot = plot.width() / data.len() as f32;
    let pointer = response.hover_pos().filter(|p| plot.contains(*p));
    let hovered =
        pointer.map(|p| (((p.x - plot.left()) / slot).floor() as usize).min(data.len() - 1));
    let bar_width = (slot * 0.24).min(20.0);
    let label_stride = (30.0 / slot).ceil().max(1.0) as usize;
    for (index, point) in data.iter().enumerate() {
        let x = plot.left() + slot * (index as f32 + 0.5);
        if hovered == Some(index) {
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(x - slot / 2.0, plot.top()),
                    pos2(x + slot / 2.0, plot.bottom()),
                ),
                0,
                Color32::from_rgba_unmultiplied(71, 227, 224, 10),
            );
        }
        for (offset, value, color) in [
            (-0.58, point.attempts, ATTEMPTS_COLOR),
            (0.58, point.cleared, C::GLOW),
        ] {
            let center = x + offset * bar_width;
            let y = plot.bottom() - plot.height() * (value as f64 / top) as f32;
            if value > 0 {
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(center - bar_width / 2.0, y),
                        pos2(center + bar_width / 2.0, plot.bottom()),
                    ),
                    1,
                    color,
                );
            } else {
                painter.line_segment(
                    [
                        pos2(center - bar_width / 2.0, y),
                        pos2(center + bar_width / 2.0, y),
                    ],
                    Stroke::new(1.5, color),
                );
            }
            if slot >= 58.0 {
                painter.text(
                    pos2(center, y - 4.0),
                    Align2::CENTER_BOTTOM,
                    compact_count(value),
                    font.clone(),
                    color,
                );
            }
        }
        if index % label_stride == 0 {
            let name = point
                .label
                .strip_suffix(text(" Stages", "-Stages"))
                .unwrap_or(&point.label);
            let galley = painter.layout_no_wrap(name.to_owned(), font.clone(), C::TEXT_SECONDARY);
            let name = if galley.size().x <= slot - 6.0 {
                name.to_owned()
            } else {
                (index + 1).to_string()
            };
            painter.text(
                pos2(x, plot.bottom() + 12.0),
                Align2::CENTER_CENTER,
                name,
                font.clone(),
                C::TEXT_SECONDARY,
            );
        }
    }
    if let Some(index) = hovered {
        let point = &data[index];
        response.show_tooltip_text(format_text!(
            "{}\nAttempts: {}\n{}: {}",
            "{}\nVersuche: {}\n{}: {}",
            point.label,
            point.attempts,
            if point.pvp {
                text("Wins", "Siege")
            } else {
                text("Stages cleared", "Stages gecleart")
            },
            point.cleared
        ));
    }
    ui.small(text(
        "Hover for the game mode and exact values",
        "Modus und genaue Werte per Mauszeiger",
    ));
}

fn compact_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 10_000 {
        format!("{:.0}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

pub fn statistics(ui: &mut egui::Ui, run: &RunActivity, available_height: f32) {
    ui.spacing_mut().item_spacing.x = SpaceTokens::SM;
    ui.heading(egui::RichText::new(text("Statistics", "Statistiken")).color(C::TEXT_PRIMARY));
    ui.add_space(SpaceTokens::SM);
    // Both charts share the remaining viewport budget; adding modes adds no rows.
    let chart_budget = (available_height - 390.0).max(144.0);
    let history_height = (chart_budget * 0.44).clamp(64.0, 130.0);
    let comparison_height = (chart_budget * 0.56).clamp(80.0, 174.0);
    card(ui, |ui| {
        let seconds = run.elapsed(Instant::now()).as_secs();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(text("ELAPSED TIME", "LAUFZEIT"))
                    .size(TypeTokens::KBD)
                    .color(C::TEXT_SECONDARY),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{:02}:{:02}:{:02}",
                        seconds / 3600,
                        seconds / 60 % 60,
                        seconds % 60
                    ))
                    .size(24.0)
                    .color(C::GLOW),
                );
            });
        });
        ui.small(text(
            "Current run including navigation",
            "Aktueller Lauf inkl. Navigation",
        ));
        ui.add_space(SpaceTokens::SM);
        stage_history(ui, run, history_height);
    });
    ui.add_space(SpaceTokens::MD);
    card(ui, |ui| {
        ui.columns(2, |columns| {
            columns[0].set_min_height(82.0);
            columns[1].set_min_height(82.0);
            let rate = run.success_rate().map(|r| format_text!("{r:.0}%", "{r:.0} %")).unwrap_or_else(|| "—".into());
            stat_value(&mut columns[0], text("SUCCESS RATE", "ERFOLGSQUOTE"), &rate);
            let (wins, losses) = run.outcomes();
            columns[0].small(format_text!("{wins} wins · {losses} losses", "{wins} Siege · {losses} Niederlagen"))
                .on_hover_text(text("Wins divided by battles with a confirmed result. Ongoing battles and unclear results are excluded.", "Siege / entschiedene Kämpfe. Laufende und unklare Kämpfe werden nicht eingerechnet."));
            let ratio = if run.is_pvp_run() { (wins>0).then(|| (wins+losses) as f64 / wins as f64) } else { run.attempts_per_clear() };
            let average = ratio.map(|a| format!("{a:.1}").replace('.', text(".", ","))).unwrap_or_else(|| "—".into());
            stat_value(&mut columns[1], if run.is_pvp_run() {text("AVG. ATTEMPTS / WIN", "Ø VERSUCHE / SIEG")} else {text("AVG. ATTEMPTS / STAGE", "Ø VERSUCHE / STAGE")}, &average);
            columns[1].small(if run.is_pvp_run() {text("Battles with confirmed results", "Nur entschiedene Kämpfe")} else {text("Cleared stages", "Abgeschlossene Stufen")})
                .on_hover_text(if run.is_pvp_run() {text("Battles with a confirmed result divided by wins.", "Entschiedene Kämpfe / bestätigte Siege. Laufende und unklare Kämpfe werden ausgeschlossen.")} else {text("Attempts through the last clear divided by stages cleared.", "Versuche bis zur letzten geschafften Stufe / geschaffte Stufen. Versuche an einer noch offenen Stufe werden ausgeschlossen.")});
        });
    });
    ui.add_space(SpaceTokens::MD);
    card(ui, |ui| mode_comparison(ui, run, comparison_height));
}

fn stat_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(TypeTokens::KBD)
            .color(C::TEXT_SECONDARY),
    );
    ui.add_space(SpaceTokens::SM);
    ui.label(
        egui::RichText::new(value)
            .size(TypeTokens::METRIC)
            .color(C::GLOW),
    );
}
