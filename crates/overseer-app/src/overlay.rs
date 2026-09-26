//! Overlay mode: a second window, for when the game has the screen.
//!
//! In a match the board is not something you alt-tab to. It is something you
//! glance at, in a corner. So this window has no title bar, sits above
//! everything else, is exactly as tall as the rows it holds, parks in a
//! corner of your choosing and takes no mouse input at all.
//!
//! It is a window of its own rather than the board's window wearing a
//! different shape, which leaves the board where it was: on a second monitor
//! you get both, and on one monitor you close the board to the tray and keep
//! the overlay. Everything about the overlay is also decided before the
//! window exists, which is worth a great deal on this machine.
//!
//! Clicks pass straight through, which is not a limitation but the point: an
//! overlay you can click is an overlay that takes the focus off the game
//! mid-round. Everything is therefore driven from outside it, by the hotkey
//! and the tray.
//!
//! It sits over a game running borderless, which is how VALORANT runs by
//! default. Nothing can sit over true exclusive fullscreen, and nothing here
//! tries: the app reads Riot's own local API and draws a window, and that is
//! the whole of its relationship with the game.

use egui::{Pos2, Ui, Vec2, ViewportBuilder, ViewportId, pos2, vec2};
use serde::{Deserialize, Serialize};

use overseer_ui::{colour, space};

/// How far the overlay sits from the edges of the screen.
const MARGIN: f32 = 16.0;
/// How much of the bottom of the screen to treat as spoken for.
///
/// The task bar is always on top too, and between two windows that both
/// claim that, the one activated most recently wins: an overlay in a bottom
/// corner is behind it on the desktop and looks like it failed to open. The
/// work area is what should be asked for here, and neither egui nor winit
/// will say, so this is Windows 11's default bar height. Over a game running
/// borderless there is no bar and the gap is a gap; on the desktop it is the
/// difference between visible and not.
const TASKBAR: f32 = 48.0;
/// How wide the overlay is.
///
/// Enough for the columns that matter at a glance and no more: an overlay
/// as wide as the board is a board. At this width the priority list keeps
/// the agent, the name, the rank, the K/D and the last five results, which
/// is the whole of what can be read in the seconds this is looked at.
pub(crate) const WIDTH: f32 = 540.0;
/// What to assume the screen is, on the one frame of a window's life before
/// anybody has said. Only ever wrong for a moment: the frame after it, the
/// real size arrives and the overlay is placed again.
const FALLBACK_SCREEN: Vec2 = vec2(1920.0, 1080.0);
/// Everything in the overlay that is not a player row: two team bands,
/// two column heading rows, the air above the first and the gap between
/// the blocks. Measured from the same tokens the board draws with, so a
/// band that grows cannot leave the overlay clipping its own last row.
const CHROME: f32 = space::MD                       // the air above the first band
    + (space::ROW + space::MD) * 2.0                // two team bands
    + space::XL * 2.0                               // two rows of column headings
    + space::XL * 2.0; // the gap after each block

/// Which corner of the screen the overlay lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Corner {
    /// Above the minimap, which is where your eyes already go.
    #[default]
    TopLeft,
    /// Above the scoreboard side of the screen.
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
            Self::TopLeft => "top left",
            Self::TopRight => "top right",
            Self::BottomLeft => "bottom left",
            Self::BottomRight => "bottom right",
        }
    }

    /// Why you might want it there.
    pub(crate) const fn about(self) -> &'static str {
        match self {
            Self::TopLeft => "Beside the minimap, where your eyes already are",
            Self::TopRight => "Under the round timer, out of the middle",
            Self::BottomLeft => "Clear of everything the game draws up top",
            Self::BottomRight => "The quietest corner of the game's own layout",
        }
    }
}

/// Where a window of this size sits, in this corner of this screen.
///
/// Clamped so a monitor smaller than the overlay still puts it somewhere on
/// the screen rather than off the side of it.
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

/// How tall to make the overlay before anything has been drawn in it.
///
/// A guess, and only a guess: the window is resized to whatever the board
/// actually drew as soon as it has drawn once. Adding the pieces up by hand
/// is how this clipped its own last player twice, because the sum has to be
/// revisited every time a band grows a point, and nobody revisits a sum.
///
/// No rows at all is a single line saying so. The full sized empty state
/// belongs in a window somebody is looking at; over a game it would be a
/// large dark rectangle announcing that it has nothing to tell you.
pub(crate) fn size_for(rows: usize) -> Vec2 {
    if rows == 0 {
        return vec2(WIDTH, space::ROW + space::MD);
    }
    let body = space::ROW_TIGHT * rows as f32;
    vec2(WIDTH, body + CHROME)
}

/// The tallest the overlay is allowed to get, whatever it measures.
///
/// A lobby is ten rows and this is far more than ten rows need. It exists
/// so that a board that somehow grows without bound cannot turn the overlay
/// into a full screen window over somebody's game.
pub(crate) const CEILING: f32 = 620.0;

/// Draws the overlay, as a window of its own.
///
/// A second window rather than the main one wearing a different hat, and
/// that is not a preference. A window moved after it has been shown stops
/// being composited on this machine: it keeps its handle, reports itself
/// visible, sits at exactly the rectangle it was told to, and shows nothing
/// at all. Resizing is fine, moving is not. So the position is decided
/// before the window exists and never changed afterwards, and choosing a
/// different corner throws the window away and builds another one, which is
/// what the corner in the id is for.
///
/// The main window is left alone, which also means the board is still there
/// on a second monitor while the overlay runs on the first.
pub(crate) fn show(ctx: &egui::Context, corner: Corner, height: f32, draw: impl FnMut(&mut Ui)) {
    let size = vec2(WIDTH, height.clamp(space::ROW, CEILING));
    let monitor = ctx
        .input(|i| i.viewport().monitor_size)
        .unwrap_or(FALLBACK_SCREEN);
    let at = place(corner, monitor, size);
    let mut draw = draw;
    ctx.show_viewport_immediate(id_for(corner), attributes(at, size), |ui, _class| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(colour::BG_OVERLAY))
            .show(ui, &mut draw);
    });
}

/// The window's identity, which carries the corner because a corner is only
/// ever applied when the window is built.
fn id_for(corner: Corner) -> ViewportId {
    ViewportId::from_hash_of(("overseer-overlay", corner.label()))
}

/// Everything the overlay window is, decided before it exists.
fn attributes(at: Pos2, size: Vec2) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title("Valorant Overseer overlay")
        .with_position(at)
        .with_inner_size(size)
        .with_decorations(false)
        .with_always_on_top()
        .with_transparent(true)
        // Takes no clicks and holds no keyboard: the game is what you are
        // using, and this is something you glance at.
        .with_mouse_passthrough(true)
        .with_active(false)
        // No second button on the task bar. There is one app here.
        .with_taskbar(false)
        .with_resizable(false)
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::{Corner, MARGIN, TASKBAR, place, size_for};

    /// Each corner is a window of its own.
    ///
    /// The identity carries the corner so that changing it destroys the
    /// window and builds a new one at the new place. Two corners sharing an
    /// identity would mean moving the window instead, and a window moved
    /// after it has been shown stops being drawn at all here.
    #[test]
    fn a_corner_is_part_of_the_windows_identity() {
        let mut seen = std::collections::HashSet::new();
        for corner in Corner::ALL {
            assert!(
                seen.insert(super::id_for(corner)),
                "{corner:?} shares a window with another corner"
            );
        }
    }

    /// Every corner puts the window fully on the screen, in the corner it is
    /// named after.
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

    /// More players is a taller overlay, and an empty one is still a window.
    #[test]
    fn the_height_follows_the_roster() {
        let ten = size_for(10);
        let five = size_for(5);
        assert!(ten.y > five.y, "ten players fitted in five players of room");
        assert!(
            (ten.x - five.x).abs() < f32::EPSILON,
            "the width is not the roster's business"
        );
        assert!(size_for(0).y > 0.0, "an empty lobby has no window at all");
    }
}
