//! The look, as a test.
//!
//! Every layout is rendered offscreen through wgpu and compared against a PNG
//! in `tests/snapshots`. A change to a colour, a column width or a row height
//! fails here with a difference image beside it, which is the whole reason
//! this stack was chosen over a web view: the appearance is a test that fails
//! rather than a thing somebody remembers to look at.
//!
//! Run `UPDATE_SNAPSHOTS=1 cargo test` after a deliberate change, then open
//! the image and look at it before committing. The test protects against
//! accidental change; only a person can say whether a deliberate one was any
//! good.
//!
//! One test, one harness, on purpose. Each harness builds its own wgpu device,
//! and two of those at once takes the driver on this machine down with an
//! access violation rather than a message.

#![cfg(test)]

use egui::{Ui, vec2};
use egui_kittest::Harness;
use overseer_core::{Board, Player};

use crate::{app, design};

/// One frame to render: a board, and who is selected.
struct Scene {
    board: Board,
    selected: Option<&'static str>,
    /// False on the very first frame. Fonts only materialise once a frame has
    /// run, and the app names its own font families on every line it draws,
    /// so the first frame installs them and draws nothing rather than laying
    /// out a glyph in a family that is not bound yet.
    ready: bool,
}

/// A board with the shapes worth looking at in one frame: a flagged low level
/// account, an unranked one, a name too long for its column, a player with
/// nothing filled in at all, and us.
pub(crate) fn sample() -> Board {
    let mut players = flagged();
    players.extend(plain());
    players.sort_by_key(|p| p.team.clone());
    Board {
        state: Some("INGAME".to_owned()),
        state_label: Some("in game".to_owned()),
        map: Some("Icebox".to_owned()),
        mode: Some("Competitive".to_owned()),
        side: Some("Defense".to_owned()),
        self_team: Some("Blue".to_owned()),
        players,
    }
}

/// The two accounts the app is for: one on each side, both flagged.
fn flagged() -> Vec<Player> {
    vec![
        Player {
            puuid: Some("a".to_owned()),
            name: Some("SilentEnt#GG".to_owned()),
            team: Some("Blue".to_owned()),
            agent: Some("KAY/O".to_owned()),
            role: Some("Initiator".to_owned()),
            rank: Some("Diamond 3".to_owned()),
            rank_tier: Some(20),
            rr: Some(72),
            peak_rank: Some("Ascendant 3".to_owned()),
            peak_rank_tier: Some(23),
            peak_act: Some("V25 Act 3".to_owned()),
            kd: Some(1.90),
            win_rate: Some(64.0),
            games: Some(118),
            level: Some(50),
            smurf: true,
            smurf_reasons: vec![
                "Lvl 50, peak Ascendant 2".to_owned(),
                "36% headshots".to_owned(),
            ],
            ..Player::default()
        },
        Player {
            puuid: Some("d".to_owned()),
            name: Some("NeonLock#VAL".to_owned()),
            team: Some("Red".to_owned()),
            agent: Some("Clove".to_owned()),
            rank: Some("Immortal 1".to_owned()),
            rank_tier: Some(24),
            rr: Some(97),
            kd: Some(1.79),
            win_rate: Some(64.0),
            games: Some(261),
            level: Some(43),
            smurf: true,
            smurf_reasons: vec!["Lvl 43, peak Immortal 1".to_owned()],
            ..Player::default()
        },
    ]
}

/// The rest of the lobby: us, an unranked account with a name too long for
/// its column, and somebody the backend knows nothing about yet.
fn plain() -> Vec<Player> {
    vec![
        Player {
            puuid: Some("b".to_owned()),
            name: Some("Day#9932".to_owned()),
            team: Some("Blue".to_owned()),
            agent: Some("Chamber".to_owned()),
            role: Some("Sentinel".to_owned()),
            rank: Some("Gold 2".to_owned()),
            rank_tier: Some(13),
            rr: Some(37),
            peak_rank: Some("Gold 3".to_owned()),
            peak_rank_tier: Some(14),
            peak_act: Some("V25 Act 4".to_owned()),
            kd: Some(0.91),
            win_rate: Some(52.0),
            games: Some(118),
            level: Some(154),
            is_self: true,
            ..Player::default()
        },
        Player {
            puuid: Some("c".to_owned()),
            name: Some("AVeryLongNameIndeed#0000".to_owned()),
            team: Some("Blue".to_owned()),
            agent: Some("Astra".to_owned()),
            rank: Some("Unranked".to_owned()),
            rank_tier: Some(0),
            level: Some(12),
            ..Player::default()
        },
        Player {
            puuid: Some("e".to_owned()),
            team: Some("Red".to_owned()),
            ..Player::default()
        },
    ]
}

/// Draws what the window draws, without the window: the same functions, the
/// same fonts, the same style, a fixed board.
///
/// The fonts are installed once against the harness's context rather than
/// here, because `set_fonts` lands on the next frame and a style naming a
/// family that is not bound yet panics inside epaint.
fn draw(ui: &mut Ui, scene: &Scene) {
    app::snapshot_view(ui, &scene.board, scene.selected);
}

#[test]
fn every_layout_is_unchanged() {
    // Wide enough for the panel at its full width, then the middle layout,
    // then narrow enough that the board is alone and shedding columns. The
    // three are the whole responsive story, so all three are pinned.
    let scenes: [(&str, egui::Vec2, Scene); 4] = [
        (
            "wide",
            vec2(1200.0, 340.0),
            Scene {
                board: sample(),
                selected: Some("SilentEnt#GG"),
                ready: true,
            },
        ),
        (
            "normal",
            vec2(860.0, 340.0),
            Scene {
                board: sample(),
                selected: Some("Day#9932"),
                ready: true,
            },
        ),
        (
            "compact",
            vec2(460.0, 320.0),
            Scene {
                board: sample(),
                selected: Some("Day#9932"),
                ready: true,
            },
        ),
        (
            "waiting",
            vec2(860.0, 240.0),
            Scene {
                board: Board::default(),
                selected: None,
                ready: true,
            },
        ),
    ];

    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 340.0))
        .build_ui_state(
            |ui, state: &mut Scene| {
                if state.ready {
                    draw(ui, state);
                }
            },
            Scene {
                board: sample(),
                selected: None,
                ready: false,
            },
        );
    design::install_fonts(&harness.ctx);
    harness.ctx.set_style_of(egui::Theme::Dark, design::style());
    harness.run();

    for (name, size, scene) in scenes {
        harness.set_size(size);
        *harness.state_mut() = scene;
        // Twice: the first pass lays out at the new size, the second draws it.
        harness.run();
        harness.run();
        harness.snapshot(name);
    }
}
