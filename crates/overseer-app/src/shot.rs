//! Every layout rendered offscreen through wgpu and compared with a PNG in
//! `tests/snapshots`. After a deliberate change, run `UPDATE_SNAPSHOTS=1 cargo
//! test` and look at the new images before committing. The pinned pictures
//! all come from one test so their wgpu devices never run in parallel, since
//! two at once crash this machine's driver with an access violation.

#![cfg(test)]

use egui::{Ui, vec2};
use egui_kittest::Harness;
use overseer_core::{Board, Bridge, Lineup, Player};

use crate::career::Career;
use crate::history::History;
use crate::lineups::Lineups;
use crate::notes::{Note, Notes};
use crate::settings::{Quality, Settings};
use crate::sort::{Direction, Sort};
use crate::view;
use overseer_ui as design;

mod stress;

/// One frame to render: a board, and who is selected.
struct Scene {
    board: Board,
    selected: Option<&'static str>,
    /// How the board is ordered, which puts the sort arrow in the picture.
    sort: Sort,
    /// What has been written about the people on the board.
    notes: Notes,
    /// The selected player's history.
    career: Career,
    /// False on the first frame, which draws nothing: `set_fonts` lands a
    /// frame late, and text in a family that is not bound yet panics inside
    /// epaint.
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

    /// On a board of its own.
    fn with(self, board: Board) -> Self {
        Self { board, ..self }
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
            rank: Some("Gold 1".to_owned()),
            kd: Some(1.27),
            win_rate: Some(56.0),
        },
    );
    stats.insert(
        "Red".to_owned(),
        overseer_core::TeamStats {
            rank: Some("Gold 3".to_owned()),
            kd: Some(1.11),
            win_rate: Some(49.0),
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
        session: Some(overseer_core::Session {
            net: Some(37),
            points: ["W", "W", "L", "W", "L", "W"]
                .iter()
                .enumerate()
                .map(|(i, result)| overseer_core::SessionPoint {
                    map: Some("Icebox".to_owned()),
                    result: Some(if *result == "W" { "Victory" } else { "Defeat" }.to_owned()),
                    delta: Some(if *result == "W" { 19 } else { -16 }),
                    rr: Some(40 + i64::try_from(i).unwrap_or(0) * 6),
                })
                .collect(),
        }),
        players,
        ..Board::default()
    }
}

/// Two more on our side, because column shedding and party brackets only show
/// up in a full lobby of ten.
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
            // Two games behind this K/D, so it is drawn as a thin sample.
            kd: Some(2.93),
            win_rate: Some(52.0),
            games: Some(102),
            level: Some(181),
            hs_pct: Some(25.4),
            form: form("WW"),
            peak_rank: Some("Diamond 2".to_owned()),
            peak_rank_tier: Some(19),
            peak_act: Some("V25 Act 2".to_owned()),
            auto_tags: vec![
                overseer_core::AutoTag {
                    tag: Some("marshal".to_owned()),
                    why: Some(
                        "Carried the Marshal in 9 of 44 rounds over their last 2 games".to_owned(),
                    ),
                    kind: Some("Weapon".to_owned()),
                    ..overseer_core::AutoTag::default()
                },
                overseer_core::AutoTag {
                    tag: Some("saves".to_owned()),
                    why: Some("Saved after all 3 lost pistol rounds".to_owned()),
                    kind: Some("Economy".to_owned()),
                    ..overseer_core::AutoTag::default()
                },
            ],
            ..Player::default()
        },
    ]
}

/// The party Riot says two enemy accounts queued in.
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
            "Level 50, peak Ascendant 2".to_owned(),
            "36% headshots".to_owned(),
        ],
        auto_tags: vec![
            overseer_core::AutoTag {
                tag: Some("outlaw".to_owned()),
                why: Some(
                    "Carried the Outlaw in 22 of 104 rounds over their last 5 games".to_owned(),
                ),
                kind: Some("Weapon".to_owned()),
                ..overseer_core::AutoTag::default()
            },
            overseer_core::AutoTag {
                tag: Some("forces".to_owned()),
                why: Some("Bought a gun after 3 of 4 lost pistol rounds".to_owned()),
                kind: Some("Economy".to_owned()),
                ..overseer_core::AutoTag::default()
            },
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
        smurf_reasons: vec!["Level 43, peak Immortal 1".to_owned()],
        auto_tags: vec![
            overseer_core::AutoTag {
                tag: Some("op".to_owned()),
                why: Some(
                    "Carried the Operator in 31 of 110 rounds over their last 5 games".to_owned(),
                ),
                means: Some("Plays the Operator a lot. Expect it on long angles.".to_owned()),
                kind: Some("weapon".to_owned()),
            },
            overseer_core::AutoTag {
                tag: Some("entry".to_owned()),
                why: Some("In the round's first fight 36 times in 110 rounds, won 24".to_owned()),
                kind: Some("Duels".to_owned()),
                ..overseer_core::AutoTag::default()
            },
            overseer_core::AutoTag {
                tag: Some("hot".to_owned()),
                why: Some("Won their last 4".to_owned()),
                kind: Some("Form".to_owned()),
                ..overseer_core::AutoTag::default()
            },
            overseer_core::AutoTag {
                tag: Some("forces".to_owned()),
                why: Some("Bought a gun after 4 of 5 lost pistol rounds".to_owned()),
                kind: Some("Economy".to_owned()),
                ..overseer_core::AutoTag::default()
            },
        ],
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

/// Draws what the window draws, on a fixed board.
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

/// A history for the charts, written as the backend's JSON so the parser is
/// tested by the same fixture. Its rating crosses between Diamond 2 and 3
/// three times, in Riot's own numbers, and its K/D goes both ways.
fn a_history() -> Career {
    let raw = r#"{
        "source": "demo", "puuid": "b",
        "matches": [
          {"map":"Icebox","mode":"Competitive","result":"Victory","agent":"Chamber",
           "kills":24,"deaths":13,"assists":4,"kd":1.85,"hsPct":33,
           "rrDelta":21,"rrAfter":36,"tierAfter":20,"rankAfter":"Diamond 3"},
          {"map":"Lotus","mode":"Competitive","result":"Victory","agent":"Chamber",
           "kills":19,"deaths":15,"assists":6,"kd":1.27,"hsPct":27,
           "rrDelta":18,"rrAfter":15,"tierAfter":20,"rankAfter":"Diamond 3"},
          {"map":"Ascent","mode":"Competitive","result":"Defeat","agent":"Jett",
           "kills":12,"deaths":18,"assists":2,"kd":0.67,"hsPct":19,
           "rrDelta":-17,"rrAfter":97,"tierAfter":19,"rankAfter":"Diamond 2"},
          {"map":"Bind","mode":"Competitive","result":"Victory","agent":"Chamber",
           "kills":21,"deaths":16,"assists":3,"kd":1.31,"hsPct":29,
           "rrDelta":20,"rrAfter":14,"tierAfter":20,"rankAfter":"Diamond 3"},
          {"map":"Split","mode":"Competitive","result":"Defeat","agent":"Chamber",
           "kills":14,"deaths":19,"assists":7,"kd":0.74,"hsPct":22,
           "rrDelta":-15,"rrAfter":94,"tierAfter":19,"rankAfter":"Diamond 2"},
          {"map":"Haven","mode":"Competitive","result":"Victory","agent":"Jett",
           "kills":26,"deaths":14,"assists":1,"kd":1.86,"hsPct":35,
           "rrDelta":22,"rrAfter":9,"tierAfter":20,"rankAfter":"Diamond 3"},
          {"map":"Sunset","mode":"Unrated","result":"Defeat","agent":"Clove",
           "kills":9,"deaths":17,"assists":8,"kd":0.53,"hsPct":15},
          {"map":"Pearl","mode":"Competitive","result":"Victory","agent":"Chamber",
           "kills":18,"deaths":15,"assists":5,"kd":1.2,"hsPct":26,
           "rrDelta":16,"rrAfter":87,"tierAfter":19,"rankAfter":"Diamond 2"}
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

/// Every frame the snapshot test pins, by name and window size.
fn scenes() -> [(&'static str, egui::Vec2, Scene); 8] {
    [
        ("wide", vec2(1200.0, 480.0), Scene::of(Some("SilentEnt#GG"))),
        // Sorted by K/D, best first, so the heading's arrow is in a picture.
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
        // A full history, for the charts no number alone can check.
        (
            "career",
            vec2(1200.0, 1240.0),
            Scene::of(Some("Day#9932")).lived(a_history()),
        ),
        // The screen the app opens on, at the size it opens at: the state
        // plate with the three steps on it.
        ("opening", vec2(860.0, 420.0), Scene::empty()),
        // The same screen in a window too short for the plate, which shows
        // its fallback.
        ("waiting", vec2(860.0, 240.0), Scene::empty()),
    ]
}

#[test]
fn every_layout_is_unchanged() {
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
        // Run until nothing asks for another frame, so the image is not a
        // fade caught halfway.
        harness.run();
        let mut frames = 0;
        while harness.ctx.has_requested_repaint() && frames < 120 {
            harness.run();
            frames += 1;
        }
        harness.snapshot(name);
    }

    let mut results = settings_shot();
    results.extend(overlay_shot());
    results.extend(history_shot());
    results.extend(lineups_shot());
    results.extend(harness.take_snapshot_results());
    if let Err(failed) = results.into_result() {
        panic!("{failed}");
    }
}

/// Three past games for the History snapshot, the newest a win with its
/// scoreboard open, then a loss, then a Swiftplay with no RR.
fn past_games() -> serde_json::Value {
    let line = |name: &str, team: &str, agent: &str, k: u32, d: u32, a: u32, acs: u32| {
        serde_json::json!({
            "puuid": name, "name": name, "team": team, "agent": agent,
            "kills": k, "deaths": d, "assists": a, "acs": acs,
            "adr": (f64::from(acs) * 0.6).round(), "hsPct": 18 + k.rem_euclid(9), "kast": 64 + a.rem_euclid(30),
            "firstBloods": k.rem_euclid(5), "isSubject": name == "SaltiSmySugar#OBAMA",
        })
    };
    serde_json::json!({
        "asked": 5,
        "games": [
            {
                "matchId": "m1", "map": "Haven", "mode": "Competitive",
                "startedAt": NOW_MS - 3 * 3_600_000, "lengthMs": 2_280_000,
                "score": [13, 11], "result": "Victory", "rrDelta": 27, "yourTeam": "Blue",
                "players": [
                    line("SaltiSmySugar#OBAMA", "Blue", "Brimstone", 21, 15, 15, 242),
                    line("NeonLock#VAL", "Red", "Clove", 24, 17, 6, 268),
                    line("Xan#NA1", "Blue", "Jett", 19, 16, 4, 231),
                    line("GhostDash#OCE", "Red", "Phoenix", 18, 18, 7, 214),
                    line("SumsItUp#777", "Blue", "Sova", 15, 14, 11, 198),
                    line("FrostSpike#VAL", "Red", "Yoru", 16, 19, 3, 190),
                    line("Spectre#GG", "Blue", "Killjoy", 13, 15, 6, 171),
                    line("DriftAim#GG", "Red", "Skye", 12, 18, 13, 162),
                    line("Pepsi Man#COLA", "Blue", "Omen", 11, 17, 9, 150),
                    line("Wanderlust#EUW", "Red", "Viper", 10, 19, 8, 139),
                ],
            },
            {
                "matchId": "m2", "map": "Abyss", "mode": "Competitive",
                "startedAt": NOW_MS - 26 * 3_600_000, "lengthMs": 2_040_000,
                "score": [10, 13], "result": "Defeat", "rrDelta": -22, "yourTeam": "Blue",
                "players": [line("SaltiSmySugar#OBAMA", "Blue", "Brimstone", 14, 15, 9, 206)],
            },
            {
                "matchId": "m3", "map": "Corrode", "mode": "Swiftplay",
                "startedAt": NOW_MS - 3 * 86_400_000, "lengthMs": 720_000,
                "score": [5, 3], "result": "Victory", "yourTeam": "Blue",
                "players": [line("SaltiSmySugar#OBAMA", "Blue", "Brimstone", 10, 6, 6, 378)],
            },
        ],
    })
}

/// The clock the History snapshot reads, so "3 hours ago" stays true.
const NOW_MS: i64 = 1_790_000_000_000;

/// The History screen's snapshot, from a harness of its own.
fn history_shot() -> egui_kittest::SnapshotResults {
    let mut shot = Harness::builder()
        .with_size(vec2(1200.0, 780.0))
        .build_ui_state(
            |ui, state: &mut (bool, History)| {
                if state.0 {
                    let _picked = state.1.show(ui, 5, NOW_MS, true);
                }
            },
            (false, History::showing(past_games())),
        );
    design::install_fonts(&shot.ctx);
    shot.ctx.set_style_of(egui::Theme::Dark, design::style());
    shot.run();
    shot.state_mut().0 = true;
    shot.run();
    shot.run();
    shot.snapshot("history");
    shot.take_snapshot_results()
}

/// Ascent with its real callout positions, an outlined A site, and three
/// lineups, for the Lineups snapshots. The minimap is left out, since it is
/// fetched onto this machine and a picture of it would differ elsewhere.
fn an_atlas() -> serde_json::Value {
    let callouts = [
        ("A Site", 0.3501, 0.1425),
        ("B Site", 0.2855, 0.7373),
        ("A Main", 0.4842, 0.2007),
        ("A Lobby", 0.6029, 0.259),
        ("A Tree", 0.3982, 0.2946),
        ("A Garden", 0.2853, 0.3091),
        ("B Main", 0.405, 0.7121),
        ("B Lobby", 0.7166, 0.6776),
        ("Mid Market", 0.2985, 0.497),
        ("Mid Courtyard", 0.4928, 0.4877),
        ("Mid Catwalk", 0.525, 0.4111),
        ("Mid Link", 0.5143, 0.6175),
    ]
    .map(|(name, x, y)| serde_json::json!({ "name": name, "at": [x, y] }));
    let kit = |name: &str, q: &str| {
        serde_json::json!({ "name": name, "abilities": [
            { "key": "C", "name": "Stim Beacon" }, { "key": "Q", "name": q },
            { "key": "E", "name": "Sky Smoke" }, { "key": "X", "name": "Orbital Strike" },
        ]})
    };
    serde_json::json!({
        "maps": [{ "name": "Ascent", "sites": ["A", "B"], "callouts": callouts }],
        "agents": [kit("Brimstone", "Incendiary"), kit("Viper", "Snake Bite"), kit("Killjoy", "Nanoswarm")],
        "lineups": [
            { "id": "brim-a", "map": "Ascent", "agent": "Brimstone", "ability": "Incendiary",
              "side": "attack", "site": "A", "title": "A Main to Default",
              "notes": "Stand in the corner of A Main, aim at the top of the pillar and throw.",
              "stand": [0.52, 0.22], "land": [0.36, 0.13],
              "clip": { "source": "https://example.com/v", "from": 70.0, "to": 77.0, "volume": 60, "file": "C:/clip.mp4" } },
            { "id": "brim-b", "map": "Ascent", "agent": "Brimstone", "ability": "Incendiary",
              "side": "defense", "site": "B", "title": "B Main Retake", "stand": [0.2, 0.62], "land": [0.36, 0.71] },
            { "id": "kj-a", "map": "Ascent", "agent": "Killjoy", "ability": "Nanoswarm",
              "side": "defense", "site": "A", "title": "A Generator", "stand": [0.3, 0.3], "land": [0.33, 0.17] },
        ],
        "drawings": { "Ascent": [
            { "kind": "circle", "colour": "#FF4655", "a": [0.36, 0.13], "b": [0.40, 0.13] },
            { "kind": "cone", "colour": "#18E5A7", "a": [0.52, 0.45], "b": [0.52, 0.30], "spread": 50.0 },
            { "kind": "rect", "colour": "#4A7BFF", "a": [0.62, 0.62], "b": [0.74, 0.70] },
            { "kind": "oval", "colour": "#A86CFF", "a": [0.24, 0.45], "b": [0.34, 0.52] },
            { "kind": "triangle", "colour": "#FFC845", "a": [0.45, 0.80], "b": [0.45, 0.75] },
            { "kind": "text", "colour": "#F2EFE8", "a": [0.60, 0.40], "b": [0.60, 0.40], "text": "Stack here" },
            { "kind": "brush", "colour": "#FF8A3D", "a": [0.10, 0.70], "b": [0.30, 0.86], "width": 0.008,
              "points": [[0.10, 0.70], [0.14, 0.74], [0.19, 0.76], [0.24, 0.80], [0.27, 0.84], [0.30, 0.86]] },
            { "kind": "textbox", "colour": "#3FD0FF", "a": [0.70, 0.10], "b": [0.95, 0.22],
              "text": "Smoke heaven first, then the molly on default once it lands" },
        ] },
        "tools": { "ytdlp": true, "ffmpeg": true, "ffplay": true },
    })
}

/// Clip settings that leave a clip still, so a snapshot doesn't start a
/// video playing.
fn still_clips() -> crate::lineups::Prefs {
    crate::lineups::Prefs {
        autoplay: false,
        ..crate::lineups::Prefs::default()
    }
}

/// The Lineups screen, browsing with a lineup picked and writing a new one,
/// from a harness of its own.
fn lineups_shot() -> egui_kittest::SnapshotResults {
    let mut results = egui_kittest::SnapshotResults::default();
    for (name, mode) in [
        ("lineups", "browse"),
        ("lineup-edit", "write"),
        ("lineup-draw", "draw"),
        ("lineup-text", "text"),
        ("lineup-any", "any"),
        ("lineup-note", "note"),
    ] {
        let mut shot = Harness::builder()
            .with_size(vec2(1200.0, 820.0))
            .build_ui_state(
                |ui, state: &mut (bool, Lineups, Bridge)| {
                    if state.0 {
                        state.1.show(
                            ui,
                            (&state.2, std::path::Path::new(".")),
                            true,
                            (
                                1.0,
                                still_clips(),
                                crate::lineups::Look {
                                    turn: crate::settings::MapTurn::Drawn,
                                    names: false,
                                    area: None,
                                    agent: "Brimstone",
                                    slow: false,
                                },
                            ),
                        );
                    }
                },
                (
                    false,
                    Lineups::showing(
                        an_atlas(),
                        (mode == "browse").then_some("brim-a"),
                        if mode == "note" { "browse" } else { mode },
                    ),
                    Bridge::start(std::path::Path::new("."), || {}),
                ),
            );
        design::install_fonts(&shot.ctx);
        shot.ctx.set_style_of(egui::Theme::Dark, design::style());
        shot.run();
        shot.state_mut().0 = true;
        shot.run();
        shot.run();
        if mode == "note" {
            // Resting on the A Main pin, which has notes, past the tooltip's delay.
            shot.hover_at(egui::pos2(425.0, 220.0));
            for _ in 0..8 {
                shot.step();
            }
            shot.run();
        }
        shot.snapshot(name);
        results.extend(shot.take_snapshot_results());
    }
    results
}

/// Presses at `from`, moves to `to` in a few steps and lets go there, the way
/// a mouse does.
fn drag(shot: &mut Harness<'_, (bool, Lineups, Bridge)>, from: egui::Pos2, to: egui::Pos2) {
    shot.hover_at(from);
    shot.run();
    shot.drag_at(from);
    shot.run();
    for t in [0.25, 0.5, 0.75, 1.0] {
        shot.hover_at(from.lerp(to, t));
        shot.run();
    }
    shot.drop_at(to);
    shot.run();
}

/// The Lineups screen in `mode`, laid out as its snapshots are, for real
/// pointer events.
fn screen(mode: &str) -> Harness<'static, (bool, Lineups, Bridge)> {
    let mut shot = Harness::builder()
        .with_size(vec2(1200.0, 820.0))
        .build_ui_state(
            |ui, state: &mut (bool, Lineups, Bridge)| {
                if state.0 {
                    state.1.show(
                        ui,
                        (&state.2, std::path::Path::new(".")),
                        true,
                        (
                            1.0,
                            still_clips(),
                            crate::lineups::Look {
                                turn: crate::settings::MapTurn::Drawn,
                                names: false,
                                area: None,
                                agent: "Brimstone",
                                slow: false,
                            },
                        ),
                    );
                }
            },
            (
                false,
                Lineups::showing(an_atlas(), None, mode),
                Bridge::start(std::path::Path::new("."), || {}),
            ),
        );
    design::install_fonts(&shot.ctx);
    shot.ctx.set_style_of(egui::Theme::Dark, design::style());
    shot.run();
    shot.state_mut().0 = true;
    shot.run();
    shot.run();
    shot
}

/// The trim bar on the clip from the `lineup-edit` snapshot, 2:31 to 2:38,
/// which spans 2:24 to 2:45 at about 16 pixels a second. Dragging a handle
/// writes its time in tenths, and the other end stays put.
#[test]
fn a_drag_on_the_trim_bar_writes_the_time() {
    let mut write = screen("write");
    let trimmed = |shot: &Harness<'_, (bool, Lineups, Bridge)>| {
        let (from, to) = shot.state().1.trimmed().unwrap();
        (from.to_owned(), to.to_owned())
    };
    assert_eq!(trimmed(&write), ("2:31".to_owned(), "2:38".to_owned()));

    drag(
        &mut write,
        egui::pos2(1060.0, 544.0),
        egui::pos2(1100.0, 544.0),
    );
    let (from, to) = trimmed(&write);
    assert_eq!(from, "2:31", "dragging To moved From");
    let end = to.strip_prefix("2:").and_then(|s| s.parse::<f64>().ok());
    assert!(
        end.is_some_and(|s| (40.0..41.2).contains(&s)),
        "40 pixels right of 2:38 wrote {to}"
    );
    assert!(to.contains('.'), "a drag lands on tenths, not {to}");

    // Dragged past the bar's end, a handle stops there.
    let mut far = screen("write");
    drag(&mut far, egui::pos2(944.0, 544.0), egui::pos2(300.0, 544.0));
    assert_eq!(trimmed(&far), ("2:24".to_owned(), "2:38".to_owned()));
}

/// Real pointer events on the Lineups screen, laid out as its snapshots are.
/// A drag across the map is a whole lineup, a drag from a shape's dot moves
/// just that shape, and a drag anywhere else still draws a new one.
#[test]
fn a_drag_on_the_map_places_a_lineup_and_moves_a_shape() {
    let (from, to) = (egui::pos2(620.0, 300.0), egui::pos2(700.0, 450.0));
    let mut browse = screen("browse");
    drag(&mut browse, from, to);
    assert!(
        browse.state().1.placed().is_none(),
        "browsing, a drag started a lineup"
    );

    let mut add = screen("add");
    drag(&mut add, from, to);
    let Some(Lineup {
        stand: Some([sx, sy]),
        land: Some([lx, ly]),
        ..
    }) = add.state().1.placed()
    else {
        panic!("a drag across the map should place both ends of a lineup");
    };
    assert!(
        (0.08..0.14).contains(&(lx - sx)) && (0.16..0.24).contains(&(ly - sy)),
        "it landed {:?} away from where it was thrown",
        (lx - sx, ly - sy)
    );

    let mut draw = screen("draw");
    let before = draw.state().1.sketched().unwrap().to_vec();
    // The blue rectangle's dot, where the draw snapshot shows it.
    drag(
        &mut draw,
        egui::pos2(543.5, 546.5),
        egui::pos2(603.5, 546.5),
    );
    let after = draw.state().1.sketched().unwrap().to_vec();
    assert_eq!(after.len(), before.len(), "moving a shape drew a new one");
    let (was, now) = (before.get(2).unwrap(), after.get(2).unwrap());
    assert_eq!(now.kind, "rect");
    let (dx, dy) = (now.a[0] - was.a[0], now.a[1] - was.a[1]);
    assert!(
        (0.06..0.10).contains(&dx) && dy.abs() < 0.005,
        "the rectangle moved by {:?}",
        (dx, dy)
    );
    assert!(
        (now.b[0] - was.b[0] - dx).abs() < 1e-5 && (now.b[1] - was.b[1] - dy).abs() < 1e-5,
        "the rectangle changed size"
    );

    drag(
        &mut draw,
        egui::pos2(150.0, 650.0),
        egui::pos2(230.0, 700.0),
    );
    assert_eq!(
        draw.state().1.sketched().unwrap().len(),
        before.len() + 1,
        "a drag away from every dot draws"
    );
}

/// The eraser takes off the drawing a click lands on, and a sweep takes off
/// what it crosses, and neither moves a lineup.
#[test]
fn the_eraser_takes_off_what_it_touches() {
    let mut erase = screen("erase");
    let kinds = |shot: &Harness<'_, (bool, Lineups, Bridge)>| -> Vec<String> {
        let drawn = shot.state().1.sketched().unwrap_or_default();
        drawn.iter().map(|s| s.kind.clone()).collect()
    };
    let before = kinds(&erase);
    // Inside the blue rectangle, a click.
    drag(
        &mut erase,
        egui::pos2(560.0, 560.0),
        egui::pos2(560.0, 560.0),
    );
    let after_click = kinds(&erase);
    assert!(!after_click.contains(&"rect".to_owned()), "{after_click:?}");
    assert_eq!(after_click.len(), before.len() - 1);
    // Across the orange stroke.
    drag(
        &mut erase,
        egui::pos2(160.0, 636.0),
        egui::pos2(240.0, 636.0),
    );
    let after_sweep = kinds(&erase);
    assert!(
        !after_sweep.contains(&"brush".to_owned()),
        "{after_sweep:?}"
    );
    assert_eq!(after_sweep.len(), before.len() - 2);
    assert!(
        erase.state().1.placed().is_none(),
        "the eraser started a lineup"
    );
}

/// The Lineups screen on a real atlas, for looking at by hand: the backend's
/// `lineups.overview()` saved as JSON at `LINEUPS_ATLAS`, drawn into
/// `STRESS_OUT`. Ignored, since the minimaps are only on this machine.
#[test]
#[ignore = "needs LINEUPS_ATLAS and STRESS_OUT"]
fn lineups_live() {
    let (Ok(atlas), Ok(out)) = (std::env::var("LINEUPS_ATLAS"), std::env::var("STRESS_OUT")) else {
        return;
    };
    let atlas: serde_json::Value = serde_json::from_slice(&std::fs::read(atlas).unwrap()).unwrap();
    let picked = atlas
        .pointer("/lineups/0/id")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    for (name, mode) in [
        ("lineups-live", "browse"),
        ("lineup-edit-live", "write"),
        ("lineup-any-live", "any"),
        ("lineup-draw-live", "draw"),
    ] {
        let mut shot = Harness::builder()
            .with_size(vec2(1600.0, 1000.0))
            .build_ui_state(
                |ui, state: &mut (bool, Lineups, Bridge)| {
                    if state.0 {
                        state.1.show(
                            ui,
                            (&state.2, std::path::Path::new(".")),
                            true,
                            (
                                1.0,
                                still_clips(),
                                crate::lineups::Look {
                                    turn: crate::settings::MapTurn::Attack,
                                    names: false,
                                    area: None,
                                    agent: "Brimstone",
                                    slow: false,
                                },
                            ),
                        );
                    }
                },
                (
                    false,
                    Lineups::showing(atlas.clone(), picked.as_deref(), mode),
                    Bridge::start(std::path::Path::new("."), || {}),
                ),
            );
        design::install_fonts(&shot.ctx);
        shot.ctx.set_style_of(egui::Theme::Dark, design::style());
        shot.run();
        shot.state_mut().0 = true;
        shot.run();
        shot.run();
        shot.render()
            .unwrap()
            .save(format!("{out}/{name}.png"))
            .unwrap();
    }
}

/// The overlay as the game sees it: the sample lobby drawn the way
/// `overlay::show` draws it, at the overlay's width and its tallest.
fn overlay_shot() -> egui_kittest::SnapshotResults {
    let mut results = overlay_frame(
        Board {
            win_prob: Some(54.0),
            ..sample()
        },
        "overlay",
    );
    // Agent select, where only your own team is known.
    let full = sample();
    let ours = Board {
        state: Some("PREGAME".to_owned()),
        score: None,
        players: full
            .players
            .iter()
            .filter(|p| p.team == full.self_team)
            .cloned()
            .collect(),
        ..sample()
    };
    results.extend(overlay_frame(ours, "overlay-agent-select"));
    results.extend(greeting_frame());
    results
}

/// The overlay over `board`, saved as the snapshot `name`.
fn overlay_frame(board: Board, name: &str) -> egui_kittest::SnapshotResults {
    let mut shot = Harness::builder()
        .with_size(vec2(crate::overlay::WIDTH, crate::overlay::CEILING))
        .build_ui_state(
            move |ui, ready: &mut bool| {
                if !*ready {
                    return;
                }
                crate::overlay::contents(ui, &mut |ui: &mut Ui| {
                    let corner = ui.max_rect();
                    let _drew = crate::board::draw(
                        ui,
                        &crate::board::Scene {
                            board: &board,
                            sort: &Sort::default(),
                            filter: "",
                            selected: None,
                            notes: &Notes::default(),
                            hidden: &[],
                            enemies_first: true,
                            place: crate::board::Place::Overlay,
                            still: true,
                            since: 10.0,
                        },
                    );
                    let _hide = crate::overlay::hide_button(ui, corner);
                });
            },
            false,
        );
    design::install_fonts(&shot.ctx);
    shot.ctx.set_style_of(egui::Theme::Dark, design::style());
    shot.run();
    *shot.state_mut() = true;
    shot.run();
    shot.run();
    shot.snapshot(name);
    shot.take_snapshot_results()
}

/// The overlay's greeting, saved as its own snapshot.
fn greeting_frame() -> egui_kittest::SnapshotResults {
    let mut greeting = Harness::builder()
        // The harness's own panel keeps 8 points round the edge.
        .with_size(vec2(crate::overlay::WIDTH + 16.0, crate::overlay::GREETING + 16.0))
        .build_ui_state(
            |ui, ready: &mut bool| {
                if *ready {
                    crate::overlay::contents(ui, &mut |ui: &mut Ui| {
                        let _drew = crate::overlay::greeting(ui);
                    });
                }
            },
            false,
        );
    design::install_fonts(&greeting.ctx);
    greeting
        .ctx
        .set_style_of(egui::Theme::Dark, design::style());
    greeting.run();
    *greeting.state_mut() = true;
    greeting.run();
    greeting.snapshot("overlay-greeting");
    greeting.take_snapshot_results()
}

/// The settings screen's snapshot, from a harness of its own.
fn settings_shot() -> egui_kittest::SnapshotResults {
    // Same first frame rule as the board: fonts land on the frame after they
    // are installed, so the first one draws nothing.
    let mut shot = Harness::builder()
        .with_size(vec2(880.0, 2300.0))
        .build_ui_state(
            |ui, state: &mut (bool, Settings)| {
                if state.0 {
                    view::settings(
                        ui,
                        &mut state.1,
                        (
                            Quality::Rich,
                            false,
                            &crate::machine::Machine {
                                memory: Some(16 << 30),
                                threads: 8,
                                card: true,
                            },
                        ),
                        // Both broken, so the longer notes and their
                        // wrapping are in the picture.
                        (
                            view::Trouble {
                                hotkey: Some(&crate::hotkey::Failure::Taken),
                                tray: Some("assets/overseer.ico: not found"),
                            },
                            std::path::Path::new("."),
                            // The proxy isn't running, so the switch says
                            // what turning it on will do.
                            &mut {
                                let mut offline = crate::offline::Offline::default();
                                offline.state = Some(crate::offline::State::default());
                                offline
                            },
                        ),
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
