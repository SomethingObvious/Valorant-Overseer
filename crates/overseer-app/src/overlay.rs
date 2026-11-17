//! The overlay, a second window for when the game has the screen.
//!
//! It has no title bar, stays on top in a corner, is as tall as its rows and
//! never takes the focus, even when its hide button is clicked, so the game
//! keeps it. The hotkey and the tray drive it. It shows over VALORANT's
//! default borderless mode, and nothing can draw over exclusive fullscreen.

use egui::{Align2, Pos2, Rect, Sense, Ui, Vec2, ViewportBuilder, ViewportId, pos2, vec2};
use overseer_ui::{Face, caps_text, colour, space};
use serde::{Deserialize, Serialize};

const MARGIN: f32 = 16.0;
/// Room kept for the task bar, which is always on top too and would cover a
/// bottom corner on the desktop. Neither egui nor winit reports the work
/// area, so this is Windows 11's default bar height.
const TASKBAR: f32 = 48.0;
/// Cards of 620 points, room for every zone and a name, and a gutter each
/// side for the party tab.
pub(crate) const WIDTH: f32 = 620.0 + 2.0 * crate::board::GUTTER;
pub(crate) const TITLE: &str = "Valorant Overseer Overlay";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Corner {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    /// The quietest corner in VALORANT's own layout.
    BottomRight,
}

impl Corner {
    pub(crate) const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Top Left",
            Self::TopRight => "Top Right",
            Self::BottomLeft => "Bottom Left",
            Self::BottomRight => "Bottom Right",
        }
    }

    pub(crate) const fn about(self) -> &'static str {
        match self {
            Self::TopLeft => "Top left of the screen, beside the minimap",
            Self::TopRight => "Top right of the screen",
            Self::BottomLeft => "Bottom left of the screen",
            Self::BottomRight => "Bottom right of the screen",
        }
    }
}

fn place(corner: Corner, monitor: Vec2, size: Vec2) -> Pos2 {
    let right = (monitor.x - size.x - MARGIN).max(MARGIN);
    let bottom = (monitor.y - size.y - MARGIN - TASKBAR).max(MARGIN);
    match corner {
        Corner::TopLeft => pos2(MARGIN, MARGIN),
        Corner::TopRight => pos2(right, MARGIN),
        Corner::BottomLeft => pos2(MARGIN, bottom),
        Corner::BottomRight => pos2(right, bottom),
    }
}

/// The height of the match strip, a plate and five cards, which the overlay
/// is built for before it has measured what it draws.
pub(crate) const DESIGNED: f32 = crate::board::OVERLAY_HEIGHT;

pub(crate) const CEILING: f32 = 700.0;

pub(crate) fn tallest(monitor: f32) -> f32 {
    (monitor * 0.8).max(CEILING)
}

const PLATE: f32 = 32.0;

pub(crate) const GREETING: f32 = PLATE + 2.0 * space::MD;

pub(crate) fn show(
    ctx: &egui::Context,
    corner: Corner,
    height: f32,
    mut board: impl FnMut(&mut Ui),
) {
    let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) else {
        ctx.request_repaint();
        return;
    };
    let size = vec2(WIDTH, height.clamp(GREETING, tallest(monitor.y)));
    let at = place(corner, monitor, size);
    ctx.show_viewport_immediate(
        id_for(corner, size.y),
        attributes(at, size),
        |ui, _class| {
            contents(ui, &mut board);
        },
    );
}

pub(crate) fn contents(ui: &mut Ui, board: &mut impl FnMut(&mut Ui)) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(colour::BG))
        .show(ui, |ui| board(ui));
    ui.painter().rect_stroke(
        ui.max_rect(),
        0,
        egui::Stroke::new(1.0, colour::LINE),
        egui::StrokeKind::Inside,
    );
}

pub(crate) fn greeting(ui: &mut Ui) -> f32 {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), GREETING), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return GREETING;
    }
    let painter = ui.painter();
    let plate = Rect::from_min_max(
        pos2(rect.left() + crate::board::GUTTER, rect.top() + space::MD),
        pos2(
            rect.right() - crate::board::GUTTER,
            rect.bottom() - space::MD,
        ),
    );
    let paint = crate::board::paint::slant(plate, false, true, colour::TEXT_STRONG);
    painter.add(paint);
    let middle = plate.center().y;
    let said = caps_text(
        painter,
        pos2(plate.left() + space::LG, middle),
        Align2::LEFT_CENTER,
        "Overlay On",
        Face::Heavy.at(overseer_ui::size::DISPLAY),
        colour::BG,
    );
    let label = crate::board::paint::label();
    let _waiting = painter.text(
        pos2(said.right() + space::LG, middle),
        Align2::LEFT_CENTER,
        "The enemy team shows here once a match loads.",
        Face::Body.at(label.size + 1.0),
        colour::BG,
    );
    let _hide = caps_text(
        painter,
        pos2(
            plate
                .height()
                .mul_add(-crate::board::paint::LEAN, plate.right())
                - space::LG,
            middle,
        ),
        Align2::RIGHT_CENTER,
        &format!("{} Hides It", crate::hotkey::label()),
        label,
        colour::BG,
    );
    GREETING
}

pub(crate) fn hide_button(ui: &Ui, rect: Rect) -> bool {
    let spot = Rect::from_center_size(
        pos2(
            rect.right() - crate::board::GUTTER - 11.0,
            rect.top() + crate::board::GUTTER + crate::board::STRIP_MIDDLE,
        ),
        vec2(24.0, 24.0),
    );
    let response = ui
        .interact(spot, ui.id().with("overlay-hide"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "Hide the overlay. {} brings it back.",
            crate::hotkey::label()
        ));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Hide the overlay")
    });
    if ui.is_rect_visible(spot) {
        let painter = ui.painter();
        let (fill, ink) = if response.hovered() {
            (colour::BG_HOVER, colour::TEXT_STRONG)
        } else {
            (colour::BG_INSET, colour::TEXT)
        };
        painter.add(crate::board::paint::slant(spot, true, true, fill));
        let bar = Rect::from_center_size(spot.center() + vec2(0.0, 3.0), vec2(9.0, 2.0));
        painter.rect_filled(bar, 0, ink);
    }
    response.clicked()
}

fn id_for(corner: Corner, height: f32) -> ViewportId {
    let bottom = matches!(corner, Corner::BottomLeft | Corner::BottomRight);
    let placed_by = bottom.then(|| height.to_bits());
    ViewportId::from_hash_of(("overseer-overlay", corner.label(), placed_by))
}

fn attributes(at: Pos2, size: Vec2) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(TITLE)
        .with_position(at)
        .with_inner_size(size)
        .with_decorations(false)
        .with_always_on_top()
        .with_transparent(true)
        .with_active(false)
        .with_taskbar(false)
        .with_resizable(false)
}

#[cfg(test)]
mod tests {

    use super::{Corner, DESIGNED};

    #[test]
    fn a_corner_is_part_of_the_windows_identity() {
        let mut seen = std::collections::HashSet::new();
        for corner in Corner::ALL {
            assert!(
                seen.insert(super::id_for(corner, DESIGNED)),
                "{corner:?} shares a window with another corner"
            );
        }
        let taller = DESIGNED + 200.0;
        assert_ne!(
            super::id_for(Corner::BottomRight, DESIGNED),
            super::id_for(Corner::BottomRight, taller)
        );
        assert_eq!(
            super::id_for(Corner::TopRight, DESIGNED),
            super::id_for(Corner::TopRight, taller)
        );
    }
}
