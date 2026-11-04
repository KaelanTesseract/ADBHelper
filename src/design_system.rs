//! Graphit foundation, visually refined against the user's Meely reference.
//! Active light is contained within surfaces, with a thin turquoise edge.

use eframe::egui;

pub struct ColorTokens;
impl ColorTokens {
    pub const BG_VIEWPORT: egui::Color32 = egui::Color32::from_rgb(17, 17, 21);
    pub const BG_PANEL: egui::Color32 = egui::Color32::from_rgb(20, 20, 25);
    pub const BG_SECTION: egui::Color32 = egui::Color32::from_rgb(23, 23, 28);
    pub const BG_FIELD: egui::Color32 = egui::Color32::from_rgb(29, 29, 35);
    pub const BG_INSET: egui::Color32 = egui::Color32::from_rgb(13, 13, 13);
    pub const BG_ACTIVE: egui::Color32 = egui::Color32::from_rgb(20, 35, 40);
    pub const BORDER: egui::Color32 = egui::Color32::from_rgb(42, 42, 49);
    pub const VIEWPORT_GRID: egui::Color32 = egui::Color32::from_rgb(51, 51, 51);
    pub const GROUND_AXIS: egui::Color32 = egui::Color32::from_rgb(56, 115, 59);
    pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(236, 236, 241);
    pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(165, 165, 176);
    pub const TEXT_TERTIARY: egui::Color32 = egui::Color32::from_rgb(125, 125, 125);
    pub const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(115, 115, 115);
    pub const TEXT_DISABLED: egui::Color32 = egui::Color32::from_rgb(142, 142, 142);
    pub const ACCENT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0, 109, 253);
    pub const FOCUS_RING: egui::Color32 = egui::Color32::from_rgb(0, 109, 253);
    pub const SELECTION: egui::Color32 = egui::Color32::from_rgb(241, 240, 92);
    pub const AXIS_X: egui::Color32 = egui::Color32::from_rgb(224, 48, 58);
    pub const AXIS_Y: egui::Color32 = egui::Color32::from_rgb(100, 190, 98);
    pub const AXIS_Z: egui::Color32 = egui::Color32::from_rgb(61, 139, 219);
    pub const GLOW: egui::Color32 = egui::Color32::from_rgb(71, 227, 224);
    pub const CLOSE_HOVER: egui::Color32 = egui::Color32::from_rgb(172, 61, 69);
    pub const DANGER: egui::Color32 = egui::Color32::from_rgb(142, 43, 50);
    pub const DANGER_HOVER: egui::Color32 = egui::Color32::from_rgb(174, 52, 61);
}

pub struct SpaceTokens;
impl SpaceTokens {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
    pub const GAP: f32 = 10.0;
    pub const TIGHT: f32 = 6.0;
    pub const NAV_BRAND: f32 = 24.0;
}

pub struct RadiusTokens;
impl RadiusTokens {
    pub const XS: u8 = 1;
    pub const SM: u8 = 3;
    pub const MD: u8 = 3;
    pub const LG: u8 = 2;
    pub const PILL: u8 = 99;
}

pub struct TypeTokens;
impl TypeTokens {
    pub const HEADING: f32 = 20.0;
    pub const LABEL: f32 = 13.0;
    pub const BODY: f32 = 13.0;
    pub const VALUE: f32 = 11.0;
    pub const KBD: f32 = 10.0;
    pub const AXIS: f32 = 15.0;
    pub const BRAND: f32 = 20.0;
    pub const METRIC: f32 = 30.0;
}

pub struct SizeTokens;
impl SizeTokens {
    pub const ROW: f32 = 24.0;
    pub const FIELD: f32 = 26.0;
    pub const CONTROL: f32 = 32.0;
    pub const HEADER: f32 = 40.0;
    pub const TOPBAR: f32 = 48.0;
    pub const DOCK: f32 = 56.0;
    pub const PANEL_WIDTH: f32 = 250.0;
}

pub struct GeometryTokens;
impl GeometryTokens {
    pub const APP_INSET: i8 = SpaceTokens::LG as i8;
    pub const TITLEBAR_X: i8 = 4;
    pub const TITLEBAR_Y: i8 = 2;
    pub const TITLEBAR_HEIGHT: f32 = SizeTokens::TOPBAR;
    pub const NAV_DASHBOARD_WIDTH: f32 = 94.0;
    pub const NAV_SETTINGS_WIDTH: f32 = 116.0;
    pub const WINDOW_CONTROL_SIZE: f32 = 34.0;
    pub const BRAND_MARK_WIDTH: f32 = 34.0;
    pub const BRAND_MARK_HEIGHT: f32 = 34.0;
}

pub fn apply_theme(context: &egui::Context) {
    context.set_visuals(egui::Visuals::dark());
    context.style_mut_of(egui::Theme::Dark, |style| {
        let visuals = &mut style.visuals;
        visuals.panel_fill = ColorTokens::BG_VIEWPORT;
        visuals.window_fill = ColorTokens::BG_PANEL;
        visuals.window_stroke = egui::Stroke::new(1.0, ColorTokens::BORDER);
        visuals.window_corner_radius = egui::CornerRadius::same(RadiusTokens::LG);
        visuals.menu_corner_radius = egui::CornerRadius::same(RadiusTokens::LG);
        visuals.window_shadow = egui::Shadow::NONE;
        visuals.popup_shadow = egui::Shadow::NONE;
        visuals.extreme_bg_color = ColorTokens::BG_INSET;
        visuals.faint_bg_color = ColorTokens::BG_SECTION;
        visuals.selection.bg_fill = ColorTokens::BG_ACTIVE;
        visuals.selection.stroke = egui::Stroke::new(1.0, ColorTokens::TEXT_PRIMARY);
        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = ColorTokens::BG_PANEL;
        widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, ColorTokens::BORDER);
        widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, ColorTokens::TEXT_SECONDARY);
        widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
        widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        widgets.inactive.bg_stroke = egui::Stroke::NONE;
        widgets.inactive.fg_stroke = egui::Stroke::new(1.5, ColorTokens::TEXT_SECONDARY);
        widgets.hovered.bg_fill = ColorTokens::BG_FIELD;
        widgets.hovered.weak_bg_fill = ColorTokens::BG_FIELD;
        widgets.hovered.bg_stroke = egui::Stroke::NONE;
        widgets.hovered.fg_stroke = egui::Stroke::new(1.5, ColorTokens::TEXT_PRIMARY);
        widgets.active.bg_fill = ColorTokens::BG_ACTIVE;
        widgets.active.weak_bg_fill = ColorTokens::BG_ACTIVE;
        widgets.active.bg_stroke = egui::Stroke::NONE;
        widgets.active.fg_stroke = egui::Stroke::new(1.5, ColorTokens::TEXT_PRIMARY);
        widgets.open = widgets.active;
        for visual in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            visual.corner_radius = egui::CornerRadius::same(RadiusTokens::SM);
            visual.expansion = 0.0;
        }
        style.spacing.item_spacing = egui::vec2(SpaceTokens::SM, SpaceTokens::XS);
        style.spacing.button_padding = egui::vec2(SpaceTokens::SM, SpaceTokens::XS);
        style.spacing.interact_size = egui::vec2(SizeTokens::CONTROL, SizeTokens::ROW);
        style.spacing.window_margin = egui::Margin::same(SpaceTokens::MD as i8);
        style.spacing.menu_margin = egui::Margin::same(SpaceTokens::LG as i8);
        style.text_styles = [
            (
                egui::TextStyle::Heading,
                egui::FontId::proportional(TypeTokens::HEADING),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::proportional(TypeTokens::BODY),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::proportional(TypeTokens::BODY),
            ),
            (
                egui::TextStyle::Small,
                egui::FontId::proportional(TypeTokens::KBD),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::monospace(TypeTokens::VALUE),
            ),
        ]
        .into();
    });
}

pub fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    state_card(ui, false, add_contents);
}

pub fn active_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    state_card(ui, true, add_contents);
}

pub fn state_card(ui: &mut egui::Ui, active: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    let mut prepared = panel_frame().begin(ui);
    prepared
        .content_ui
        .set_min_width(prepared.content_ui.available_width());
    let light = prepared.content_ui.painter().add(egui::Shape::Noop);
    prepared
        .content_ui
        .with_layout(egui::Layout::top_down(egui::Align::Min), add_contents);
    let rect = prepared.frame.widget_rect(prepared.content_ui.min_rect());
    if active {
        prepared
            .content_ui
            .painter()
            .set(light, active_glow_mesh(rect.shrink(1.0)));
    }
    prepared.end(ui);
    if active {
        paint_active_edge(ui, rect, RadiusTokens::LG);
    }
}

pub fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(ColorTokens::BG_PANEL)
        .stroke(egui::Stroke::new(1.0, ColorTokens::BORDER))
        .corner_radius(egui::CornerRadius::same(RadiusTokens::LG))
        .inner_margin(egui::Margin::same(SpaceTokens::MD as i8))
}

pub fn primary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).color(ColorTokens::TEXT_PRIMARY))
        .fill(ColorTokens::BG_FIELD)
        .stroke(egui::Stroke::new(1.0, ColorTokens::BORDER))
}

pub fn stop_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).color(ColorTokens::TEXT_PRIMARY))
        .fill(ColorTokens::DANGER)
        .stroke(egui::Stroke::new(1.0, ColorTokens::DANGER_HOVER))
}

pub fn status_badge(ui: &mut egui::Ui, text: &str, active: bool) {
    let color = if active {
        ColorTokens::GLOW
    } else {
        ColorTokens::TEXT_SECONDARY
    };
    let label = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(TypeTokens::KBD),
        color,
    );
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(label.size().x + 16.0, 22.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(
        rect,
        RadiusTokens::XS,
        if active {
            ColorTokens::BG_ACTIVE
        } else {
            ColorTokens::BG_SECTION
        },
    );
    if active {
        paint_active_glow(ui, rect, RadiusTokens::XS);
    }
    ui.painter().galley(
        egui::pos2(rect.left() + 8.0, rect.center().y - label.size().y / 2.0),
        label,
        color,
    );
}

pub fn active_checkbox(ui: &mut egui::Ui, checked: &mut bool, text: &str) -> egui::Response {
    let response = ui.checkbox(checked, text);
    if *checked {
        let icon = egui::Rect::from_center_size(
            egui::pos2(response.rect.left() + 8.0, response.rect.center().y),
            egui::vec2(16.0, 16.0),
        );
        paint_active_glow(ui, icon, RadiusTokens::XS);
    }
    response
}

pub fn paint_active_glow(ui: &egui::Ui, rect: egui::Rect, radius: u8) {
    ui.painter().add(active_glow_mesh(rect.shrink(1.0)));
    paint_active_edge(ui, rect, radius);
}

fn paint_active_edge(ui: &egui::Ui, rect: egui::Rect, radius: u8) {
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(0.7, egui::Color32::from_rgb(35, 95, 105)),
        egui::StrokeKind::Inside,
    );
}

/// Vertex-alpha light falloff: a broad soft pool inside the bottom edge, no outer ring.
pub fn active_glow_mesh(rect: egui::Rect) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let columns = 32_u32;
    let rows = 16_u32;
    for y in 0..=rows {
        for x in 0..=columns {
            let u = x as f32 / columns as f32;
            let v = y as f32 / rows as f32;
            let across = (-((u - 0.45) / 0.40).powi(2) * 2.0).exp();
            let above = (-((1.0 - v) * rect.height() / 40.0).powi(2) * 1.5).exp();
            let alpha = (across * above * 30.0).round() as u8;
            mesh.colored_vertex(
                egui::pos2(
                    rect.left() + u * rect.width(),
                    rect.top() + v * rect.height(),
                ),
                egui::Color32::from_rgba_unmultiplied(30, 200, 214, alpha),
            );
        }
    }
    for y in 0..rows {
        for x in 0..columns {
            let a = y * (columns + 1) + x;
            mesh.add_triangle(a, a + 1, a + columns + 1);
            mesh.add_triangle(a + 1, a + columns + 2, a + columns + 1);
        }
    }
    mesh
}
