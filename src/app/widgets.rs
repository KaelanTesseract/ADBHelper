//! Small custom widgets for the title bar and navigation.

use super::*;

pub(super) fn nav_tab(ui: &mut egui::Ui, selected: bool, label: &str, width: f32) -> bool {
    let text = if selected {
        egui::RichText::new(label).color(ColorTokens::TEXT_PRIMARY)
    } else {
        egui::RichText::new(label).color(MUTED)
    };
    let light = ui.painter().add(egui::Shape::Noop);
    let response = ui.add_sized(
        egui::vec2(width, GeometryTokens::TITLEBAR_HEIGHT),
        egui::Button::new(text).frame(false),
    );
    if selected {
        ui.painter()
            .set(light, design_system::active_glow_mesh(response.rect));
        let y = response.rect.bottom() - 0.5;
        ui.painter().line_segment(
            [
                egui::pos2(response.rect.center().x - width * 0.28, y),
                egui::pos2(response.rect.center().x + width * 0.28, y),
            ],
            egui::Stroke::new(0.7, egui::Color32::from_rgb(40, 112, 122)),
        );
    }
    response.clicked()
}

pub(super) fn window_button(ui: &mut egui::Ui, kind: WindowButton) -> egui::Response {
    let response = ui.add_sized(
        egui::vec2(
            GeometryTokens::WINDOW_CONTROL_SIZE,
            GeometryTokens::WINDOW_CONTROL_SIZE,
        ),
        egui::Button::new("").frame(false),
    );
    let color = if matches!(kind, WindowButton::Close) && response.hovered() {
        ColorTokens::CLOSE_HOVER
    } else if response.hovered() {
        ColorTokens::TEXT_PRIMARY
    } else {
        MUTED
    };
    let center = response.rect.center();
    let painter = ui.painter();
    match kind {
        WindowButton::Minimize => {
            painter.line_segment(
                [
                    egui::pos2(center.x - 5.0, center.y + 2.0),
                    egui::pos2(center.x + 5.0, center.y + 2.0),
                ],
                egui::Stroke::new(1.4, color),
            );
        }
        WindowButton::Maximize { maximized: false } => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center, egui::vec2(10.0, 10.0)),
                0.5,
                egui::Stroke::new(1.2, color),
                egui::StrokeKind::Inside,
            );
        }
        WindowButton::Maximize { maximized: true } => {
            let back = egui::Rect::from_min_size(
                egui::pos2(center.x - 3.0, center.y - 5.0),
                egui::vec2(8.0, 8.0),
            );
            let front = egui::Rect::from_min_size(
                egui::pos2(center.x - 5.0, center.y - 2.0),
                egui::vec2(8.0, 8.0),
            );
            painter.rect_stroke(
                back,
                0.5,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(front, 0.5, ColorTokens::BG_PANEL);
            painter.rect_stroke(
                front,
                0.5,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
        }
        WindowButton::Close => {
            painter.line_segment(
                [
                    egui::pos2(center.x - 4.0, center.y - 4.0),
                    egui::pos2(center.x + 4.0, center.y + 4.0),
                ],
                egui::Stroke::new(1.4, color),
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + 4.0, center.y - 4.0),
                    egui::pos2(center.x - 4.0, center.y + 4.0),
                ],
                egui::Stroke::new(1.4, color),
            );
        }
    }
    response
}
