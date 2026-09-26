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

use crate::{app, design};

/// A ten player board is the worst case the app ever draws. It came to 128
/// shapes when this was written: ten rows with a rule and a state bar each,
/// two team headings, two heading rows, the header, and the panel.
///
/// The budget is that figure with a quarter of headroom. Raising it is
/// allowed and sometimes right; raising it without noticing is the thing
/// being prevented, because at sixty frames a second twice the shapes is
/// twice the tessellation and twice the vertex upload, for something nobody
/// asked for. The first number here was a guess at 260 and the test below
/// rejected it for not being a budget at all.
const SHAPE_BUDGET: usize = 160;

/// Builds the harness the way the app is built, on a full board.
fn harness() -> Harness<'static, bool> {
    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 400.0))
        .build_ui_state(
            |ui: &mut Ui, ready: &mut bool| {
                if *ready {
                    app::snapshot_view(ui, &board(), Some("Day#9932"));
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
    let harness = harness();
    // Two more frames, so that anything with a one frame delay has had its
    // chance to ask.
    let mut harness = harness;
    harness.run();
    harness.run();
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
        app::snapshot_view(ui, &board(), Some("Day#9932"));
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
