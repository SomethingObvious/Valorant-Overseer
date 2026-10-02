//! Lobbies built to break the layout, rendered to a folder to look at 1:1
//! after a change. Nothing is compared against a snapshot. Run with
//! `STRESS_OUT=<folder> cargo test -p overseer-app stress -- --ignored`.

use egui::{Vec2, vec2};
use egui_kittest::Harness;
use overseer_core::{Board, Encounter, LockProgress, Party, Player, Score, StackGuess, Streak};

use super::{Scene, a_history, draw, form, sample};
use crate::notes::Notes;
use crate::sort::Sort;
use overseer_ui as design;

/// A frame of the window, or of the overlay.
enum Shot {
    Window(Scene),
    Overlay(Board),
    /// The window with the pointer resting at a point, for its tooltips.
    Hover(Scene, egui::Pos2),
}

/// A plain account at a tier, for a fixture to change what it cares about.
fn player(id: &str, name: &str, team: &str, agent: &str, tier: u32) -> Player {
    Player {
        puuid: Some(id.to_owned()),
        name: Some(name.to_owned()),
        team: Some(team.to_owned()),
        agent: Some(agent.to_owned()),
        rank: rank_name(tier),
        rank_tier: (tier >= 3).then_some(tier),
        rr: Some(40),
        kd: Some(1.04),
        hs_pct: Some(22.0),
        win_rate: Some(51.0),
        games: Some(64),
        level: Some(120),
        form: form("WLWLW"),
        ..Player::default()
    }
}

/// What Riot calls a tier.
fn rank_name(tier: u32) -> Option<String> {
    if tier < 3 {
        return None;
    }
    if tier >= 27 {
        return Some("Radiant".to_owned());
    }
    let group = design::rank_group(tier);
    let mut name: Vec<char> = group.chars().collect();
    if let Some(first) = name.first_mut() {
        *first = first.to_ascii_uppercase();
    }
    let division = tier.rem_euclid(3) + 1;
    Some(format!(
        "{} {division}",
        name.into_iter().collect::<String>()
    ))
}

/// A board around these players, in a match, you on Blue.
fn board(players: Vec<Player>) -> Board {
    Board {
        state: Some("INGAME".to_owned()),
        state_label: Some("in game".to_owned()),
        map: Some("Ascent".to_owned()),
        mode: Some("Competitive".to_owned()),
        side: Some("Attack".to_owned()),
        self_team: Some("Blue".to_owned()),
        score: Some(Score {
            ally: Some(7),
            enemy: Some(5),
            round: Some(13),
        }),
        players,
        ..Board::default()
    }
}

/// Names from everywhere Riot has players, and the widest name there is.
fn names() -> Board {
    let mut players = vec![
        player("n1", "빛나는별하늘#KR1", "Red", "Jett", 21),
        player("n2", "とうふメンタル#JP1", "Red", "Sage", 15),
        player("n3", "Ñandú Ölçer#ÉÉÉ", "Red", "Omen", 12),
        player("n4", "WWWWWWWWWWWWWWWW#WWWWW", "Red", "Raze", 18),
        player("n5", "水晶の龍#中文", "Red", "Neon", 9),
        player("n6", "Day#9932", "Blue", "Chamber", 13),
        player("n7", "🔥fire🔥#EMO", "Blue", "Phoenix", 11),
        player("n8", "Ωmega Δelta#ΣΣ", "Blue", "Viper", 14),
        player("n9", "i#1", "Blue", "Cypher", 10),
        player("n10", "", "Blue", "Deadlock", 12),
    ];
    if let Some(me) = players
        .iter_mut()
        .find(|p| p.puuid.as_deref() == Some("n6"))
    {
        me.is_self = true;
    }
    // Streamer mode: Riot hides the name and the level.
    if let Some(hidden) = players
        .iter_mut()
        .find(|p| p.puuid.as_deref() == Some("n10"))
    {
        hidden.name = None;
        hidden.hidden.name = true;
        hidden.hidden.level = true;
    }
    board(players)
}

/// Every number at the end of its range.
fn extremes() -> Board {
    let mut radiant = player("x1", "Apex#TOP", "Red", "Jett", 27);
    radiant.rr = Some(1043);
    radiant.leaderboard = Some(1);
    radiant.kd = Some(12.5);
    radiant.hs_pct = Some(100.0);
    radiant.win_rate = Some(100.0);
    radiant.games = Some(1);
    radiant.level = Some(999);
    radiant.rr_earned = Some(30);
    radiant.form = form("WWWWW");
    radiant.streak = Some(Streak {
        kind: Some("W".to_owned()),
        count: Some(12),
    });
    let mut iron = player("x2", "Floor#LOW", "Red", "Tejo2", 3);
    iron.rr = Some(0);
    iron.kd = Some(0.0);
    iron.hs_pct = Some(0.0);
    iron.win_rate = Some(0.0);
    iron.level = Some(1);
    iron.rr_earned = Some(-30);
    iron.form = form("LLLDL");
    let mut unranked = player("x3", "NoRank#UR", "Red", "Harbor", 0);
    unranked.rr = None;
    unranked.kd = None;
    unranked.hs_pct = None;
    unranked.win_rate = None;
    unranked.form = Vec::new();
    let mut drawn = player("x4", "Draws#DDD", "Red", "Astra", 17);
    drawn.form = form("DDDDD");
    let mut met = player("x5", "Rival#999", "Red", "Iso", 20);
    met.encounter = Some(Encounter {
        against_count: Some(47),
        wins_against: Some(20),
        losses_against: Some(27),
        ..Encounter::default()
    });
    let mut me = player("x6", "Day#9932", "Blue", "Chamber", 13);
    me.is_self = true;
    let mut b = board(vec![
        radiant,
        iron,
        unranked,
        drawn,
        met,
        me,
        player("x7", "Mate#1", "Blue", "Sova", 14),
        player("x8", "Mate#2", "Blue", "Killjoy", 12),
        player("x9", "Mate#3", "Blue", "Breach", 13),
        player("x10", "Mate#4", "Blue", "Gekko", 11),
    ]);
    b.map = Some("Newmap".to_owned());
    b.score = Some(Score {
        ally: Some(12),
        enemy: Some(12),
        round: Some(25),
    });
    b
}

/// A full five on the other side, and two parties on yours.
fn parties() -> Board {
    let party = |number: u32, size: u32| Party {
        number: Some(number),
        size: Some(size),
        ..Party::default()
    };
    let mut players = vec![
        player("p1", "Five#A", "Red", "Jett", 18),
        player("p2", "Five#B", "Red", "Sova", 18),
        player("p3", "Five#C", "Red", "Omen", 17),
        player("p4", "Five#D", "Red", "Killjoy", 19),
        player("p5", "Five#E", "Red", "Skye", 18),
        player("p6", "Day#9932", "Blue", "Chamber", 13),
        player("p7", "Duo#MATE", "Blue", "Sage", 12),
        player("p8", "Trio#ONE", "Blue", "Raze", 14),
        player("p9", "Trio#TWO", "Blue", "Viper", 13),
        player("p10", "Trio#TRI", "Blue", "Fade", 13),
    ];
    for p in players.iter_mut().take(5) {
        p.party = Some(party(2, 5));
    }
    for p in players.iter_mut().skip(5).take(2) {
        p.party = Some(party(1, 2));
    }
    for p in players.iter_mut().skip(7) {
        p.party = Some(party(3, 3));
    }
    if let Some(me) = players.get_mut(5) {
        me.is_self = true;
    }
    board(players)
}

/// Everybody on the other side flagged, each for several reasons.
fn flagged() -> Board {
    let mut players = Vec::new();
    for (i, agent) in ["Jett", "Reyna", "Neon", "Raze", "Yoru"].iter().enumerate() {
        let mut p = player(&format!("f{i}"), &format!("Fresh{i}#NEW"), "Red", agent, 21);
        p.level = Some(20 + u32::try_from(i).unwrap_or(0));
        p.kd = Some(1.9);
        p.smurf = true;
        p.smurf_reasons = vec![
            format!("Level {}, peak Immortal 2", 20 + i),
            "K/D 1.92 over 14 games".to_owned(),
            "38% headshots".to_owned(),
        ];
        p.stack_guess = (i < 2).then_some(StackGuess {
            size: Some(2),
            confidence: Some(70),
            shared: Some(5),
            same: Some(5),
        });
        players.push(p);
    }
    let mut me = player("m", "Day#9932", "Blue", "Chamber", 13);
    me.is_self = true;
    players.push(me);
    for i in 0..4 {
        players.push(player(
            &format!("a{i}"),
            &format!("Mate{i}#1"),
            "Blue",
            "Sova",
            12,
        ));
    }
    board(players)
}

/// Agent select, before anybody has locked and before anything is known.
fn sparse() -> Board {
    let mut players = Vec::new();
    for i in 0..5 {
        let mut p = Player {
            puuid: Some(format!("s{i}")),
            name: Some(format!("Someone{i}#0000")),
            team: Some("Blue".to_owned()),
            is_self: i == 0,
            ..Player::default()
        };
        if i < 2 {
            p.agent = Some("Clove".to_owned());
        }
        players.push(p);
    }
    Board {
        state: Some("PREGAME".to_owned()),
        state_label: Some("agent select".to_owned()),
        map: Some("Lotus".to_owned()),
        mode: Some("Competitive".to_owned()),
        self_team: Some("Blue".to_owned()),
        lock_progress: Some(LockProgress {
            locked: Some(2),
            total: Some(5),
        }),
        players,
        ..Board::default()
    }
}

/// A deathmatch: fourteen players, each their own team.
fn deathmatch() -> Board {
    let agents = [
        "Jett", "Reyna", "Neon", "Raze", "Yoru", "Phoenix", "Iso", "Sova", "Omen", "Sage", "Fade",
        "Skye", "Viper", "Chamber",
    ];
    let players: Vec<Player> = agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let id = format!("d{i}");
            let mut p = player(
                &id,
                &format!("Brawler{i}#DM"),
                &id,
                agent,
                6 + 1.max(i as u32),
            );
            p.is_self = i == 0;
            p
        })
        .collect();
    Board {
        mode: Some("Deathmatch".to_owned()),
        side: None,
        self_team: Some("d0".to_owned()),
        score: None,
        ..board(players)
    }
}

/// Somebody Riot said nothing about, somebody hidden, and three you wrote
/// about, one of them hidden.
fn strangers() -> (Board, Notes) {
    let mut board = sample();
    board.players.push(Player {
        puuid: Some("z1".to_owned()),
        team: Some("Red".to_owned()),
        ..Player::default()
    });
    let mut hidden = player("z2", "", "Red", "Neon", 16);
    hidden.name = None;
    hidden.hidden.name = true;
    board.players.push(hidden);
    let mut notes = Notes::default();
    for (id, tag) in [("f", "flanks"), ("z2", "toxic"), ("g", "op main")] {
        notes.set(
            id,
            crate::notes::Note {
                text: String::new(),
                tags: vec![tag.to_owned()],
                name: String::new(),
            },
        );
    }
    (board, notes)
}

/// One account with the longest of everything the panel prints: name, agent
/// names, reasons, tags and a four figure win sample.
fn wordy() -> Board {
    let mut b = sample();
    let mut long = player("w1", "WWWWWWWWWWWWWWWW#WWWWW", "Red", "Brimstone", 12);
    long.role = Some("Controller".to_owned());
    long.title = Some("Tactical Hesitation".to_owned());
    long.level = Some(1234);
    long.games = Some(1234);
    long.peak_rank = Some("Immortal 3".to_owned());
    long.peak_rank_tier = Some(26);
    long.peak_act = Some("V25 Act 6".to_owned());
    long.previous_rank = Some("Ascendant 3".to_owned());
    long.top_agents = ["Brimstone", "Deadlock", "Harbor"]
        .iter()
        .zip([12, 9, 7])
        .map(|(agent, games)| overseer_core::TopAgent {
            agent: Some((*agent).to_owned()),
            games: Some(games),
        })
        .collect();
    long.map_win_rate = Some(overseer_core::MapWinRate {
        win_rate: Some(58.0),
        games: Some(1234),
    });
    long.smurf = true;
    long.smurf_reasons = vec![
        "14 ranks below their peak of Immortal 3".to_owned(),
        "Won 88.0% of 1234 games".to_owned(),
        "K/D 2.41 over 5 games".to_owned(),
    ];
    long.auto_tags = ["spectre", "forces", "one-trick", "strong map", "teammate"]
        .iter()
        .map(|t| overseer_core::AutoTag {
            tag: Some((*t).to_owned()),
            kind: Some("Weapon".to_owned()),
            ..overseer_core::AutoTag::default()
        })
        .collect();
    b.players.push(long);
    b
}

/// The scenes, each at the size it is worth seeing at.
fn shots() -> Vec<(&'static str, Vec2, Shot)> {
    let window = |board: Board, selected: Option<&'static str>| {
        Shot::Window(Scene {
            board,
            ..Scene::of(selected)
        })
    };
    let (odd, noted) = strangers();
    vec![
        (
            "names",
            vec2(1200.0, 640.0),
            window(names(), Some("빛나는별하늘#KR1")),
        ),
        (
            "extremes",
            vec2(1200.0, 700.0),
            window(extremes(), Some("Apex#TOP")),
        ),
        (
            "parties",
            vec2(1200.0, 640.0),
            window(parties(), Some("Five#A")),
        ),
        (
            "flagged",
            vec2(1200.0, 760.0),
            window(flagged(), Some("Fresh0#NEW")),
        ),
        ("sparse", vec2(1200.0, 420.0), window(sparse(), None)),
        (
            "strangers",
            vec2(1200.0, 760.0),
            Shot::Window(Scene::of(Some("GhostDash#OCE")).noted(noted).with(odd)),
        ),
        (
            "deathmatch",
            vec2(1200.0, 900.0),
            window(deathmatch(), None),
        ),
        (
            "narrow",
            vec2(480.0, 900.0),
            window(sample(), Some("Day#9932")),
        ),
        ("tiny", vec2(360.0, 600.0), window(sample(), None)),
        (
            "huge",
            vec2(2560.0, 1000.0),
            window(sample(), Some("NeonLock#VAL")),
        ),
        (
            "hover-tag",
            vec2(1200.0, 480.0),
            Shot::Hover(Scene::of(Some("NeonLock#VAL")), egui::pos2(200.0, 186.0)),
        ),
        (
            "career-tall",
            vec2(1200.0, 2600.0),
            Shot::Window(Scene::of(Some("Day#9932")).lived(a_history())),
        ),
        (
            "ally-panel",
            vec2(1200.0, 1400.0),
            Shot::Window(Scene::of(Some("SilentEnt#GG"))),
        ),
        (
            "enemy-career",
            vec2(1200.0, 1300.0),
            Shot::Window(Scene::of(Some("NeonLock#VAL")).lived(a_history())),
        ),
    ]
    .into_iter()
    .chain(wordy_shots())
    .chain(overlays())
    .collect()
}

/// The longest of everything, at the panel's narrow, default and wide sizes.
fn wordy_shots() -> Vec<(&'static str, Vec2, Shot)> {
    let wordy_scene = || Scene {
        board: wordy(),
        ..Scene::of(Some("WWWWWWWWWWWWWWWW#WWWWW"))
    };
    vec![
        (
            "wordy-narrow",
            vec2(1000.0, 1100.0),
            Shot::Window(wordy_scene()),
        ),
        (
            "wordy-1280",
            vec2(1280.0, 800.0),
            Shot::Window(wordy_scene()),
        ),
        (
            "wordy-wide",
            vec2(1920.0, 1000.0),
            Shot::Window(wordy_scene()),
        ),
        (
            "rating-hover",
            vec2(1200.0, 1240.0),
            Shot::Hover(
                Scene::of(Some("Day#9932")).lived(a_history()),
                egui::pos2(1100.0, 712.0),
            ),
        ),
        (
            "rating-hover-end",
            vec2(1200.0, 1240.0),
            Shot::Hover(
                Scene::of(Some("Day#9932")).lived(a_history()),
                egui::pos2(1178.0, 700.0),
            ),
        ),
    ]
}

/// The overlay's scenes, at its own width.
fn overlays() -> Vec<(&'static str, Vec2, Shot)> {
    vec![
        (
            "overlay-sparse",
            vec2(548.0, 200.0),
            Shot::Overlay(sparse()),
        ),
        (
            "overlay-strangers",
            vec2(548.0, 460.0),
            Shot::Overlay(strangers().0),
        ),
        ("overlay-names", vec2(548.0, 400.0), Shot::Overlay(names())),
        (
            "overlay-parties",
            vec2(548.0, 400.0),
            Shot::Overlay(parties()),
        ),
        (
            "overlay-flagged",
            vec2(548.0, 400.0),
            Shot::Overlay(flagged()),
        ),
        (
            "overlay-extremes",
            vec2(548.0, 400.0),
            Shot::Overlay(extremes()),
        ),
    ]
}

#[test]
#[ignore = "renders to a folder for a person to look at"]
fn every_stress_lobby_renders_to_the_folder() {
    let Ok(out) = std::env::var("STRESS_OUT") else {
        return;
    };
    let notes = Notes::default();
    let sort = Sort::default();
    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 400.0))
        .build_ui_state(
            |ui, state: &mut Option<Shot>| match state {
                Some(Shot::Window(scene) | Shot::Hover(scene, _)) => draw(ui, scene),
                Some(Shot::Overlay(board)) => {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE.fill(design::colour::BG))
                        .show(ui, |ui| {
                            let _touched = crate::board::draw(
                                ui,
                                &crate::board::Scene {
                                    board,
                                    sort: &sort,
                                    filter: "",
                                    selected: None,
                                    notes: &notes,
                                    hidden: &[],
                                    enemies_first: true,
                                    place: crate::board::Place::Overlay,
                                    still: false,
                                    since: 10.0,
                                },
                            );
                        });
                }
                None => {}
            },
            None,
        );
    design::install_fonts(&harness.ctx);
    harness.ctx.set_style_of(egui::Theme::Dark, design::style());
    harness.run();
    for (name, size, shot) in shots() {
        let names: String = match &shot {
            Shot::Window(scene) | Shot::Hover(scene, _) => &scene.board,
            Shot::Overlay(board) => board,
        }
        .players
        .iter()
        .filter_map(|p| p.name.as_deref())
        .collect();
        design::cover_now(&harness.ctx, &names);
        harness.set_size(size);
        let rest = match &shot {
            Shot::Hover(_, at) => Some(*at),
            _ => None,
        };
        *harness.state_mut() = Some(shot);
        harness.run();
        if let Some(at) = rest {
            // Rest there long enough for a tooltip's delay to pass.
            harness.hover_at(at);
            for _frame in 0..40 {
                harness.step();
            }
        }
        let mut frames = 0;
        while harness.ctx.has_requested_repaint() && frames < 120 {
            harness.run();
            frames += 1;
        }
        let image = harness.render().expect("a frame renders");
        image
            .save(format!("{out}/{name}.png"))
            .expect("the folder takes a picture");
    }
}
