use crate::activity::RunActivity;
use crate::design_system::{ColorTokens as C, SpaceTokens, TypeTokens};
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
pub fn stage_history(ui: &mut egui::Ui, run: &RunActivity) {
    ui.label(
        egui::RichText::new("STAGE-VERLAUF")
            .size(TypeTokens::KBD)
            .color(C::TEXT_SECONDARY),
    );
    ui.small("Geschaffte Stages über die Laufzeit");
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 130.0), Sense::hover());
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
            "Verlauf beginnt mit dem Lauf",
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
            response.on_hover_text(format!("{} · {} Stages gecleart", clock(time), value));
        }
    }
}

/// Common scale and visible numbers make comparisons meaningful across modes.
pub fn mode_comparison(ui: &mut egui::Ui, run: &RunActivity) {
    ui.label(
        egui::RichText::new("VERGLEICH DER SPIELMODI")
            .size(TypeTokens::KBD)
            .color(C::TEXT_SECONDARY),
    );
    ui.small("Versuche und geschaffte Stages");
    ui.add_space(SpaceTokens::SM);
    if run.modes.is_empty() {
        ui.colored_label(C::TEXT_SECONDARY, "Noch keine Modusdaten");
        return;
    }
    let maximum = run
        .modes
        .values()
        .map(|s| s.attempts.max(s.cleared))
        .max()
        .unwrap_or(0)
        .max(1);
    for (mode, stats) in &run.modes {
        ui.label(egui::RichText::new(mode.label()).color(C::TEXT_PRIMARY));
        for (label, value, color) in [
            ("Versuche", stats.attempts, Color32::from_rgb(126, 133, 157)),
            ("Gecleart", stats.cleared, C::GLOW),
        ] {
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 21.0), Sense::hover());
            let bar = Rect::from_min_max(
                pos2(rect.left() + 62.0, rect.center().y - 3.0),
                pos2(rect.right() - 30.0, rect.center().y + 3.0),
            );
            ui.painter().text(
                rect.left_center(),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(10.0),
                C::TEXT_SECONDARY,
            );
            ui.painter().rect_filled(bar, 1, C::BG_FIELD);
            if value > 0 {
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        bar.min,
                        vec2(bar.width() * value as f32 / maximum as f32, bar.height()),
                    ),
                    1,
                    color,
                );
            }
            ui.painter().text(
                rect.right_center(),
                Align2::RIGHT_CENTER,
                value.to_string(),
                FontId::proportional(11.0),
                C::TEXT_PRIMARY,
            );
            response.on_hover_text(format!("{} · {label}: {value}", mode.label()));
        }
        ui.add_space(SpaceTokens::SM);
    }
}
