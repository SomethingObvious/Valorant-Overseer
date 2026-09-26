//! The performance budget, as tests rather than as intentions.
//!
//! Two things are worth pinning, and neither of them is a stopwatch. A timing
//! assertion in a debug build measures the build, and a timing assertion
//! anywhere measures whatever else the machine was doing, so it fails on a
//! busy afternoon and teaches everybody to ignore it.
//!
//! What is worth pinning is **work**. A frame that asks for another frame
//! when nothing changed is the difference between an app that costs nothing
//! while you play and one that costs a core. A frame that tessellates twice
//! as many shapes as it used to is somebody having added a shadow to every
//! row. Both are deterministic, so both can be a hard number.
//!
//! The stopwatch still exists, in `--perf` on the binary, which prints cold
//! start and frame times for the real window on the real adapter. It is a
//! report, not a gate, because that is what a stopwatch can honestly be.

#![cfg(test)]

use egui::{Ui, vec2};
use egui_kittest::Harness;
use overseer_core::Board;

use crate::settings::Settings;
use crate::sort::Sort;
use crate::view;
use overseer_ui as design;

/// A ten player board is the worst case the app ever draws, and it comes to
/// 262 shapes: ten rows with a groove, a state bar and up to five result
/// pips each, twelve columns of text, two team headings with their averages,
/// two heading rows, three block surfaces with a shadow apiece, the title bar
/// and the panel.
///
/// The budget is that figure with headroom. It has moved twice: from 160,
/// when the board went from six columns to twelve and grew the form pips,
/// and to here, when every row separator became two strokes rather than one
/// so the rows would read as cut into a surface rather than printed on a
/// page. That is the process working rather than failing. The number makes
/// an increase a decision somebody took, and this comment is where the
/// reason goes. What it is really guarding against is the other kind of
/// increase, where a shadow lands on every row and nothing says so.
const SHAPE_BUDGET: usize = 330;

/// Builds the harness the way the app is built, on a full board.
fn harness() -> Harness<'static, bool> {
    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 400.0))
        .build_ui_state(
            |ui: &mut Ui, ready: &mut bool| {
                if *ready {
                    view::snapshot(
                        ui,
                        view::Shown {
                            board: &board(),
                            selected: Some("Day#9932"),
                            settings: &Settings::default(),
                            sort: &Sort::default(),
                            notes: &mut crate::notes::Notes::default(),
                            career: &crate::career::Career::default(),
                        },
                    );
                }
            },
            false,
        );
    design::install_fonts(&harness.ctx);
    harness.ctx.set_style_of(egui::Theme::Dark, design::style());
    harness.run();
    *harness.state_mut() = true;
    harness.run();
    harness
}

/// Ten players, both teams, which is a full competitive lobby.
fn board() -> Board {
    let mut board = crate::shot::sample();
    while board.players.len() < 10 {
        let mut extra = board.players.first().cloned().unwrap_or_default();
        extra.puuid = Some(format!("filler-{}", board.players.len()));
        extra.name = Some(format!("Filler{}#0000", board.players.len()));
        let side = if board.players.len().is_multiple_of(2) {
            "Blue"
        } else {
            "Red"
        };
        extra.team = Some(side.to_owned());
        extra.is_self = false;
        board.players.push(extra);
    }
    board
}

/// A window nobody is touching must not ask to be drawn again.
///
/// This is the whole idle story in one assertion. egui repaints when
/// something asks it to; if any part of the app asks on every frame, the app
/// runs at the monitor's refresh rate for ever, and the only symptom is a
/// fan. An animation will make this fail while it runs, which is correct: an
/// animation is a reason to draw, and the test will be given a still frame to
/// look at when there is one.
#[test]
fn a_still_window_asks_for_nothing() {
    let mut harness = harness();
    // Animations are a reason to draw, so the window is given time to finish
    // whatever it started before being asked whether it is still asking. If
    // it never stops, that is the bug this test is for.
    let mut frames = 0;
    while harness.ctx.has_requested_repaint() && frames < 120 {
        harness.run();
        frames += 1;
    }
    assert!(frames < 120, "the board never stopped asking for frames");
    assert!(
        !harness.ctx.has_requested_repaint(),
        "the board asked for another frame with nothing to draw"
    );
}

/// A full board fits in its shape budget.
#[test]
fn a_full_board_stays_inside_its_shape_budget() {
    let mut harness = harness();
    harness.run();
    let shapes = harness.ctx.run_ui(egui::RawInput::default(), |ui| {
        view::snapshot(
            ui,
            view::Shown {
                board: &board(),
                selected: Some("Day#9932"),
                settings: &Settings::default(),
                sort: &Sort::default(),
                notes: &mut crate::notes::Notes::default(),
                career: &crate::career::Career::default(),
            },
        );
    });
    let count = shapes.shapes.len();
    let mut shapes = shapes;
    shapes.textures_delta.clear();
    assert!(
        count <= SHAPE_BUDGET,
        "a full board now draws {count} shapes, and the budget is {SHAPE_BUDGET}"
    );
    // A budget nothing is near is not a budget. If this fires, the board got
    // much cheaper and the number should come down with it.
    assert!(
        count * 2 >= SHAPE_BUDGET,
        "a full board draws {count} shapes against a budget of {SHAPE_BUDGET}, which is no longer a budget"
    );
}

/// The overlay draws exactly the height it was placed for.
///
/// It is placed once, from this constant, and then only resized, because a
/// window moved after it is shown stops being drawn here. When the constant
/// was short of what the board really drew, a bottom corner grew past where
/// it had been put.
#[test]
fn the_overlay_is_as_tall_as_it_was_placed_for() {
    let ctx = egui::Context::default();
    design::install_fonts(&ctx);
    ctx.set_style_of(egui::Theme::Dark, design::style());
    // Everybody seen: a player Riot said nothing about is a short row, and
    // the overlay is placed for a full one.
    let mut board = board();
    for player in &mut board.players {
        player.name.get_or_insert_with(|| "Seen#0000".to_owned());
    }
    let notes = crate::notes::Notes::default();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(crate::overlay::WIDTH, 800.0),
        )),
        ..Default::default()
    };
    let mut drew = 0.0;
    for _frame in 0..2 {
        let mut output = ctx.run_ui(input(), |ui| {
            drew = crate::board::draw(
                ui,
                &crate::board::Scene {
                    board: &board,
                    sort: &Sort::default(),
                    filter: "",
                    selected: None,
                    notes: &notes,
                    hidden: &[],
                    enemies_first: true,
                    place: crate::board::Place::Overlay,
                    still: true,
                    since: 10.0,
                },
            )
            .drew;
        });
        output.textures_delta.clear();
    }
    assert!(
        (drew - crate::board::OVERLAY_HEIGHT).abs() < 0.5,
        "the overlay drew {drew} points and was placed for {}",
        crate::board::OVERLAY_HEIGHT
    );
}
