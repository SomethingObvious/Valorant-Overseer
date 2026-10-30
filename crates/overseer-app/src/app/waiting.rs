//! What the window says while there is no board to draw: how far the
//! backend, the Riot client, VALORANT and the match have got, with the step
//! being waited on moving, so waiting reads apart from stuck.

use std::path::Path;
use std::time::Duration;

use egui::{Align2, Color32, Rect, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Saved, Status};

use crate::board::{self, Asked};
use crate::controls::{self, Tone};
use overseer_ui::{self, Face, caps_text, colour, motion, size, space};

/// The steps to a board, in order.
const STEPS: [&str; 4] = ["Overseer", "Riot Client", "VALORANT", "Match"];

/// How often the step being waited on is redrawn as it moves, in seconds.
const TICK: f64 = 1.0 / 30.0;

/// How long the step being waited on moves for, in seconds. Each frame
/// redraws the whole window, and with VALORANT closed this screen can sit
/// there all day beside another game.
const LIVELY: f64 = 10.0;

/// How long the screen can be away, in seconds, before the wait counts as
/// a new one when it comes back. A look at Lineups or the settings carries
/// on, where a match in between starts over.
const AWAY: f64 = 300.0;

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
fn stage<'a>(
    status: &'a Status,
    board: &'a Board,
    trouble: Option<(&'a str, &'a str)>,
) -> Stage<'a> {
    let said = board.notice.as_ref().and_then(|n| n.message.as_deref());
    let (title, detail, step, failed) = match (trouble, status, board.waiting.as_deref()) {
        (Some((title, why)), ..) => (title, why, 0, true),
        (None, Status::Connecting(_), _) => (
            "Starting Up",
            "Overseer is starting up, which takes a few seconds.",
            0,
            false,
        ),
        // What went wrong exactly is on the connection light's hover.
        (None, Status::Lost(_), _) => (
            "Not Connected",
            "Overseer lost its connection and keeps trying by itself. Reconnect Now tries again straight away.",
            0,
            true,
        ),
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
            2,
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
/// its foot and how long this one has taken. Under it, Reconnect Now while
/// the connection is down or stopped, and Show My Last Stats with your stats
/// kept from an earlier board. Says which was pressed. `stopped` is None
/// where nothing can be pressed, like the overlay.
pub(super) fn empty(
    ui: &mut Ui,
    (status, board): (&Status, &Board),
    trouble: Option<(&str, &str)>,
    fade: f32,
    stopped: Option<bool>,
) -> Option<Asked> {
    let stage = stage(status, board, trouble);
    let mut offers = Vec::new();
    if stopped.is_some_and(|s| s || matches!(status, Status::Lost(_))) {
        offers.push((Asked::Reconnect, "Reconnect Now"));
    }
    if stopped.is_some() && board.you.is_some() {
        offers.push((Asked::LastStats, "Show My Last Stats"));
    }
    let room = ui.available_rect_before_wrap();
    // Painted, so a screen reader gets the same words from a node of their own.
    let said = ui.interact(room, ui.id().with("waiting-said"), Sense::hover());
    said.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Label,
            true,
            format!("{}. {}", stage.title, stage.detail),
        )
    });
    if room.height() < 210.0 {
        plain(ui, stage.title, stage.detail);
        let top = ui.cursor().top() + space::SM;
        return offer(ui, room.center().x, top, &offers);
    }
    let (rect, _response) = ui.allocate_exact_size(room.size(), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return None;
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
    let step = if since < 600.0 { 1.0 } else { 60.0 };
    let next = if moving || fading {
        TICK
    } else {
        // Never under half a second, so a frame that lands just before the
        // clock turns over doesn't ask for another straight away.
        (step - since.rem_euclid(step)).max(0.5)
    };
    ui.ctx()
        .request_repaint_after(Duration::from_secs_f64(next));
    offer(ui, plate.center().x, plate.bottom() + space::LG, &offers)
}

/// The buttons under the plate, side by side and centred on `x` with their
/// top at `top`. Says which was pressed.
fn offer(ui: &mut Ui, x: f32, top: f32, offers: &[(Asked, &str)]) -> Option<Asked> {
    let wide: f32 = 200.0;
    let count = offers.len() as f32;
    let all = wide.mul_add(count, space::MD * (count - 1.0).max(0.0));
    let mut asked = None;
    for (i, (what, label)) in offers.iter().enumerate() {
        let left = (wide + space::MD).mul_add(i as f32, x - all / 2.0);
        let spot = Rect::from_min_size(pos2(left, top), vec2(wide, space::XXL));
        let pressed = ui
            .scope_builder(egui::UiBuilder::new().max_rect(spot), |ui| {
                controls::button_wide(ui, label, (Tone::Plain, true), wide).clicked()
            })
            .inner;
        if pressed {
            asked = Some(*what);
        }
    }
    asked
}

/// How long the screen has been saying `title`, in seconds. It starts
/// again when the title changes, and when the screen comes back after
/// [`AWAY`] seconds of drawing something else, so a wait doesn't go on from
/// the last time the same words showed, hours ago. Coming back sooner
/// carries on, without the step moving again.
fn since(ui: &Ui, title: &str) -> f64 {
    let now = ui.input(|i| i.time);
    let pass = ui.ctx().cumulative_pass_nr();
    let key = egui::Id::new(title);
    ui.ctx().data_mut(|d| {
        let kept =
            d.get_temp_mut_or_insert_with(egui::Id::new("empty-since"), || (key, now, pass, now));
        let away = pass > kept.2.saturating_add(1) && now - kept.3 > AWAY;
        if kept.0 != key || away {
            *kept = (key, now, pass, now);
        }
        (kept.2, kept.3) = (pass, now);
        now - kept.1
    })
}

/// The clock for a wait `seconds` long: to the second for its first ten
/// minutes, so it never looks stuck, and in whole minutes after that.
pub(super) fn clock(seconds: u64) -> String {
    if seconds < 600 {
        format!("{}:{:02}", seconds.div_euclid(60), seconds.rem_euclid(60))
    } else {
        format!("{} min", seconds.div_euclid(60))
    }
}

/// Your row as the backend last kept it on disk, or None before it kept one.
pub(crate) fn kept(root: &Path) -> Option<Saved> {
    let file = root.join("backend").join("data").join("you.json");
    let raw = std::fs::read_to_string(file).ok()?;
    serde_json::from_str::<Saved>(&raw)
        .ok()
        .filter(|you| you.player.puuid.is_some())
}

/// A board of just your kept row, drawn as your side of a lobby. Its party
/// was the lobby it came from, which is long gone, so that is left off.
pub(crate) fn kept_board(you: &Saved) -> Board {
    let mut player = you.player.clone();
    (player.party, player.stack_guess, player.encounter) = (None, None, None);
    Board {
        state: Some("MENUS".to_owned()),
        state_label: Some("Last Stats".to_owned()),
        self_team: player.team.clone(),
        players: vec![player],
        ..Board::default()
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
    // Round, since the slanted pip is a game's result everywhere else.
    let radius = 6.5;
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
                painter.circle_filled(
                    centre,
                    radius * t.mul_add(1.2, 1.0),
                    colour::ALLY.gamma_multiply(0.35 * (1.0 - t)),
                );
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
    painter.circle(centre, radius, fill, edge);
}

/// The same words with no furniture, for a window too short to hold any.
fn plain(ui: &mut Ui, title: &str, detail: &str) {
    ui.add_space(space::XXL);
    let wide = ui.available_width();
    // Centred and wrapped, as a reason can be longer than the window is wide.
    let mut job = egui::text::LayoutJob::simple(
        detail.to_owned(),
        Face::Body.at(size::BODY),
        colour::TEXT_DIM,
        space::XL.mul_add(-2.0, wide),
    );
    job.halign = egui::Align::Center;
    let sentence = ui.painter().layout_job(job);
    let below = space::XXL + space::SM;
    let (rect, _response) = ui.allocate_exact_size(
        vec2(wide, below + sentence.size().y + space::SM),
        Sense::hover(),
    );
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
    painter.galley(
        pos2(rect.center().x, rect.top() + below),
        sentence,
        colour::TEXT_DIM,
    );
}

#[cfg(test)]
mod tests {
    use super::kept;

    #[test]
    fn your_kept_row_is_read_from_the_backends_folder() {
        let root = std::env::temp_dir().join(format!("overseer-kept-{}", std::process::id()));
        let data = root.join("backend").join("data");
        // A run that failed half way leaves its folder for the next one.
        let _stale = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&data).expect("a temp folder");
        assert!(kept(&root).is_none(), "nothing kept yet");
        std::fs::write(data.join("you.json"), "not json").expect("a write");
        assert!(kept(&root).is_none(), "a broken file reads as nothing");
        std::fs::write(
            data.join("you.json"),
            r#"{"player": {"puuid": "me", "kd": 1.2}, "at": 5.0}"#,
        )
        .expect("a write");
        let you = kept(&root).expect("the row reads");
        assert_eq!(you.player.puuid.as_deref(), Some("me"));
        assert_eq!(you.at, Some(5.0));
        let _gone = std::fs::remove_dir_all(&root);
    }
}
