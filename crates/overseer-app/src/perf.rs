//! The performance budget, as tests.
//!
//! They count work, not time. A timing assertion measures the build and
//! whatever else the machine is busy with, but a frame that asks for another
//! frame, or a board that suddenly draws twice the shapes, is the same on
//! every run.

#![cfg(test)]

use egui::{Ui, vec2};
use egui_kittest::Harness;
use overseer_core::Board;

use crate::settings::Settings;
use crate::sort::Sort;
use crate::view;
use overseer_ui as design;

/// A full ten player board, the most the app ever draws, comes to 212
/// shapes. This leaves headroom.
///
/// Raise it when the design adds shapes on purpose, and say why here. What
/// it catches is the unplanned kind, like a shadow landing on every row.
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

/// A window nobody is touching stops asking to be drawn. If anything asks
/// every frame, the app runs at the monitor's refresh rate for ever and the
/// only symptom is the fan.
#[test]
fn a_still_window_asks_for_nothing() {
    let mut harness = harness();
    // An animation is a reason to draw, so let it finish first.
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
    let mut output = harness.ctx.run_ui(egui::RawInput::default(), |ui| {
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
    output.textures_delta.clear();
    let count = output.shapes.len();
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

/// The overlay draws exactly the height it was placed for. It never moves
/// once shown, so a taller board would grow a bottom corner off its place.
#[test]
fn the_overlay_is_as_tall_as_it_was_placed_for() {
    let ctx = egui::Context::default();
    design::install_fonts(&ctx);
    ctx.set_style_of(egui::Theme::Dark, design::style());
    // Everybody named, because a player Riot said nothing about draws a
    // short row and the overlay is placed for full ones.
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
