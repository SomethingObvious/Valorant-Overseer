//! The masthead: the match, as a broadcast scorebug.
//!
//! The map's own art dimmed across the whole width, the map's name as the
//! title, the queue and the side as plates, the score in the middle and the
//! connection at the right. The app's name is not in it: the title bar
//! already says what this window is, and the wordmark used to spend the
//! most valuable hundred and forty points in the window saying so twice.

use egui::epaint::Vertex;
use egui::{Align2, Color32, Mesh, Rect, Sense, Shape, Ui, pos2, vec2};
use overseer_core::Board;
use overseer_ui::{Face, art, caps_text, colour, motion, space};

use crate::board::paint;

/// How tall the masthead is.
pub(crate) const HEIGHT: f32 = 56.0;

/// Everything the masthead shows.
#[derive(Debug)]
pub(crate) struct Masthead<'a> {
    /// What the bridge last sent.
    pub(crate) board: &'a Board,
    /// The connection light: how big, what colour, and what it says.
    pub(crate) light: (f32, Color32, String),
    /// The efficient tier: no light behind anything.
    pub(crate) still: bool,
}

/// Draws the masthead.
pub(crate) fn draw(ui: &mut Ui, head: &Masthead<'_>) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    backdrop(&painter, rect, head.board.map.as_deref());
    let middle = rect.center().y;
    let right = light(&painter, rect, &head.light, head.still);
    let left = title(
        &painter,
        head.board,
        rect.left() + space::XL + space::SM,
        middle,
        right,
    );
    score(&painter, head, rect.center().x, middle, (left, right));
}

/// The map, dimmed, under a ramp that keeps the title side dark enough to
/// read on whatever the map happens to be.
fn backdrop(painter: &egui::Painter, rect: Rect, map: Option<&str>) {
    painter.rect_filled(rect, 0, colour::BG_RAISED);
    if let Some(strip) = map.and_then(|m| art::map(painter.ctx(), m)) {
        // Fit the width and take the middle band of the height, so the
        // strip is cropped rather than stretched into a smear.
        let texture = strip.size_vec2();
        let band = (texture.x / texture.y) / (rect.width() / rect.height()).max(0.01);
        let band = band.clamp(0.0, 1.0);
        let top = 0.5 - band / 2.0;
        let mut mesh = Mesh::with_texture(strip.id());
        mesh.add_rect_with_uv(
            rect,
            Rect::from_min_max(pos2(0.0, top), pos2(1.0, top + band)),
            Color32::from_gray(150),
        );
        painter.add(Shape::mesh(mesh));
    }
    let mut ramp = Mesh::default();
    for (x, y, alpha) in [
        (rect.left(), rect.top(), 215_u8),
        (rect.center().x, rect.top(), 90),
        (rect.center().x, rect.bottom(), 90),
        (rect.left(), rect.bottom(), 215),
        (rect.right(), rect.top(), 140),
        (rect.right(), rect.bottom(), 140),
    ] {
        ramp.vertices.push(Vertex {
            pos: pos2(x, y),
            uv: egui::epaint::WHITE_UV,
            color: Color32::from_rgba_unmultiplied(10, 11, 14, alpha),
        });
    }
    ramp.add_triangle(0, 1, 2);
    ramp.add_triangle(0, 2, 3);
    ramp.add_triangle(1, 4, 5);
    ramp.add_triangle(1, 5, 2);
    painter.add(Shape::mesh(ramp));
    painter.hline(rect.x_range(), rect.bottom() - 0.5, (1.0, colour::VOID));
}

/// The map's name, then the state, the queue and the side as plates, each
/// dropped if it would reach `limit`. Returns where the last one ended.
fn title(painter: &egui::Painter, board: &Board, start: f32, middle: f32, limit: f32) -> f32 {
    let name = board.map.as_deref().unwrap_or("overseer");
    let drawn = caps_text(
        painter,
        pos2(start, middle),
        Align2::LEFT_CENTER,
        name,
        Face::Heavy.at(28.0),
        colour::TEXT_STRONG,
    );
    let mut x = drawn.right() + space::LG;
    let state = board
        .state_label
        .as_deref()
        .or(board.state.as_deref())
        .unwrap_or("waiting");
    // The side as a cream plate. Red is the enemy's, and a red ATTACK
    // chip beside a red enemy plate was the same colour meaning two things.
    for (text, tint, solid) in [
        (Some(state), colour::TEXT_DIM, false),
        (board.mode.as_deref(), colour::TEXT, false),
        (board.side.as_deref(), colour::TEXT_STRONG, true),
    ] {
        let Some(text) = text else { continue };
        let width = overseer_ui::caps_width(painter, text, paint::label()) + space::XL * 2.0;
        if x + width > limit {
            break;
        }
        x = plate(painter, x, middle, text, tint, solid).right() + space::SM;
    }
    x
}

/// One slanted plate in the masthead: a tint of its colour with the word in
/// it, or solid with the word in the ground's black.
fn plate(
    painter: &egui::Painter,
    x: f32,
    middle: f32,
    text: &str,
    tint: Color32,
    solid: bool,
) -> Rect {
    let width = overseer_ui::caps_width(painter, text, paint::label()) + space::XL * 2.0;
    let rect = Rect::from_min_size(pos2(x, middle - 11.0), vec2(width, 22.0));
    let (fill, ink) = if solid {
        (tint, colour::BG)
    } else {
        (tint.gamma_multiply(0.16), tint)
    };
    painter.add(paint::slant(rect, true, true, fill));
    let _drawn = caps_text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        paint::label(),
        ink,
    );
    rect
}

/// The score in the middle, or the lock count during agent select, if there
/// is room for it between the two ends.
fn score(painter: &egui::Painter, head: &Masthead<'_>, centre: f32, middle: f32, ends: (f32, f32)) {
    let (left, right) = ends;
    let board = head.board;
    if let Some(score) = board.score.as_ref() {
        if centre - 90.0 < left || centre + 150.0 > right {
            return;
        }
        let (ally, enemy) = (score.ally.unwrap_or(0), score.enemy.unwrap_or(0));
        let font = Face::Heavy.at(36.0);
        let ours = caps_text(
            painter,
            pos2(centre - space::LG, middle),
            Align2::RIGHT_CENTER,
            &ally.to_string(),
            font.clone(),
            colour::ALLY,
        );
        flare(painter, ours, ally, colour::ALLY, head.still);
        let cut = Rect::from_center_size(pos2(centre, middle), vec2(8.0, 30.0));
        painter.add(paint::slant(
            cut,
            true,
            true,
            colour::TEXT_FAINT.gamma_multiply(0.6),
        ));
        let theirs = caps_text(
            painter,
            pos2(centre + space::LG, middle),
            Align2::LEFT_CENTER,
            &enemy.to_string(),
            font,
            colour::ENEMY,
        );
        flare(painter, theirs, enemy, colour::ENEMY, head.still);
        if let Some(round) = score.round {
            let _drawn = caps_text(
                painter,
                pos2(theirs.right() + space::LG, middle + 2.0),
                Align2::LEFT_CENTER,
                &format!("round {round}"),
                paint::label(),
                // Brighter than a label usually is: it sits on the map's
                // art, and dim grey on a bright map dropped under 4.5 to 1.
                colour::TEXT,
            );
        }
    } else if let Some(lock) = board.lock_progress.as_ref() {
        let text = format!(
            "{} of {} locked",
            lock.locked.unwrap_or(0),
            lock.total.unwrap_or(0)
        );
        let width = overseer_ui::caps_width(painter, &text, paint::label()) + space::XL * 2.0;
        if centre - width / 2.0 > left && centre + width / 2.0 < right {
            let _plate = plate(
                painter,
                centre - width / 2.0,
                middle,
                &text,
                colour::WARN,
                true,
            );
        }
    }
}

/// A light behind a score digit, for as long as the animator takes to catch
/// up with it.
///
/// Nothing else in the window flashes, because nothing else in it is a
/// thing that happens. A round won is, and it comes off the animator rather
/// than a timestamp kept for the purpose: how far the eased value still is
/// from the real one is exactly how recently it happened. A new match
/// starting from nought is not a round won, so a fall resets it silently.
fn flare(painter: &egui::Painter, digit: Rect, value: u32, tint: Color32, still: bool) {
    let id = egui::Id::new(("score", tint.to_array()));
    let target = value as f32;
    let previous = painter.ctx().data(|d| d.get_temp::<f32>(id));
    if previous.is_some_and(|p| target < p) || still {
        painter
            .ctx()
            .animate_value_with_time(id.with("eased"), target, 0.0);
    }
    painter.ctx().data_mut(|d| d.insert_temp(id, target));
    let settled = painter
        .ctx()
        .animate_value_with_time(id.with("eased"), target, motion::MEASURE);
    let strength = (target - settled).abs().min(1.0);
    if strength > 0.01 {
        painter.add(paint::glow(
            digit.center(),
            digit.height() * 1.1,
            tint,
            0.5 * strength,
        ));
    }
}

/// The connection light and what it says, from the right edge. Returns
/// where it starts.
fn light(painter: &egui::Painter, rect: Rect, light: &(f32, Color32, String), still: bool) -> f32 {
    let (radius, tint, text) = light;
    let drawn = caps_text(
        painter,
        pos2(rect.right() - space::XL, rect.center().y),
        Align2::RIGHT_CENTER,
        text,
        paint::label(),
        colour::TEXT_DIM,
    );
    let dot = pos2(drawn.left() - space::MD, rect.center().y);
    if !still {
        painter.add(paint::glow(dot, radius * 3.5, *tint, 0.55));
    }
    painter.circle_filled(dot, *radius, *tint);
    dot.x - radius - space::LG
}
