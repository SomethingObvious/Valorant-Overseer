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

use crate::settings::{Quality, Settings};
use crate::sort::{Direction, Sort};
use crate::view;
use overseer_ui as design;

/// One frame to render: a board, and who is selected.
struct Scene {
    board: Board,
    selected: Option<&'static str>,
    /// How the board is ordered, so the arrow on a heading is in the picture
    /// rather than only in the code.
    sort: Sort,
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
    let mut stats = std::collections::HashMap::new();
    stats.insert(
        "Blue".to_owned(),
        overseer_core::TeamStats {
            avg_rank: Some("Gold 1".to_owned()),
            avg_rank_tier: Some(12),
            avg_kd: Some(1.27),
            avg_win_rate: Some(56.0),
            smurf_count: Some(1),
        },
    );
    Board {
        state: Some("INGAME".to_owned()),
        state_label: Some("in game".to_owned()),
        map: Some("Icebox".to_owned()),
        mode: Some("Competitive".to_owned()),
        side: Some("Defense".to_owned()),
        self_team: Some("Blue".to_owned()),
        score: Some(overseer_core::Score {
            ally: Some(3),
            enemy: Some(1),
            round: Some(5),
        }),
        team_stats: stats,
        players,
        ..Board::default()
    }
}

/// The two accounts the app is for: one on each side, both flagged.
fn flagged() -> Vec<Player> {
    vec![smurf_ally(), smurf_enemy()]
}

/// A low level account on your side with an Ascendant peak and the aim to
/// match, which is the case the whole flag exists for.
fn smurf_ally() -> Player {
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
        hs_pct: Some(36.2),
        form: ["W", "W", "L", "W", "W"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        streak: Some(overseer_core::Streak {
            kind: Some("W".to_owned()),
            count: Some(2),
        }),
        top_agents: vec![overseer_core::TopAgent {
            agent: Some("KAY/O".to_owned()),
            games: Some(42),
        }],
        map_win_rate: Some(overseer_core::MapWinRate {
            win_rate: Some(71.0),
            games: Some(7),
        }),
        encounter: Some(overseer_core::Encounter {
            with_count: Some(3),
            wins_with: Some(2),
            losses_with: Some(1),
            ..overseer_core::Encounter::default()
        }),
        party: Some(overseer_core::Party {
            number: Some(1),
            size: Some(2),
            ..overseer_core::Party::default()
        }),
        weapons: vec![overseer_core::WeaponSkin {
            weapon: Some("Vandal".to_owned()),
            skin: Some(overseer_core::Skin {
                name: Some("Reaver".to_owned()),
            }),
        }],
        smurf: true,
        smurf_reasons: vec![
            "Lvl 50, peak Ascendant 2".to_owned(),
            "36% headshots".to_owned(),
        ],
        ..Player::default()
    }
}

/// The same story on the other side, in a party the app had to infer.
fn smurf_enemy() -> Player {
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
        hs_pct: Some(31.4),
        rr_earned: Some(19),
        form: ["W", "W", "W", "W", "L"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        streak: Some(overseer_core::Streak {
            kind: Some("W".to_owned()),
            count: Some(4),
        }),
        map_win_rate: Some(overseer_core::MapWinRate {
            win_rate: Some(62.0),
            games: Some(13),
        }),
        encounter: Some(overseer_core::Encounter {
            against_count: Some(9),
            wins_against: Some(4),
            losses_against: Some(5),
            ..overseer_core::Encounter::default()
        }),
        stack_guess: Some(overseer_core::StackGuess {
            size: Some(3),
            confidence: Some(88),
            shared: Some(9),
            same: Some(7),
        }),
        smurf: true,
        smurf_reasons: vec!["Lvl 43, peak Immortal 1".to_owned()],
        ..Player::default()
    }
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
            rr_earned: Some(-12),
            hs_pct: Some(24.8),
            form: ["L", "W", "L", "L", "W"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            map_win_rate: Some(overseer_core::MapWinRate {
                win_rate: Some(48.0),
                games: Some(21),
            }),
            party: Some(overseer_core::Party {
                number: Some(1),
                size: Some(2),
                ..overseer_core::Party::default()
            }),
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
            form: ["L", "L", "L"].iter().map(|s| (*s).to_owned()).collect(),
            streak: Some(overseer_core::Streak {
                kind: Some("L".to_owned()),
                count: Some(3),
            }),
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
    view::snapshot(
        ui,
        &scene.board,
        scene.selected,
        &Settings::default(),
        &scene.sort,
    );
}

/// Every frame worth pinning: the three layouts, a sorted board, and the
/// screen a new install opens on. Data rather than test, so that adding one
/// is adding a line here.
fn scenes() -> [(&'static str, egui::Vec2, Scene); 6] {
    [
        (
            "wide",
            vec2(1200.0, 340.0),
            Scene {
                board: sample(),
                selected: Some("SilentEnt#GG"),
                sort: Sort::default(),
                ready: true,
            },
        ),
        // Sorted by K/D, best first, which is the question a heading gets
        // clicked to answer. The arrow belongs in a picture somebody looks
        // at rather than only in a unit test.
        (
            "sorted",
            vec2(1200.0, 300.0),
            Scene {
                board: sample(),
                selected: Some("NeonLock#VAL"),
                sort: Sort {
                    column: Some("k/d".to_owned()),
                    direction: Some(Direction::Down),
                },
                ready: true,
            },
        ),
        // Sorted by K/D, best first, which is the question a heading gets
        // clicked to answer. The arrow belongs in a picture somebody looks
        // at rather than only in a unit test.
        (
            "sorted",
            vec2(1200.0, 300.0),
            Scene {
                board: sample(),
                selected: Some("NeonLock#VAL"),
                sort: Sort {
                    column: Some("k/d".to_owned()),
                    direction: Some(Direction::Down),
                },
                ready: true,
            },
        ),
        (
            "normal",
            vec2(860.0, 340.0),
            Scene {
                board: sample(),
                selected: Some("Day#9932"),
                sort: Sort::default(),
                ready: true,
            },
        ),
        (
            "compact",
            vec2(460.0, 320.0),
            Scene {
                board: sample(),
                selected: Some("Day#9932"),
                sort: Sort::default(),
                ready: true,
            },
        ),
        (
            "waiting",
            vec2(860.0, 240.0),
            Scene {
                board: Board::default(),
                selected: None,
                sort: Sort::default(),
                ready: true,
            },
        ),
    ]
}

#[test]
fn every_layout_is_unchanged() {
    // Wide enough for the panel at its full width, then the middle layout,
    // then narrow enough that the board is alone and shedding columns. The
    // three are the whole responsive story, so all three are pinned.
    let scenes = scenes();

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
                sort: Sort::default(),
                ready: false,
            },
        );
    design::install_fonts(&harness.ctx);
    harness.ctx.set_style_of(egui::Theme::Dark, design::style());
    harness.run();

    for (name, size, scene) in scenes {
        harness.set_size(size);
        *harness.state_mut() = scene;
        // Run until nothing is asking for another frame, so the image is of
        // a settled window rather than of a fade halfway through.
        harness.run();
        let mut frames = 0;
        while harness.ctx.has_requested_repaint() && frames < 120 {
            harness.run();
            frames += 1;
        }
        harness.snapshot(name);
    }

    let mut results = settings_shot();
    results.extend(harness.take_snapshot_results());
    if let Err(failed) = results.into_result() {
        panic!("{failed}");
    }
}

/// The settings screen, which is where every switch in the app lives and is
/// therefore the screen most likely to drift out of order unnoticed.
fn settings_shot() -> egui_kittest::SnapshotResults {
    // Same first frame rule as the board: fonts land on the frame after they
    // are installed, so the first one draws nothing.
    let mut shot = Harness::builder()
        .with_size(vec2(880.0, 460.0))
        .build_ui_state(
            |ui, state: &mut (bool, Settings)| {
                if state.0 {
                    view::settings(ui, &mut state.1, Quality::Rich, false);
                }
            },
            (false, Settings::default()),
        );
    design::install_fonts(&shot.ctx);
    shot.ctx.set_style_of(egui::Theme::Dark, design::style());
    shot.run();
    shot.state_mut().0 = true;
    shot.run();
    shot.run();
    shot.snapshot("settings");
    // Two harnesses means two sets of results, and kittest checks that every
    // set was looked at. The caller merges them into one report.
    shot.take_snapshot_results()
}
