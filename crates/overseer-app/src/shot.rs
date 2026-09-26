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

use crate::career::Career;
use crate::notes::{Note, Notes};
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
    /// What has been written about the people on the board.
    notes: Notes,
    /// The selected player's history, which is the tallest thing the panel
    /// draws and the only part of it with a chart in.
    career: Career,
    /// False on the very first frame. Fonts only materialise once a frame has
    /// run, and the app names its own font families on every line it draws,
    /// so the first frame installs them and draws nothing rather than laying
    /// out a glyph in a family that is not bound yet.
    ready: bool,
}

impl Scene {
    /// The sample board, with somebody selected and nothing else set.
    fn of(selected: Option<&'static str>) -> Self {
        Self {
            board: sample(),
            selected,
            sort: Sort::default(),
            notes: Notes::default(),
            career: Career::default(),
            ready: true,
        }
    }

    /// No board at all, which is what the app opens on.
    fn empty() -> Self {
        Self {
            board: Board::default(),
            selected: None,
            ..Self::of(None)
        }
    }

    /// Ordered by a heading somebody clicked.
    fn sorted(self, sort: Sort) -> Self {
        Self { sort, ..self }
    }

    /// With something written about one of them.
    fn noted(self, notes: Notes) -> Self {
        Self { notes, ..self }
    }

    /// With the selected player's history in hand.
    fn lived(self, career: Career) -> Self {
        Self { career, ..self }
    }
}

/// A board with the shapes worth looking at in one frame: a flagged low level
/// account, an unranked one, a name too long for its column, a player with
/// nothing filled in at all, and us.
pub(crate) fn sample() -> Board {
    let mut players = flagged();
    players.extend(plain());
    players.extend(more_allies());
    players.extend(more_enemies());
    players.sort_by_key(|p| p.team.clone());
    let mut stats = std::collections::HashMap::new();
    stats.insert(
        "Blue".to_owned(),
        overseer_core::TeamStats {
            avg_rank: Some("Gold 1".to_owned()),
            avg_rank_tier: Some(12.6),
            avg_kd: Some(1.27),
            avg_win_rate: Some(56.0),
            smurf_count: Some(1),
        },
    );
    stats.insert(
        "Red".to_owned(),
        overseer_core::TeamStats {
            avg_rank: Some("Gold 3".to_owned()),
            avg_rank_tier: Some(14.2),
            avg_kd: Some(1.11),
            avg_win_rate: Some(49.0),
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

/// The other five, so the board under test is a real lobby.
///
/// Five and five, because every question about this layout — does a column
/// shed at the right width, does a party bracket read, does the eye find the
/// one dangerous account — is a question about ten rows, and four rows
/// answer none of them. Two of these are a duo on the enemy side, which is
/// the grouping the gutter has to draw.
/// The three on our side who are simply playing the game.
fn more_allies() -> Vec<Player> {
    vec![
        Player {
            puuid: Some("h".to_owned()),
            name: Some("DriftFrag#RR".to_owned()),
            team: Some("Blue".to_owned()),
            agent: Some("Reyna".to_owned()),
            agent_color: Some("#D1548C".to_owned()),
            rank: Some("Gold 2".to_owned()),
            rank_tier: Some(13),
            rr: Some(66),
            kd: Some(1.44),
            win_rate: Some(41.0),
            games: Some(103),
            level: Some(212),
            hs_pct: Some(29.5),
            form: form("WLWLW"),
            ..Player::default()
        },
        Player {
            puuid: Some("i".to_owned()),
            name: Some("SageDiff#EUW".to_owned()),
            team: Some("Blue".to_owned()),
            agent: Some("Viper".to_owned()),
            agent_color: Some("#27AF75".to_owned()),
            rank: Some("Bronze 3".to_owned()),
            rank_tier: Some(8),
            rr: Some(56),
            kd: Some(0.72),
            win_rate: Some(59.0),
            games: Some(98),
            level: Some(77),
            hs_pct: Some(23.8),
            form: form("WLWLW"),
            ..Player::default()
        },
    ]
}

/// The other side, including the duo the gutter has to bracket.
fn more_enemies() -> Vec<Player> {
    vec![
        Player {
            puuid: Some("f".to_owned()),
            name: Some("GhostDash#OCE".to_owned()),
            team: Some("Red".to_owned()),
            agent: Some("Phoenix".to_owned()),
            agent_color: Some("#F5955B".to_owned()),
            rank: Some("Gold 3".to_owned()),
            rank_tier: Some(14),
            rr: Some(68),
            kd: Some(1.71),
            win_rate: Some(49.0),
            games: Some(104),
            level: Some(406),
            hs_pct: Some(31.7),
            form: form("WLWLW"),
            party: Some(duo()),
            ..Player::default()
        },
        Player {
            puuid: Some("g".to_owned()),
            name: Some("FrostSpike#VAL".to_owned()),
            team: Some("Red".to_owned()),
            agent: Some("Yoru".to_owned()),
            agent_color: Some("#5A9FE1".to_owned()),
            rank: Some("Platinum 1".to_owned()),
            rank_tier: Some(15),
            rr: Some(70),
            kd: Some(1.54),
            win_rate: Some(45.0),
            games: Some(105),
            level: Some(134),
            hs_pct: Some(30.3),
            form: form("WLWLW"),
            party: Some(duo()),
            ..Player::default()
        },
        Player {
            puuid: Some("j".to_owned()),
            name: Some("DriftAim#GG".to_owned()),
            team: Some("Red".to_owned()),
            agent: Some("Skye".to_owned()),
            agent_color: Some("#6AE2AF".to_owned()),
            rank: Some("Gold 1".to_owned()),
            rank_tier: Some(12),
            rr: Some(64),
            kd: Some(0.93),
            win_rate: Some(52.0),
            games: Some(102),
            level: Some(181),
            hs_pct: Some(25.4),
            form: form("WLWLW"),
            ..Player::default()
        },
    ]
}

/// Two accounts Riot says are queued together.
fn duo() -> overseer_core::Party {
    overseer_core::Party {
        id: Some("p1".to_owned()),
        color: Some("#5A9FE1".to_owned()),
        number: Some(1),
        size: Some(2),
    }
}

/// A run of results, newest first, written the way a person would say it.
fn form(results: &str) -> Vec<String> {
    results.chars().map(|c| c.to_string()).collect()
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
        agent_color: Some("#4A6B8A".to_owned()),
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
        agent_color: Some("#D1548C".to_owned()),
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
            agent_color: Some("#C9A227".to_owned()),
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
            agent_color: Some("#6F4ACC".to_owned()),
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
fn draw(ui: &mut Ui, scene: &mut Scene) {
    view::snapshot(
        ui,
        view::Shown {
            board: &scene.board,
            selected: scene.selected,
            settings: &Settings::default(),
            sort: &scene.sort,
            notes: &mut scene.notes,
            career: &scene.career,
        },
    );
}

/// A history to draw the charts from.
///
/// Built out of the JSON the backend actually sends rather than out of struct
/// literals, so the picture and the parser are tested by the same fixture.
/// Eight matches, a promotion in the middle of them, and a K/D that goes both
/// ways: the three things the two charts have to get right.
fn a_history() -> Career {
    let raw = r#"{
        "source": "demo", "puuid": "b",
        "matches": [
          {"map":"Icebox","mode":"Competitive","result":"Victory","agent":"Chamber",
           "kills":24,"deaths":13,"assists":4,"kd":1.85,"hsPct":33,
           "rrDelta":21,"rrAfter":36,"tierAfter":18,"rankAfter":"Diamond 3"},
          {"map":"Lotus","mode":"Competitive","result":"Victory","agent":"Chamber",
           "kills":19,"deaths":15,"assists":6,"kd":1.27,"hsPct":27,
           "rrDelta":18,"rrAfter":15,"tierAfter":18,"rankAfter":"Diamond 3"},
          {"map":"Ascent","mode":"Competitive","result":"Defeat","agent":"Jett",
           "kills":12,"deaths":18,"assists":2,"kd":0.67,"hsPct":19,
           "rrDelta":-17,"rrAfter":97,"tierAfter":17,"rankAfter":"Diamond 2"},
          {"map":"Bind","mode":"Competitive","result":"Victory","agent":"Chamber",
           "kills":21,"deaths":16,"assists":3,"kd":1.31,"hsPct":29,
           "rrDelta":20,"rrAfter":114,"tierAfter":17,"rankAfter":"Diamond 2"},
          {"map":"Split","mode":"Competitive","result":"Defeat","agent":"Chamber",
           "kills":14,"deaths":19,"assists":7,"kd":0.74,"hsPct":22,
           "rrDelta":-15,"rrAfter":94,"tierAfter":17,"rankAfter":"Diamond 2"},
          {"map":"Haven","mode":"Competitive","result":"Victory","agent":"Jett",
           "kills":26,"deaths":14,"assists":1,"kd":1.86,"hsPct":35,
           "rrDelta":22,"rrAfter":109,"tierAfter":17,"rankAfter":"Diamond 2"},
          {"map":"Sunset","mode":"Unrated","result":"Defeat","agent":"Clove",
           "kills":9,"deaths":17,"assists":8,"kd":0.53,"hsPct":15},
          {"map":"Pearl","mode":"Competitive","result":"Victory","agent":"Chamber",
           "kills":18,"deaths":15,"assists":5,"kd":1.2,"hsPct":26,
           "rrDelta":16,"rrAfter":87,"tierAfter":17,"rankAfter":"Diamond 2"}
        ],
        "averages": {"games":8,"wins":5,"winRate":63,"kills":17.9,"deaths":15.9,
                     "assists":4.5,"kd":1.13,"hsPct":26},
        "coPlayers": [
          {"puuid":"a","name":"SilentEnt#GG","sharedMatches":5,"agents":["KAY/O"],"isParty":true},
          {"puuid":"d","name":"NeonLock#VAL","sharedMatches":2,"agents":["Clove"],"isParty":false}
        ],
        "topGuns": [
          {"name":"Vandal","kills":92,"share":58},
          {"name":"Operator","kills":31,"share":19},
          {"name":"Sheriff","kills":18,"share":11},
          {"name":"Ghost","kills":12,"share":8}
        ],
        "forceHabit": {"forced":5,"chances":7,"pct":71},
        "bonusBuys": [{"name":"Spectre","rounds":4,"share":57}],
        "bonusRounds": 7
    }"#;
    Career::Have {
        puuid: "b".to_owned(),
        profile: Box::new(serde_json::from_str(raw).expect("the fixture is the wire format")),
    }
}

/// A note about somebody on the sample board, so the panel's boxes and the
/// mark on their row are both in a picture.
fn remembered() -> Notes {
    let mut notes = Notes::default();
    notes.set(
        "b",
        Note {
            text: "Plays for picks early, then hides. Worth watching the flank.".to_owned(),
            tags: vec!["flanks".to_owned(), "duo".to_owned()],
            name: "Day#9932".to_owned(),
        },
    );
    notes
}

/// Every frame worth pinning: the three layouts, a sorted board, and the
/// screen a new install opens on. Data rather than test, so that adding one
/// is adding a line here.
fn scenes() -> [(&'static str, egui::Vec2, Scene); 7] {
    [
        ("wide", vec2(1200.0, 480.0), Scene::of(Some("SilentEnt#GG"))),
        // Sorted by K/D, best first, which is the question a heading gets
        // clicked to answer. The arrow belongs in a picture somebody looks
        // at rather than only in a unit test.
        (
            "sorted",
            vec2(1200.0, 480.0),
            Scene::of(Some("NeonLock#VAL")).sorted(Sort {
                column: Some("k/d".to_owned()),
                direction: Some(Direction::Down),
            }),
        ),
        // A player you have written about: the mark on their row, and the
        // boxes in the panel with something in them.
        (
            "noted",
            vec2(1200.0, 480.0),
            Scene::of(Some("Day#9932")).noted(remembered()),
        ),
        ("normal", vec2(860.0, 480.0), Scene::of(Some("Day#9932"))),
        ("compact", vec2(460.0, 460.0), Scene::of(Some("Day#9932"))),
        // A full history, which is the only thing in the app with a chart in
        // it and therefore the only thing a number alone cannot check.
        (
            "career",
            vec2(1200.0, 1240.0),
            Scene::of(Some("Day#9932")).lived(a_history()),
        ),
        ("waiting", vec2(860.0, 240.0), Scene::empty()),
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
                ready: false,
                ..Scene::of(None)
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
        .with_size(vec2(880.0, 1020.0))
        .build_ui_state(
            |ui, state: &mut (bool, Settings)| {
                if state.0 {
                    view::settings(
                        ui,
                        &mut state.1,
                        Quality::Rich,
                        false,
                        // Both broken, because a settings screen that has
                        // only ever been drawn with nothing wrong is a
                        // settings screen nobody has checked the wrapping of.
                        view::Trouble {
                            hotkey: Some("Ctrl+Alt+O is taken: hotkey already registered"),
                            tray: Some("assets/overseer.ico: not found"),
                        },
                    );
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
