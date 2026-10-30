//! The masthead, laid out like a broadcast scorebug: the map's art dimmed
//! across the width, its name as the title, the queue and side as plates, the
//! score in the middle and the connection light at the right. It is also the
//! window's title bar, since the window draws its own: it drags the window,
//! and the settings and window buttons end it.

use egui::epaint::Vertex;
use egui::{Align2, Color32, Mesh, Rect, Sense, Shape, Ui, pos2, vec2};
use overseer_core::Board;
use overseer_ui::{Face, art, caps_text, chrome, colour, motion, space};

use crate::board::paint;

/// How tall the masthead is.
pub(crate) const HEIGHT: f32 = 56.0;

/// Everything the masthead shows.
#[derive(Debug)]
pub(crate) struct Masthead<'a> {
    /// What the bridge last sent.
    pub(crate) board: &'a Board,
    /// The connection light.
    pub(crate) light: Light,
    /// The efficient tier: no light behind anything.
    pub(crate) still: bool,
    /// Whether the window fills the screen, which turns maximize into restore.
    pub(crate) maximized: bool,
    /// The screen whose button is lit, because it is open.
    pub(crate) open: Option<Chrome>,
    /// Whether the pointer is on the window's edge, where a press resizes
    /// rather than drags.
    pub(crate) edge: bool,
}

pub(crate) use overseer_ui::chrome::Button as Chrome;

/// The connection light: how big, what colour, the word beside it, which is
/// empty while all is well, and the exact reason, said on its hover.
#[derive(Debug)]
pub(crate) struct Light {
    /// How big the dot is.
    pub(crate) radius: f32,
    /// What colour it is.
    pub(crate) tint: Color32,
    /// The word beside it.
    pub(crate) word: &'static str,
    /// The exact reason, for its hover.
    pub(crate) why: String,
}

/// Draws the masthead, and says which button was pressed.
pub(crate) fn draw(ui: &mut Ui, head: &Masthead<'_>) -> Option<Chrome> {
    let (rect, bar) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::click_and_drag());
    let pressed = (!head.edge && chrome::drag(ui, &bar)).then_some(Chrome::Maximize);
    if !ui.is_rect_visible(rect) {
        return pressed;
    }
    let painter = ui.painter().clone();
    backdrop(&painter, rect, head.board.map.as_deref());
    let middle = rect.center().y;
    let (buttons_at, clicked) = chrome::buttons(
        ui,
        rect,
        &[
            Chrome::Close,
            Chrome::Maximize,
            Chrome::Minimize,
            Chrome::Settings,
            Chrome::History,
            Chrome::Lineups,
        ],
        chrome::State {
            maximized: head.maximized,
            open: head.open,
        },
    );
    let right = light(
        &painter,
        rect.with_max_x(buttons_at),
        &head.light,
        head.still,
    );
    let spoken = ui.interact(
        Rect::from_x_y_ranges(right..=buttons_at, rect.y_range()),
        ui.id().with("connection"),
        Sense::hover(),
    );
    let said = match (head.light.word, head.light.why.as_str()) {
        ("", _) => "Connected".to_owned(),
        (word, "") => word.to_owned(),
        (word, why) => format!("{word}. {why}"),
    };
    spoken.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &said));
    if !head.light.why.is_empty() {
        let _why = spoken.on_hover_text(&head.light.why);
    }
    let right = reading(ui, &painter, rect, right, head);
    let left = title(
        &painter,
        head.board,
        rect.left() + space::XL + space::SM,
        middle,
        right,
    );
    score(&painter, head, rect.center().x, middle, (left, right));
    clicked.or(pressed)
}

/// The map, dimmed, under a ramp that keeps the title side dark enough to
/// read on whatever the map happens to be.
pub(crate) fn backdrop(painter: &egui::Painter, rect: Rect, map: Option<&str>) {
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
            color: colour::BG.gamma_multiply_u8(alpha),
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
    let name = board.map.as_deref().unwrap_or("Overseer");
    let font = Face::Heavy.at(28.0);
    // The line box the title is centred in keeps room for descenders under
    // the capitals, so anything lined up with the box would sit high. The
    // mark is centred on the capitals, and the plates share their bottom.
    let galley = painter.layout_job(overseer_ui::caps(name, font.clone(), colour::TEXT_STRONG));
    let caps = galley.mesh_bounds;
    let top = middle - galley.size().y / 2.0;
    // Out of a match the title is the app's own name, with the mark in front.
    let mut start = start;
    if board.map.is_none()
        && let Some(mark) = art::mark(painter.ctx())
    {
        // A little taller than the capitals.
        let centre = top + caps.center().y;
        let tall = caps.height() * art::MARK_OVER_CAPS;
        let wide = tall * mark.aspect_ratio();
        let at = Rect::from_min_size(pos2(start, centre - tall / 2.0), vec2(wide, tall));
        let mut mesh = Mesh::with_texture(mark.id());
        mesh.add_rect_with_uv(
            at,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.add(Shape::mesh(mesh));
        start = at.right() + space::MD;
    }
    let drawn = caps_text(
        painter,
        pos2(start, middle),
        Align2::LEFT_CENTER,
        name,
        font,
        colour::TEXT_STRONG,
    );
    let mut x = drawn.right() + space::LG;
    let state = board
        .state_label
        .as_deref()
        .or(board.state.as_deref())
        .unwrap_or("Waiting");
    // The side is a solid plate in the game's own colours for it. The state
    // drops out once there is a score, since the score says the match is on.
    for (text, tint, solid) in [
        (
            board.score.is_none().then_some(state),
            colour::TEXT_DIM,
            false,
        ),
        // Skipped when the state already says it: in the lobby the mode is
        // "Lobby" and the state is "In Lobby".
        (
            board
                .mode
                .as_deref()
                .filter(|mode| !state.to_lowercase().contains(&mode.to_lowercase())),
            colour::TEXT,
            false,
        ),
        (
            board.side.as_deref(),
            paint::side_tint(board.side.as_deref(), colour::TEXT_STRONG),
            true,
        ),
    ] {
        let Some(text) = text else { continue };
        let width = space::XL.mul_add(2.0, overseer_ui::caps_width(painter, text, paint::label()));
        if x + width > limit {
            break;
        }
        x = plate(painter, x, top + caps.max.y, text, tint, solid).right() + space::SM;
    }
    x
}

/// One slanted plate in the masthead standing on `bottom`: a tint of its
/// colour with the word in it, or solid with the word in the ground's black.
pub(crate) fn plate(
    painter: &egui::Painter,
    x: f32,
    bottom: f32,
    text: &str,
    tint: Color32,
    solid: bool,
) -> Rect {
    let width = space::XL.mul_add(2.0, overseer_ui::caps_width(painter, text, paint::label()));
    let rect = Rect::from_min_size(pos2(x, bottom - 22.0), vec2(width, 22.0));
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
                &format!("Round {round}"),
                paint::label(),
                // Brighter than a label usually is, because dim grey on a
                // bright map falls under 4.5 to 1 contrast.
                colour::TEXT,
            );
        }
    } else if let Some(lock) = board.lock_progress.as_ref() {
        let text = format!(
            "{} of {} locked",
            lock.locked.unwrap_or(0),
            lock.total.unwrap_or(0)
        );
        let width = space::XL.mul_add(2.0, overseer_ui::caps_width(painter, &text, paint::label()));
        if centre - width / 2.0 > left && centre + width / 2.0 < right {
            let _plate = plate(
                painter,
                centre - width / 2.0,
                middle + 11.0,
                &text,
                colour::TEXT_STRONG,
                false,
            );
        }
    }
}

/// A glow behind a score digit, for as long as the animator takes to catch up
/// with a new value.
fn flare(painter: &egui::Painter, digit: Rect, value: u32, tint: Color32, still: bool) {
    let id = egui::Id::new(("score", tint.to_array()));
    let target = value as f32;
    let previous = painter.ctx().data(|d| d.get_temp::<f32>(id));
    // A score that falls is a new match, not a round won, so it snaps to the
    // new value without a glow.
    if previous.is_some_and(|p| target < p) || still {
        painter
            .ctx()
            .animate_value_with_time(id.with("eased"), target, 0.0);
    }
    painter.ctx().data_mut(|d| d.insert_temp(id, target));
    let settled = painter
        .ctx()
        .animate_value_with_time(id.with("eased"), target, motion::MEASURE);
    // How far the eased value still lags is how recently the round was won,
    // so there is no timestamp to keep.
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

/// How big the reading ring is.
const RING: f32 = 6.0;

/// While the rows are still reading back, a ring for how much is in hand
/// and when the rest should be, left of `right`. Returns where it starts.
fn reading(ui: &Ui, painter: &egui::Painter, rect: Rect, right: f32, head: &Masthead<'_>) -> f32 {
    let Some(reading) = head.board.reading.as_ref() else {
        return right;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    let left = (reading.done_at - now).max(0.0);
    // Whole seconds, so it ticks about once a second. egui takes a wait
    // shorter than a frame as a repaint now, and a frame can take a quarter
    // second, so a tick due sooner than that waits for the one after.
    let tick = (1.0 - now.fract()).max(0.3);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f64(tick));
    let middle = rect.center().y;
    let when = countdown(left);
    let games = reading.games.unwrap_or(20);
    let time = caps_text(
        painter,
        pos2(right, middle),
        Align2::RIGHT_CENTER,
        &when,
        paint::label(),
        colour::TEXT,
    );
    let what = caps_text(
        painter,
        pos2(time.left() - space::SM, middle),
        Align2::RIGHT_CENTER,
        &format!("{games} Games"),
        paint::label(),
        colour::TEXT_DIM,
    );
    let centre = pos2(what.left() - space::MD - RING, middle);
    let id = ui.id().with("reading");
    let shown = if head.still {
        reading.progress
    } else {
        ui.ctx().animate_value_with_time(id, reading.progress, 0.4)
    };
    painter.circle_stroke(centre, RING, (2.0, colour::EDGE));
    // Clockwise from the top, as a clock fills.
    let sweep = shown.clamp(0.0, 1.0) * std::f32::consts::TAU;
    let points: Vec<egui::Pos2> = (0..=32)
        .map(|i| {
            let angle = sweep.mul_add(i as f32 / 32.0, -std::f32::consts::FRAC_PI_2);
            centre + RING * vec2(angle.cos(), angle.sin())
        })
        .collect();
    painter.add(Shape::line(
        points,
        egui::Stroke::new(2.0, colour::TEXT_STRONG),
    ));
    let area = Rect::from_min_max(
        pos2(centre.x - RING - space::SM, middle - 12.0),
        pos2(right, middle + 12.0),
    );
    let hover = ui.interact(area, id, Sense::hover());
    hover.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::ProgressIndicator,
            true,
            format!("Reading {games} games, {when}"),
        )
    });
    if hover.hovered() {
        crate::board::tip::mark_hovered(ui.ctx(), ui.layer_id(), area);
        let pace = if left < 1.0 {
            "The last of them are on their way.".to_owned()
        } else {
            format!("About {when} to go, at the pace Riot allows.")
        };
        crate::board::tip::say_under(
            ui.ctx(),
            id.with("card"),
            area,
            &format!(
                "Reading each player's last {games} games, {:.0}% so far. {pace}",
                reading.progress * 100.0
            ),
        );
    }
    centre.x - RING - space::LG
}

/// What is left, the way the ring says it: seconds, then minutes and
/// seconds, and once the estimate has run out, that it is finishing.
fn countdown(left: f64) -> String {
    match left.ceil() {
        s if s < 1.0 => "Finishing".to_owned(),
        s if s < 60.0 => format!("{s:.0}s"),
        s => format!("{:.0}:{:02.0}", (s / 60.0).floor(), s.rem_euclid(60.0)),
    }
}

/// The connection light and what it says, from the right edge. Returns
/// where it starts.
fn light(painter: &egui::Painter, rect: Rect, light: &Light, still: bool) -> f32 {
    let Light {
        radius, tint, word, ..
    } = light;
    let drawn = caps_text(
        painter,
        pos2(rect.right() - space::XL, rect.center().y),
        Align2::RIGHT_CENTER,
        word,
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

#[cfg(test)]
mod tests {
    use super::countdown;

    /// A part second counts as a whole one, so it never says 0s while
    /// there is still something to wait for.
    #[test]
    fn the_countdown_reads_like_a_clock() {
        assert_eq!(countdown(0.0), "Finishing");
        assert_eq!(countdown(0.2), "1s");
        assert_eq!(countdown(42.0), "42s");
        assert_eq!(countdown(59.5), "1:00");
        assert_eq!(countdown(85.0), "1:25");
        assert_eq!(countdown(600.0), "10:00");
    }
}
