//! The overlay, a second window for when the game has the screen.
//!
//! It has no title bar, stays on top in a corner, is as tall as its rows and
//! never takes the focus, even when its hide button is clicked, so the game
//! keeps it. The hotkey and the tray drive it. It shows over VALORANT's
//! default borderless mode, and nothing can draw over exclusive fullscreen.

use egui::{Align2, Pos2, Rect, Sense, Ui, Vec2, ViewportBuilder, ViewportId, pos2, vec2};
use overseer_ui::{Face, caps_text, colour, space};
use serde::{Deserialize, Serialize};

/// How far the overlay sits from the edges of the screen.
const MARGIN: f32 = 16.0;
/// Room kept for the task bar, which is always on top too and would cover a
/// bottom corner on the desktop. Neither egui nor winit reports the work
/// area, so this is Windows 11's default bar height.
const TASKBAR: f32 = 48.0;
/// Cards of 620 points, room for every zone and a name, and a gutter each
/// side for the party tab.
pub(crate) const WIDTH: f32 = 620.0 + 2.0 * crate::board::GUTTER;
/// The overlay window's title, which is how it is found to keep it from
/// taking the focus.
pub(crate) const TITLE: &str = "Valorant Overseer Overlay";
/// Which corner of the screen the overlay sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Corner {
    /// Over the minimap.
    TopLeft,
    /// Above the scoreboard side of the screen, and where the overlay
    /// starts, out of the way of the agents in agent select.
    #[default]
    TopRight,
    /// Out of the way of everything the game draws at the top.
    BottomLeft,
    /// The quietest corner in VALORANT's own layout.
    BottomRight,
}

impl Corner {
    /// Every corner, for the settings screen.
    pub(crate) const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    /// What to call it on screen.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Top Left",
            Self::TopRight => "Top Right",
            Self::BottomLeft => "Bottom Left",
            Self::BottomRight => "Bottom Right",
        }
    }

    /// Why you might want it there.
    pub(crate) const fn about(self) -> &'static str {
        match self {
            Self::TopLeft => "Top left of the screen, beside the minimap",
            Self::TopRight => "Top right of the screen",
            Self::BottomLeft => "Bottom left of the screen",
            Self::BottomRight => "Bottom right of the screen",
        }
    }
}

/// Where a window of `size` sits in `corner`, clamped so it never starts off
/// the edge of a monitor smaller than it.
pub(crate) fn place(corner: Corner, monitor: Vec2, size: Vec2) -> Pos2 {
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

/// A little more than a full match needs, so a board that grows without
/// bound can never cover the game.
pub(crate) const CEILING: f32 = 700.0;

/// The greeting's plate, the height of the team plates on the board.
const PLATE: f32 = 32.0;

/// The greeting's height: its plate and a margin above and below.
pub(crate) const GREETING: f32 = PLATE + 2.0 * space::MD;

/// Draws the overlay in its own window, `height` tall, which is what it
/// last drew.
pub(crate) fn show(
    ctx: &egui::Context,
    corner: Corner,
    height: f32,
    mut board: impl FnMut(&mut Ui),
) {
    // Built against a guessed screen, it would have to move next frame.
    let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) else {
        ctx.request_repaint();
        return;
    };
    // As tall as the board, not a fixed size: this adapter presents opaque,
    // and the transparent part of a taller window would be a black slab over
    // the game.
    let size = vec2(WIDTH, height.clamp(GREETING, CEILING));
    let at = place(corner, monitor, size);
    ctx.show_viewport_immediate(
        id_for(corner, size.y),
        attributes(at, size),
        |ui, _class| {
            contents(ui, &mut board);
        },
    );
}

/// What the overlay's window holds: its ground, the board, and a hairline
/// round the edge so it reads as a panel over the game rather than a hole
/// in it.
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

/// What the overlay shows for a moment after it is switched on with no
/// match to show: a red plate where the enemy team will be, saying so, and
/// the key that hides it again. Returns the height it took.
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
    let paint = crate::board::paint::slant(plate, false, true, colour::ENEMY);
    painter.add(paint);
    let middle = plate.center().y;
    let said = caps_text(
        painter,
        pos2(plate.left() + space::LG, middle),
        Align2::LEFT_CENTER,
        "Overlay On",
        Face::Heavy.at(20.0),
        colour::BG,
    );
    let label = crate::board::paint::label();
    let _waiting = caps_text(
        painter,
        pos2(said.right() + space::LG, middle),
        Align2::LEFT_CENTER,
        "The enemy team shows here once a match loads",
        label.clone(),
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
        &format!("{} Hides It", crate::hotkey::LABEL),
        label,
        colour::BG,
    );
    GREETING
}

/// The overlay's hide button, in the top right corner of `rect` beside the
/// score. Returns whether it was clicked.
pub(crate) fn hide_button(ui: &Ui, rect: Rect) -> bool {
    let spot = Rect::from_center_size(
        pos2(
            rect.right() - crate::board::GUTTER - 11.0,
            rect.top() + crate::board::GUTTER + crate::board::STRIP_MIDDLE,
        ),
        vec2(22.0, 20.0),
    );
    let response = ui
        .interact(spot, ui.id().with("overlay-hide"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "Hide the overlay. {} brings it back.",
            crate::hotkey::LABEL
        ));
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

/// The window's identity. It carries the corner because a corner only takes
/// effect when the window is built. A window moved after it is shown stops
/// being composited on this machine, though it still says it is visible,
/// so a bottom corner, which is placed from the window's `height`, carries
/// that too and a new height there builds a new window.
fn id_for(corner: Corner, height: f32) -> ViewportId {
    let bottom = matches!(corner, Corner::BottomLeft | Corner::BottomRight);
    let placed_by = bottom.then(|| height.to_bits());
    ViewportId::from_hash_of(("overseer-overlay", corner.label(), placed_by))
}

/// The overlay window, all decided before it exists.
fn attributes(at: Pos2, size: Vec2) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(TITLE)
        .with_position(at)
        .with_inner_size(size)
        .with_decorations(false)
        .with_always_on_top()
        .with_transparent(true)
        // Never the keyboard when it appears. `never_activate` stops a click
        // from taking it after that.
        .with_active(false)
        // One task bar button for the app, not two.
        .with_taskbar(false)
        .with_resizable(false)
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::{Corner, DESIGNED, MARGIN, TASKBAR, place};

    /// Changing corner has to build a new window, because a shown window that
    /// moves stops being drawn, and so does a new height in a bottom corner.
    /// A top corner only grows down, so its window just resizes.
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

    /// Every corner puts the window fully on screen, in that corner.
    #[test]
    fn every_corner_is_the_corner_it_says() {
        let monitor = vec2(1920.0, 1080.0);
        let size = vec2(460.0, 300.0);
        for corner in Corner::ALL {
            let at = place(corner, monitor, size);
            assert!(at.x >= MARGIN, "{corner:?} started off the left");
            assert!(at.y >= MARGIN, "{corner:?} started above the top");
            assert!(
                at.x + size.x <= monitor.x - MARGIN + 0.01,
                "{corner:?} ran off the right"
            );
            assert!(
                at.y + size.y <= monitor.y - MARGIN - TASKBAR + 0.01,
                "{corner:?} ran into the task bar"
            );
        }
        assert!(place(Corner::TopRight, monitor, size).x > monitor.x / 2.0);
        assert!(place(Corner::BottomLeft, monitor, size).y > monitor.y / 2.0);
    }

    /// A screen smaller than the overlay still puts it on the screen.
    #[test]
    fn a_tiny_screen_does_not_push_it_off_the_side() {
        let at = place(Corner::BottomRight, vec2(300.0, 200.0), vec2(460.0, 400.0));
        assert!((at.x - MARGIN).abs() < f32::EPSILON, "pushed off the left");
        assert!((at.y - MARGIN).abs() < f32::EPSILON, "pushed off the top");
    }
}
