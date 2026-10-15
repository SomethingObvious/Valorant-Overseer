//! The title bar's own buttons, for the app and the setup wizard, which both
//! draw their own title bar in place of Windows'.

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2};

use crate::{colour, space};

/// A button at the right end of a title bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Open or close the settings.
    Settings,
    /// Open or close your match history.
    History,
    /// Open or close your lineups.
    Lineups,
    /// Into the task bar.
    Minimize,
    /// Fill the screen, or come back from filling it.
    Maximize,
    /// Close the window.
    Close,
}

/// How wide each button is.
pub const WIDTH: f32 = 44.0;

/// How the buttons are drawn this frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct State {
    /// Whether the window fills the screen, which turns maximize into restore.
    pub maximized: bool,
    /// The screen whose button is lit, because it is open.
    pub open: Option<Button>,
}

/// Draws `order` right to left from the bar's right edge, each the bar's
/// full height, with a rule before the settings. Returns where they start and
/// which was clicked.
#[must_use]
pub fn buttons(ui: &Ui, rect: Rect, order: &[Button], state: State) -> (f32, Option<Button>) {
    let painter = ui.painter();
    let mut x = rect.right();
    let mut clicked = None;
    for &button in order {
        if button == Button::Settings {
            x -= space::MD;
            painter.vline(
                x,
                (rect.top() + space::LG)..=(rect.bottom() - space::LG),
                (1.0, colour::LINE),
            );
            x -= space::MD;
        }
        x -= WIDTH;
        let area = Rect::from_min_size(pos2(x, rect.top()), vec2(WIDTH, rect.height()));
        let hit = ui.interact(area, ui.id().with(("chrome", button as u8)), Sense::click());
        let hot = hit.hovered();
        let open = state.open == Some(button);
        if hot {
            painter.rect_filled(
                area,
                0,
                if button == Button::Close {
                    colour::ENEMY
                } else {
                    colour::BG_HOVER
                },
            );
        }
        let ink = if hot || open {
            colour::TEXT_STRONG
        } else {
            colour::TEXT_DIM
        };
        icon(painter, button, area.center(), ink, state.maximized);
        if open {
            painter.hline(
                (area.left() + space::MD)..=(area.right() - space::MD),
                area.bottom() - 2.0,
                (2.0, colour::TEXT_STRONG),
            );
        }
        let tip = match button {
            Button::Settings => "Settings",
            Button::History => "History",
            Button::Lineups => "Lineups",
            Button::Minimize => "Minimize",
            Button::Maximize if state.maximized => "Restore",
            Button::Maximize => "Maximize",
            Button::Close => "Close",
        };
        hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
        if hit.on_hover_text(tip).clicked() {
            clicked = Some(button);
        }
    }
    (x, clicked)
}

/// A button's glyph, drawn in lines ten points across so they all match.
fn icon(painter: &egui::Painter, button: Button, c: Pos2, ink: Color32, maximized: bool) {
    let stroke = Stroke::new(1.2, ink);
    match button {
        Button::Minimize => {
            painter.hline((c.x - 5.0)..=(c.x + 5.0), c.y, stroke);
        }
        Button::Maximize if maximized => {
            // Two windows, the one behind showing only its top and right.
            let front = Rect::from_center_size(pos2(c.x - 1.0, c.y + 1.0), vec2(8.0, 8.0));
            painter.rect_stroke(front, 0, stroke, egui::StrokeKind::Middle);
            painter.add(Shape::line(
                vec![
                    pos2(front.left() + 2.0, front.top() - 2.0),
                    pos2(front.right() + 2.0, front.top() - 2.0),
                    pos2(front.right() + 2.0, front.bottom() - 2.0),
                ],
                stroke,
            ));
        }
        Button::Maximize => {
            let window = Rect::from_center_size(c, vec2(10.0, 10.0));
            painter.rect_stroke(window, 0, stroke, egui::StrokeKind::Middle);
        }
        Button::Close => {
            painter.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], stroke);
            painter.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], stroke);
        }
        Button::Settings => {
            // A gear: eight flat teeth round a ring, and the hole.
            let teeth = 8;
            let half = std::f32::consts::PI / teeth as f32;
            let mut rim = Vec::with_capacity(teeth * 4);
            for k in 0..teeth {
                let at = k as f32 * 2.0 * half;
                for (radius, turn) in [(5.0, -0.62_f32), (7.0, -0.36), (7.0, 0.36), (5.0, 0.62)] {
                    let angle = turn.mul_add(half, at);
                    rim.push(c + radius * vec2(angle.cos(), angle.sin()));
                }
            }
            painter.add(Shape::closed_line(rim, stroke));
            painter.circle_stroke(c, 2.2, stroke);
        }
        Button::Lineups => {
            // A sight: a ring and four ticks pointing in.
            painter.circle_stroke(c, 5.0, stroke);
            for (from, to) in [
                (vec2(0.0, -8.0), vec2(0.0, -3.0)),
                (vec2(0.0, 8.0), vec2(0.0, 3.0)),
                (vec2(-8.0, 0.0), vec2(-3.0, 0.0)),
                (vec2(8.0, 0.0), vec2(3.0, 0.0)),
            ] {
                painter.line_segment([c + from, c + to], stroke);
            }
        }
        Button::History => {
            // A clock at ten past twelve.
            painter.circle_stroke(c, 6.5, stroke);
            painter.line_segment([c, c + vec2(0.0, -4.0)], stroke);
            painter.line_segment([c, c + vec2(3.0, 0.0)], stroke);
        }
    }
}

/// What a press on the bar itself does: a drag moves the window, and a double
/// click asks to fill the screen. True when it asked.
#[must_use]
pub fn drag(ui: &Ui, bar: &Response) -> bool {
    if bar.double_clicked() {
        return true;
    }
    if bar.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    false
}

/// A hairline round the window, which has no frame from Windows, unless it
/// fills the screen.
pub fn outline(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let rect = ctx.viewport_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("window-frame"),
    ))
    .rect_stroke(rect, 0, (1.0, colour::LINE), egui::StrokeKind::Inside);
}
