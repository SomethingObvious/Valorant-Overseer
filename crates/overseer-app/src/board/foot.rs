//! Under the roster: anything the backend has to say, and how the session
//! is going.
//!
//! It follows the roster rather than being pinned to the bottom of the
//! window. Pinned, it left a band of nothing between the last row and itself
//! at every height taller than the board, and a reserved height that the
//! ladder could decline to use floated it a hundred points off the bottom.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Session};
use overseer_ui::{Face, caps_text, colour, space};

use super::paint;

/// Anything the backend needs to say about the board itself.
///
/// The only place the app speaks rather than reports, so it gets the shared
/// band. A message the backend sent and nothing displayed is a message
/// nobody will ever see.
pub(crate) fn notice(ui: &mut Ui, board: &Board) {
    let Some(notice) = board.notice.as_ref() else {
        return;
    };
    let Some(message) = notice.message.as_deref().filter(|m| !m.is_empty()) else {
        return;
    };
    let tint = match notice.level.as_deref() {
        Some("error") => colour::ENEMY,
        Some("warn" | "warning") => colour::WARN,
        _ => colour::INFO,
    };
    overseer_ui::say(ui, 0.0, tint, message, notice.action.as_deref());
}

/// The session: net rating, one pip per match, and the record.
///
/// Five greens and a red is a session you read at a glance; "+37" on its own
/// is a number you have to think about, so both are here.
pub(crate) fn session(ui: &mut Ui, board: &Board) {
    let Some(session) = board.session.as_ref().filter(|s| !s.points.is_empty()) else {
        return;
    };
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::XXL + space::SM),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let y = rect.center().y;
    let after = caps_text(
        &painter,
        pos2(rect.left(), y),
        Align2::LEFT_CENTER,
        "session",
        Face::Heavy.at(15.0),
        colour::TEXT,
    );
    let net = session
        .net
        .unwrap_or_else(|| session.points.iter().filter_map(|p| p.delta).sum());
    let tint = match net.signum() {
        1 => colour::GOOD,
        -1 => colour::BAD,
        _ => colour::TEXT_DIM,
    };
    let net_at = caps_text(
        &painter,
        pos2(after.right() + space::LG, y),
        Align2::LEFT_CENTER,
        &format!("{net:+}"),
        Face::Heavy.at(20.0),
        tint,
    );
    let rr = caps_text(
        &painter,
        pos2(net_at.right() + space::SM, y + 2.0),
        Align2::LEFT_CENTER,
        "rr",
        paint::label(),
        colour::TEXT_FAINT,
    );
    let record = record(session);
    let record_at = caps_text(
        &painter,
        pos2(rect.right(), y),
        Align2::RIGHT_CENTER,
        &record,
        paint::label(),
        colour::TEXT_FAINT,
    );
    results(
        &painter,
        session,
        rr.right() + space::XL,
        record_at.left() - space::XL,
        y,
    );
}

/// One slanted block per match, oldest left, in the colour of what it did
/// to your rating, stopping before it would reach the record.
fn results(painter: &egui::Painter, session: &Session, from: f32, limit: f32, y: f32) {
    let mut x = from;
    for point in &session.points {
        let tint = match point.delta.unwrap_or(0).signum() {
            1 => colour::ALLY,
            -1 => colour::ENEMY,
            _ => colour::TEXT_FAINT,
        };
        let block = Rect::from_min_size(pos2(x, y - 7.0), vec2(16.0, 14.0));
        if block.right() > limit {
            break;
        }
        painter.add(paint::slant(block, true, true, tint));
        x = block.right() + 3.0;
    }
}

/// "4 of 6 won".
fn record(session: &Session) -> String {
    let won = session
        .points
        .iter()
        .filter(|p| {
            p.result
                .as_deref()
                .is_some_and(|r| r.eq_ignore_ascii_case("victory"))
        })
        .count();
    format!("{won} of {} won", session.points.len())
}
