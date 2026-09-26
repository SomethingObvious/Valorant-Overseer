//! The look, as a test.
//!
//! The board is rendered offscreen through wgpu and compared against a PNG in
//! `tests/snapshots`. A change to a colour, a column width or a row height
//! fails here with a difference image beside it, which is the whole reason this
//! stack was chosen over a web view: the appearance is a test that fails rather
//! than a thing somebody remembers to look at.
//!
//! Run `UPDATE_SNAPSHOTS=1 cargo test` after a deliberate change, then look at
//! the new image before committing it.
//!
//! One test, one harness, on purpose. Each harness builds its own wgpu device,
//! and two of those at once takes the driver on this machine down with an access
//! violation rather than a message.

#![cfg(test)]

use egui::{CentralPanel, Ui};
use egui_kittest::Harness;
use overseer_core::{Board, Player};

use crate::{app, theme};

/// Which frame the harness is drawing.
enum Scene {
    /// A full board, with the shapes worth looking at.
    Board(Board),
    /// Nothing yet, which is the first thing a new install shows.
    Waiting,
}

/// A board with the shapes worth looking at in one frame: a flagged low level
/// account, an unranked one, a name too long for its column, a player with
/// nothing filled in at all, and us.
fn sample() -> Board {
    let players = vec![
        Player {
            name: Some("SilentEnt#GG".to_owned()),
            team: Some("Blue".to_owned()),
            agent: Some("KAY/O".to_owned()),
            rank: Some("Diamond 3".to_owned()),
            rank_tier: Some(20),
            kd: Some(1.90),
            win_rate: Some(64.0),
            level: Some(50),
            smurf: true,
            smurf_reasons: vec!["Lvl 50, peak Ascendant 2".to_owned()],
            ..Player::default()
        },
        Player {
            name: Some("Day#9932".to_owned()),
            team: Some("Blue".to_owned()),
            agent: Some("Chamber".to_owned()),
            rank: Some("Gold 2".to_owned()),
            rank_tier: Some(13),
            kd: Some(0.91),
            win_rate: Some(52.0),
            level: Some(154),
            is_self: true,
            ..Player::default()
        },
        Player {
            name: Some("AVeryLongNameIndeed#0000".to_owned()),
            team: Some("Blue".to_owned()),
            agent: Some("Astra".to_owned()),
            rank: Some("Unranked".to_owned()),
            rank_tier: Some(0),
            level: Some(12),
            ..Player::default()
        },
        Player {
            name: Some("NeonLock#VAL".to_owned()),
            team: Some("Red".to_owned()),
            agent: Some("Clove".to_owned()),
            rank: Some("Immortal 1".to_owned()),
            rank_tier: Some(24),
            kd: Some(1.79),
            win_rate: Some(64.0),
            level: Some(43),
            smurf: true,
            smurf_reasons: vec!["36% headshots".to_owned()],
            ..Player::default()
        },
        Player {
            team: Some("Red".to_owned()),
            ..Player::default()
        },
    ];
    Board {
        state: Some("INGAME".to_owned()),
        state_label: Some("IN GAME".to_owned()),
        map: Some("Icebox".to_owned()),
        mode: Some("Competitive".to_owned()),
        side: Some("Defense".to_owned()),
        self_team: Some("Blue".to_owned()),
        players,
    }
}

/// Draws what the window draws, without the window: the same functions, the
/// same style, a fixed board.
fn draw(ui: &mut Ui, scene: &Scene) {
    ui.ctx().set_style_of(egui::Theme::Dark, theme::style());
    CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::INK))
        .show(ui, |ui| match scene {
            Scene::Board(board) => app::snapshot_body(ui, board),
            Scene::Waiting => app::snapshot_body(ui, &Board::default()),
        });
}

#[test]
fn the_look_is_unchanged() {
    let mut scene = Scene::Board(sample());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 220.0))
        .build_ui_state(|ui, state: &mut Scene| draw(ui, state), scene);

    harness.run();
    harness.snapshot("board");

    scene = Scene::Waiting;
    *harness.state_mut() = scene;
    harness.run();
    harness.snapshot("waiting");
}
