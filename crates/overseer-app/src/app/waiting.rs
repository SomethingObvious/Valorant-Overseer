//! What the window says while there is no board to draw: how far the
//! backend, the Riot client, VALORANT and the match have got, with the step
//! being waited on moving, so waiting reads apart from stuck.

use std::time::Duration;

use egui::{Align2, Color32, Rect, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Status};

use crate::board;
use overseer_ui::{self, Face, caps_text, colour, motion, size, space};

/// The steps to a board, in order.
const STEPS: [&str; 4] = ["Backend", "Riot Client", "VALORANT", "Match"];

/// How often the step being waited on is redrawn as it moves, in seconds.
const TICK: f64 = 1.0 / 30.0;

/// How long the step being waited on moves for, in seconds. Each frame
/// redraws the whole window, and with VALORANT closed this screen can sit
/// there all day beside another game.
const LIVELY: f64 = 10.0;

/// How long the light takes to run along the line into the step being
/// waited on, and its pip to pulse once, in seconds.
const BEAT: f64 = 1.2;

/// What to say, which step is being waited on, and whether it failed
/// rather than being slow.
struct Stage<'a> {
    title: &'a str,
    detail: &'a str,
    step: usize,
    failed: bool,
}

/// Where things have got, from the bridge's status and what the empty board
/// says it waits on.
fn stage<'a>(status: &'a Status, board: &'a Board, trouble: Option<&'a str>) -> Stage<'a> {
    let said = board.notice.as_ref().and_then(|n| n.message.as_deref());
    let (title, detail, step, failed) = match (trouble, status, board.waiting.as_deref()) {
        (Some(why), ..) => ("This Build Can't Read the Bridge", why, 0, true),
        (None, Status::Connecting(_), _) => (
            "Starting Up",
            "Overseer's backend is starting, which takes a few seconds.",
            0,
            false,
        ),
        (None, Status::Lost(why), _) => ("Not Connected", why.as_str(), 0, true),
        (None, Status::Live, Some("riot")) => (
            "Waiting for the Riot Client",
            "Open VALORANT, or sign in to the Riot Client if it's open already.",
            1,
            false,
        ),
        (None, Status::Live, Some("game")) => (
            "Waiting for VALORANT",
            "You're signed in. Your lobby shows up here once VALORANT reaches the menus.",
            2,
            false,
        ),
        (None, Status::Live, Some("match")) => (
            "Loading the Match",
            "Looking up everyone in it, which takes a few seconds.",
            3,
            false,
        ),
        (None, Status::Live, _) if board.state.as_deref() == Some("OFFLINE") => (
            "Couldn't Read VALORANT",
            said.unwrap_or("Close VALORANT completely and open it again."),
            1,
            true,
        ),
        (None, Status::Live, _) => (
            "Looking for VALORANT",
            "Checking whether the Riot Client is open.",
            1,
            false,
        ),
    };
    Stage {
        title,
        detail,
        step,
        failed,
    }
}

/// What to say while there is no board, on a plate with the steps along
/// its foot and how long this one has taken.
pub(super) fn empty(
    ui: &mut Ui,
    (status, board): (&Status, &Board),
    trouble: Option<&str>,
    fade: f32,
) {
    let stage = stage(status, board, trouble);
    let room = ui.available_rect_before_wrap();
    if room.height() < 210.0 {
        plain(ui, stage.title, stage.detail);
        return;
    }
    let (rect, _response) = ui.allocate_exact_size(room.size(), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let width = 440.0_f32.min(space::XXL.mul_add(-2.0, rect.width()));
    // Laid out first, because the plate is as tall as its sentence, and the
    // longest sentence is whatever the bridge last refused to do.
    let mut job = egui::text::LayoutJob::simple(
        stage.detail.to_owned(),
        Face::Body.at(size::BODY),
        colour::TEXT_DIM,
        space::XL.mul_add(-2.0, width),
    );
    job.wrap.max_rows = 3;
    job.wrap.overflow_character = Some('\u{2026}');
    let sentence = painter.layout_job(job);
    let plate = Rect::from_center_size(
        pos2(
            rect.center().x,
            rect.top() + (rect.height() * 0.44).max(120.0),
        ),
        vec2(width, 54.0 + sentence.size().y + space::XL + 58.0),
    );
    board::paint::slab(&painter, plate, colour::BG_RAISED, 0.0, false);
    let since = since(ui, stage.title);
    // The words fade in again whenever they change, and the steps light up
    // in turn, so the backend and Riot coming up read as progress.
    let mut ink = painter.clone();
    if fade > 0.0 {
        ink.multiply_opacity(motion::eased(since as f32 / fade));
    }
    words(&ink, plate, stage.title, sentence, since);
    let lit = ui.ctx().animate_value_with_time(
        egui::Id::new("empty-steps"),
        stage.step as f32,
        fade * 2.0,
    );
    let moving = fade > 0.0 && !stage.failed && since < LIVELY;
    chain(
        &painter,
        plate,
        (lit, stage.step, stage.failed),
        moving.then_some(since),
    );
    // The clock only needs its next second, or its next minute after the
    // first, unless something is moving.
    let fading = fade > 0.0 && since < f64::from(fade);
    let step = if since < 60.0 { 1.0 } else { 60.0 };
    let next = if moving || fading {
        TICK
    } else {
        // Never under half a second, so a frame that lands just before the
        // clock turns over doesn't ask for another straight away.
        (step - since.rem_euclid(step)).max(0.5)
    };
    ui.ctx()
        .request_repaint_after(Duration::from_secs_f64(next));
}

/// How long the screen has been saying `title`, in seconds. It starts
/// again when the title changes, and when the screen comes back after a
/// pass that drew something else, so a wait doesn't go on from the last
/// time the same words showed, hours ago.
fn since(ui: &Ui, title: &str) -> f64 {
    let now = ui.input(|i| i.time);
    let pass = ui.ctx().cumulative_pass_nr();
    let key = egui::Id::new(title);
    ui.ctx().data_mut(|d| {
        let kept = d.get_temp_mut_or_insert_with(egui::Id::new("empty-since"), || (key, now, pass));
        if kept.0 != key || pass > kept.2.saturating_add(1) {
            *kept = (key, now, pass);
        }
        kept.2 = pass;
        now - kept.1
    })
}

/// The clock for a wait `seconds` long: to the second for its first
/// minute, and in whole minutes after that.
fn clock(seconds: u64) -> String {
    if seconds < 60 {
        format!("0:{seconds:02}")
    } else {
        format!("{} min", seconds.div_euclid(60))
    }
}

/// The words on the plate: the headline, the sentence already laid out to
/// decide how tall the plate had to be, and how long it has been waiting at
/// the end of the line under them.
fn words(
    painter: &egui::Painter,
    plate: Rect,
    title: &str,
    sentence: std::sync::Arc<egui::Galley>,
    since: f64,
) {
    let _title = caps_text(
        painter,
        pos2(plate.left() + space::XL, plate.top() + 28.0),
        Align2::LEFT_CENTER,
        title,
        Face::Heavy.at(24.0),
        colour::TEXT_STRONG,
    );
    painter.galley(
        pos2(plate.left() + space::XL, plate.top() + 52.0),
        sentence,
        colour::TEXT_DIM,
    );
    let line = plate.bottom() - 58.0;
    let seconds = since.max(0.0) as u64;
    let clock = caps_text(
        painter,
        pos2(plate.right() - space::XL, line),
        Align2::RIGHT_CENTER,
        &clock(seconds),
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    painter.hline(
        plate.left() + space::XL..=clock.left() - space::MD,
        line,
        (1.0, colour::LINE),
    );
}

/// The steps across the foot of `plate`, lit up to `lit` as it eases
/// towards `step`, the one being waited on. That one pulses with a light
/// running along the line into it, `since` seconds into the wait, or is red
/// when it `failed`.
fn chain(
    painter: &egui::Painter,
    plate: Rect,
    (lit, step, failed): (f32, usize, bool),
    since: Option<f64>,
) {
    let column = space::XL.mul_add(-2.0, plate.width()) / STEPS.len() as f32;
    let middle = |at: usize| plate.left() + space::XL + column * (at as f32 + 0.5);
    let (pips, names) = (plate.bottom() - 34.0, plate.bottom() - 14.0);
    let beat = since.map(|s| (s / BEAT).fract() as f32);
    for (at, name) in STEPS.into_iter().enumerate() {
        let x = middle(at);
        // How done this step is, from 0 to 1, as `lit` eases past it.
        let done = (lit - at as f32).clamp(0.0, 1.0);
        let waited_on = at == step;
        if at > 0 {
            let (from, to) = (middle(at - 1) + 12.0, x - 12.0);
            // Green once the step it leads to is done, so the line into the
            // one being waited on stays grey under its moving light.
            let reached = (lit - at as f32).clamp(0.0, 1.0);
            painter.hline(
                from..=to,
                pips,
                (1.5, colour::LINE.lerp_to_gamma(colour::ALLY, reached * 0.6)),
            );
            if waited_on
                && !failed
                && let Some(t) = beat
            {
                let head = (to - from).mul_add(motion::eased(t), from);
                painter.hline(
                    (head - 14.0).max(from)..=head,
                    pips,
                    (2.0, colour::ALLY.gamma_multiply(t.mul_add(-0.5, 1.0))),
                );
            }
        }
        let look = match (waited_on, failed) {
            (true, true) => Pip::Failed,
            (true, false) => Pip::Waiting(beat),
            (false, _) => Pip::Done(done),
        };
        pip(painter, pos2(x, pips), look);
        let ink: Color32 = if waited_on {
            colour::TEXT_STRONG
        } else {
            colour::TEXT_FAINT.lerp_to_gamma(colour::TEXT, done)
        };
        let _name = caps_text(
            painter,
            pos2(x, names),
            Align2::CENTER_CENTER,
            name,
            Face::Display.at(size::MICRO),
            ink,
        );
    }
}

/// How a step's pip looks.
#[derive(Debug, Clone, Copy)]
enum Pip {
    /// Done this far, from 0 to 1.
    Done(f32),
    /// Being waited on, this far through the beat when it moves.
    Waiting(Option<f32>),
    /// Being waited on, and failed.
    Failed,
}

/// A step's pip at `centre`. The one being waited on is outlined, so it
/// reads apart from the done ones even when nothing moves, and when it does
/// move its fill fades and comes back once a beat as a ring goes out of it.
fn pip(painter: &egui::Painter, centre: egui::Pos2, look: Pip) {
    let size = vec2(12.0, 14.0);
    let (fill, edge) = match look {
        Pip::Done(done) => (
            colour::TEXT_FAINT
                .lerp_to_gamma(colour::ALLY, done)
                .gamma_multiply(0.65_f32.mul_add(done, 0.35)),
            egui::Stroke::NONE,
        ),
        Pip::Failed => (colour::ENEMY, egui::Stroke::NONE),
        Pip::Waiting(beat) => {
            if let Some(t) = beat {
                let ring = Rect::from_center_size(centre, size * t.mul_add(1.2, 1.0));
                painter.add(board::paint::slant(
                    ring,
                    true,
                    true,
                    colour::ALLY.gamma_multiply(0.35 * (1.0 - t)),
                ));
            }
            let solid = beat.map_or(0.5, |t| {
                0.45_f32.mul_add((t * std::f32::consts::TAU).cos(), 0.55)
            });
            (
                colour::ALLY.gamma_multiply(solid * 0.6),
                egui::Stroke::new(1.5, colour::ALLY),
            )
        }
    };
    painter.add(egui::Shape::convex_polygon(
        board::paint::slanted(Rect::from_center_size(centre, size), true, true),
        fill,
        edge,
    ));
}

/// The same words with no furniture, for a window too short to hold any.
fn plain(ui: &mut Ui, title: &str, detail: &str) {
    ui.add_space(space::XXL);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XXL * 2.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let _title = caps_text(
        painter,
        pos2(rect.center().x, rect.top() + space::LG),
        Align2::CENTER_CENTER,
        title,
        Face::Heavy.at(22.0),
        colour::TEXT_STRONG,
    );
    painter.text(
        pos2(rect.center().x, rect.bottom() - space::LG),
        Align2::CENTER_CENTER,
        detail,
        Face::Body.at(size::BODY),
        colour::TEXT_DIM,
    );
}

#[cfg(test)]
mod tests {
    use super::{Board, Status, clock, since, stage};

    /// The wait starts again with new words, and when the screen comes
    /// back after a pass without it. Its clock counts seconds for a
    /// minute, then minutes.
    #[test]
    fn a_wait_starts_again_when_the_screen_comes_back() {
        let ctx = egui::Context::default();
        let pass = |at: f64, title: Option<&str>| {
            let input = egui::RawInput {
                time: Some(at),
                ..egui::RawInput::default()
            };
            let mut waited = None;
            let mut drawn = ctx.run_ui(input, |ui| waited = title.map(|t| since(ui, t)));
            drawn.textures_delta.clear();
            waited
        };
        assert_eq!(pass(100.0, Some("Waiting")), Some(0.0));
        assert_eq!(pass(105.0, Some("Waiting")), Some(5.0));
        assert_eq!(pass(106.0, None), None);
        assert_eq!(
            pass(107.0, Some("Waiting")),
            Some(0.0),
            "back after a pass away"
        );
        assert_eq!(pass(109.0, Some("Loading")), Some(0.0), "new words");
        assert_eq!(clock(42), "0:42");
        assert_eq!(clock(200), "3 min");
    }

    /// Each step lights only once the backend says it has got there, and
    /// the last is the match loading, the one moment it can light.
    #[test]
    fn each_step_is_what_the_backend_says_it_waits_on() {
        let waiting = |on: &str| Board {
            waiting: Some(on.to_owned()),
            ..Board::default()
        };
        let live = Status::Live;
        let read = |status: &Status, board: &Board| {
            let s = stage(status, board, None);
            (s.step, s.failed)
        };
        let starting = Status::Connecting("no backend yet".to_owned());
        assert_eq!(read(&starting, &Board::default()), (0, false));
        assert_eq!(
            read(&Status::Lost("gone".to_owned()), &Board::default()),
            (0, true)
        );
        assert_eq!(read(&live, &Board::default()), (1, false));
        assert_eq!(read(&live, &waiting("riot")), (1, false));
        assert_eq!(read(&live, &waiting("game")), (2, false));
        assert_eq!(read(&live, &waiting("match")), (3, false));
        let broken = Board {
            state: Some("OFFLINE".to_owned()),
            ..Board::default()
        };
        assert_eq!(read(&live, &broken), (1, true));
        assert_eq!(stage(&live, &waiting("game"), Some("old bridge")).step, 0);
    }
}
