//! The website's and the README's pictures, rendered at three times the size
//! from the snapshots' board with a site lobby's names and numbers, so no real
//! player's Riot ID is ever published. Run with
//! `SITE_OUT=<folder> cargo test -p overseer-app site -- --ignored`, then crop
//! and save them as WebP into `docs/assets/shots`. `SITE_ATLAS=<file>` adds
//! the Lineups picture, from a JSON file with a map and an agent the way the
//! backend's `lineups.overview()` gives them, since the minimap is fetched
//! onto this machine.

use egui::{Ui, Vec2, vec2};
use egui_kittest::Harness;
use overseer_core::{Board, Bridge, Player};

use super::{
    A_HISTORY, NOW_MS, Scene, career, draw, evidence, machine, overlay_board, past_games, sample,
    settle, show_lineups,
};
use crate::history::History;
use crate::lineups::Lineups;
use crate::settings::{MapTurn, Quality, Settings};
use crate::view;
use overseer_ui as design;

/// The snapshots' players as the site shows them: well-known pros' handles
/// bent far enough that none is a real Riot ID, with the short custom tags
/// most players set. The two flagged as Worth a Look keep invented names, so no real
/// person reads as a smurf.
const NAMES: [(&str, &str); 15] = [
    ("NeonLock#VAL", SELECTED),
    ("SilentEnt#GG", "Riftborn#0210"),
    ("GhostDash#OCE", "Aspasito#777"),
    ("FrostSpike#VAL", "Lessless#2211"),
    ("DriftAim#GG", "Boastful#yuh"),
    ("Day#9932", YOU),
    ("AVeryLongNameIndeed#0000", "Demonwon#1337"),
    ("DriftFrag#RR", "Zekkerz#zkn"),
    ("SageDiff#EUW", "Sacymono#2403"),
    // Your own row in the history's match, so it is you there too.
    ("SaltiSmySugar#OBAMA", YOU),
    ("Xan#NA1", "ChronoKale#kale"),
    ("SumsItUp#777", "f0rsook#1231"),
    ("Spectre#GG", "Alfajores#7z"),
    ("Pepsi Man#COLA", "CryoCellar#ice"),
    ("Wanderlust#EUW", "Derkovic#003"),
];

/// Who the board's picture has selected, by the name the site shows.
const SELECTED: &str = "Vexlyn#4AM";

/// You, by the name the site shows.
const YOU: &str = "Tenzei#0505";

/// Swaps every Riot ID in `text` for its site name.
fn renamed(text: &str) -> String {
    NAMES
        .iter()
        .fold(text.to_owned(), |text, (from, to)| text.replace(from, to))
}

/// One row's numbers as a ranked lobby has them.
struct Line {
    /// Their Riot ID.
    name: &'static str,
    /// Their rank and its tier.
    rank: (&'static str, u32),
    /// The best rank they have reached, its tier, and the act it was in.
    peak: (&'static str, u32, &'static str),
    rr: i64,
    kd: f64,
    /// Headshot share, in percent.
    hs: f64,
    /// Act win rate and the ranked games behind it.
    won: (f64, u32),
    level: u32,
    /// The results the K/D was read from, newest first, which is what the
    /// games column counts.
    form: &'static str,
}

/// Every row of the site's lobby, by puuid. Ranks run Gold 1 to Diamond 2
/// with both duos a rank apart, the two flagged accounts are low level with a
/// high peak, and every read is 16 to 20 games, the app's default depth.
const LOBBY: [(&str, Line); 10] = [
    (
        "a",
        Line {
            name: "Riftborn#0210",
            rank: ("Platinum 1", 15),
            peak: ("Ascendant 2", 22, "V25 Act 1"),
            rr: 52,
            kd: 1.38,
            hs: 28.6,
            won: (58.0, 44),
            level: 41,
            form: "WWLWWLWWLWLWWLWWLW",
        },
    ),
    (
        "b",
        Line {
            name: YOU,
            rank: ("Platinum 2", 16),
            peak: ("Platinum 3", 17, "V25 Act 4"),
            rr: 37,
            kd: 0.97,
            hs: 21.9,
            won: (52.0, 118),
            level: 154,
            form: "WLWLWWLWLWWLLWLWWLWL",
        },
    ),
    (
        "c",
        Line {
            name: "Demonwon#1337",
            rank: ("Gold 3", 14),
            peak: ("Platinum 1", 15, "V25 Act 3"),
            rr: 88,
            kd: 0.86,
            hs: 18.2,
            won: (47.0, 76),
            level: 92,
            form: "LLWLWLLWWLWLLWLWL",
        },
    ),
    (
        "h",
        Line {
            name: "Zekkerz#zkn",
            rank: ("Diamond 1", 18),
            peak: ("Diamond 2", 19, "V25 Act 3"),
            rr: 20,
            kd: 1.21,
            hs: 23.1,
            won: (55.0, 132),
            level: 212,
            form: "WLWWLWLWWLWWLWLWLWW",
        },
    ),
    (
        "i",
        Line {
            name: "Sacymono#2403",
            rank: ("Gold 1", 12),
            peak: ("Gold 2", 13, "V25 Act 2"),
            rr: 66,
            kd: 0.89,
            hs: 16.8,
            won: (50.0, 101),
            level: 77,
            form: "WLWLLWWLWLWLWLWL",
        },
    ),
    (
        "d",
        Line {
            name: SELECTED,
            rank: ("Platinum 3", 17),
            peak: ("Immortal 1", 24, "V25 Act 2"),
            rr: 64,
            kd: 1.62,
            hs: 31.0,
            won: (64.0, 34),
            level: 28,
            form: "WWWWLWWLWWLWWWLWLW",
        },
    ),
    (
        "e",
        Line {
            name: "Derkovic#003",
            rank: ("Platinum 1", 15),
            peak: ("Diamond 1", 18, "V25 Act 3"),
            rr: 41,
            kd: 1.01,
            hs: 20.6,
            won: (51.0, 88),
            level: 163,
            form: "LWWLWLWLWWLLWLWLW",
        },
    ),
    (
        "f",
        Line {
            name: "Aspasito#777",
            rank: ("Gold 3", 14),
            peak: ("Platinum 2", 16, "V25 Act 4"),
            rr: 77,
            kd: 1.12,
            hs: 18.7,
            won: (53.0, 121),
            level: 241,
            form: "WLWWLWLLWWLWLWWLWLW",
        },
    ),
    (
        "g",
        Line {
            name: "Lessless#2211",
            rank: ("Gold 2", 13),
            peak: ("Gold 3", 14, "V25 Act 4"),
            rr: 12,
            kd: 0.94,
            hs: 17.4,
            won: (48.0, 97),
            level: 118,
            form: "LWLWWLLWLWLWLWLW",
        },
    ),
    (
        "j",
        Line {
            name: "Boastful#yuh",
            rank: ("Diamond 2", 19),
            peak: ("Ascendant 1", 21, "V25 Act 1"),
            rr: 30,
            kd: 1.08,
            hs: 23.9,
            won: (52.0, 140),
            level: 307,
            form: "WLLWLWWLWLWLWWLWLL",
        },
    ),
];

/// A tag with the sentence the backend would have read it from.
fn tag(tag: &str, why: &str, kind: &str) -> overseer_core::AutoTag {
    overseer_core::AutoTag {
        tag: Some(tag.to_owned()),
        why: Some(why.to_owned()),
        kind: Some(kind.to_owned()),
        ..overseer_core::AutoTag::default()
    }
}

/// Puts one row's numbers from [`LOBBY`] on a player, with the run they are on.
fn set(p: &mut Player, line: &Line) {
    p.name = Some(line.name.to_owned());
    p.rank = Some(line.rank.0.to_owned());
    p.rank_tier = Some(line.rank.1);
    p.peak_rank = Some(line.peak.0.to_owned());
    p.peak_rank_tier = Some(line.peak.1);
    p.peak_act = Some(line.peak.2.to_owned());
    p.rr = Some(line.rr);
    p.kd = Some(line.kd);
    p.hs_pct = Some(line.hs);
    p.win_rate = Some(line.won.0);
    p.games = Some(line.won.1);
    p.level = Some(line.level);
    p.form = line.form.chars().map(String::from).collect();
    let first = line.form.chars().next();
    p.streak = Some(overseer_core::Streak {
        kind: first.map(String::from),
        count: u32::try_from(line.form.chars().take_while(|c| Some(*c) == first).count()).ok(),
    });
    p.auto_tags.clear();
    p.smurf = false;
    p.smurf_reasons.clear();
    p.smurf_evidence.clear();
}

/// The two flagged accounts' reasons, worked out from their numbers the way
/// the backend's own rules do, and the rest of what the panel shows of them.
fn flag(p: &mut Player) {
    match p.name.as_deref() {
        Some("Riftborn#0210") => {
            p.smurf = true;
            p.smurf_reasons = vec!["Level 41, peak Ascendant 2".to_owned()];
            p.smurf_evidence = vec![
                evidence("Level", "41", "under 60"),
                evidence("Peak", "Ascendant 2", "V25 Act 1"),
            ];
            p.auto_tags = vec![
                tag(
                    "outlaw",
                    "Carried the Outlaw in 37 of 409 rounds over their last 18 games",
                    "Weapon",
                ),
                tag(
                    "forces",
                    "Bought a gun after 5 of 7 lost pistol rounds",
                    "Economy",
                ),
            ];
        }
        Some(SELECTED) => vexlyn(p),
        _ => {}
    }
}

/// The selected enemy: a level 28 account with an Immortal peak, playing
/// in a trio the app had to guess and on a run of wins.
fn vexlyn(p: &mut Player) {
    p.rr_earned = Some(22);
    p.map_win_rate = Some(overseer_core::MapWinRate {
        win_rate: Some(67.0),
        games: Some(6),
    });
    p.encounter = Some(overseer_core::Encounter {
        against_count: Some(2),
        wins_against: Some(1),
        losses_against: Some(1),
        ..overseer_core::Encounter::default()
    });
    p.stack_guess = Some(overseer_core::StackGuess {
        size: Some(3),
        confidence: Some(81),
        shared: Some(6),
        same: Some(5),
    });
    p.smurf = true;
    p.smurf_reasons = vec![
        "Level 28, peak Immortal 1".to_owned(),
        "K/D 1.62 at level 28".to_owned(),
        "Won 64% of 34 games".to_owned(),
    ];
    p.smurf_evidence = vec![
        evidence("Level", "28", "under 60"),
        evidence("Peak", "Immortal 1", "V25 Act 2"),
        evidence("K/D", "1.62", "over 18 games"),
        evidence("Won", "64%", "of 34 games"),
    ];
    p.auto_tags = vec![
        overseer_core::AutoTag {
            means: Some("Plays the Operator a lot. Expect it on long angles.".to_owned()),
            ..tag(
                "op",
                "Carried the Operator in 58 of 412 rounds over their last 18 games",
                "weapon",
            )
        },
        tag(
            "entry",
            "In the round's first fight 97 times in 412 rounds, won 61",
            "Duels",
        ),
        tag("hot", "Won their last 4", "Form"),
        tag(
            "forces",
            "Bought a gun after 6 of 8 lost pistol rounds",
            "Economy",
        ),
    ];
    p.scout = Some(scout());
    p.brief = all_figures();
}

/// The lines under All Figures, worded the way the backend words them.
fn all_figures() -> Vec<overseer_core::BriefLine> {
    [
        (
            "tendency",
            true,
            "Opening kill/death involvement",
            "24% vs peers' 15% · 412 rounds",
            Some(0.89),
        ),
        (
            "tendency",
            true,
            "Gun-kill range",
            "2,140 vs peers' 1,630 map units · 211 kills",
            Some(0.81),
        ),
        (
            "tendency",
            true,
            "Rounds with Operator",
            "14% vs peers' 4% · 412 rounds",
            Some(0.93),
        ),
        (
            "tendency",
            false,
            "Death timing",
            "31s vs peers' 38s · 284 deaths",
            Some(0.27),
        ),
        (
            "tendency",
            false,
            "Spike plants",
            "19% vs peers' 21% · 206 attack rounds",
            Some(0.42),
        ),
        (
            "buy",
            false,
            "Pistol: Ghost 22/36 · light armor 25/36",
            "Avg spend 780 vs 690",
            None,
        ),
        (
            "record",
            false,
            "Recent agents: Clove 15/18, Omen 3/18",
            "",
            None,
        ),
    ]
    .into_iter()
    .map(
        |(kind, flag, headline, evidence, pct)| overseer_core::BriefLine {
            kind: Some(kind.to_owned()),
            flag,
            headline: Some(headline.to_owned()),
            evidence: Some(evidence.to_owned()),
            pct,
            ..overseer_core::BriefLine::default()
        },
    )
    .collect()
}

/// The selected enemy's Scout section, worded the way the backend words it:
/// an aggressive Operator player who takes the first fight.
fn scout() -> overseer_core::Scout {
    let finding = |headline: &str, evidence: &str, suggest: &str, pct: f32, ends: (&str, &str)| {
        overseer_core::ScoutFinding {
            headline: Some(headline.to_owned()),
            evidence: Some(evidence.to_owned()),
            suggest: Some(suggest.to_owned()),
            pos: Some(1.4 * (pct - 0.5) / 0.4),
            left: Some(ends.0.to_owned()),
            right: Some(ends.1.to_owned()),
            share: Some(crate::panel::ordinal(pct)),
            ..overseer_core::ScoutFinding::default()
        }
    };
    overseer_core::Scout {
        title: Some("Scout".to_owned()),
        tag: Some(finding(
            "Aggressive",
            "More aggressive than 86% of peers · 18 games",
            "",
            0.86,
            ("passive", "aggressive"),
        )),
        habits: vec![
            finding(
                "Takes the first fight",
                "In the opening fight more than 89% of peers · 24% vs 15% · 18 games",
                "Set up a trade for the opening.",
                0.89,
                ("stays out", "takes fights"),
            ),
            finding(
                "Fights at long range",
                "Kills from further than 81% of peers · 16/18 games",
                "Use utility before a long peek.",
                0.81,
                ("close", "long range"),
            ),
        ],
        buy: Some(overseer_core::ScoutBuy {
            label: Some("Pistol · 36 past rounds".to_owned()),
            armor: Some("Light armor 25/36".to_owned()),
            items: vec![
                overseer_core::ScoutGun {
                    name: Some("Ghost".to_owned()),
                    value: Some("22/36".to_owned()),
                },
                overseer_core::ScoutGun {
                    name: Some("Classic".to_owned()),
                    value: Some("14/36".to_owned()),
                },
            ],
            ..overseer_core::ScoutBuy::default()
        }),
        footer: Some("Last 18 ranked games · 15 on Clove".to_owned()),
        ..overseer_core::Scout::default()
    }
}

/// The site's lobby: the snapshot board's shapes with the numbers a real
/// Platinum match has, and nobody hidden.
fn lobby() -> Board {
    let mut board = sample();
    for p in &mut board.players {
        let Some((_, line)) = LOBBY.iter().find(|(id, _)| p.puuid.as_deref() == Some(*id)) else {
            continue;
        };
        set(p, line);
        match line.name {
            "Derkovic#003" => {
                p.agent = Some("Sova".to_owned());
                p.agent_color = Some("#3BA0E5".to_owned());
            }
            "Aspasito#777" => {
                p.agent = Some("Hush".to_owned());
                p.agent_color = Some("#3D2C44".to_owned());
            }
            YOU => {
                // The session's last game, a win.
                p.rr_earned = Some(19);
            }
            "Boastful#yuh" => {
                p.auto_tags = vec![
                    tag(
                        "marshal",
                        "Carried the Marshal in 41 of 396 rounds over their last 18 games",
                        "Weapon",
                    ),
                    tag("saves", "Saved after 5 of 7 lost pistol rounds", "Economy"),
                ];
            }
            _ => flag(p),
        }
        // Everyone either buys or saves after a lost pistol round, so the
        // two counts make five on each side.
        let habit = ["forces", "saves"];
        if !p
            .auto_tags
            .iter()
            .any(|t| habit.contains(&t.tag.as_deref().unwrap_or_default()))
        {
            p.auto_tags
                .push(match ECONOMY.iter().find(|(name, ..)| *name == line.name) {
                    Some((_, "forces", of)) => tag(
                        "forces",
                        &format!("Bought a gun after {of} lost pistol rounds"),
                        "Economy",
                    ),
                    Some((_, _, of)) => tag(
                        "saves",
                        &format!("Saved after {of} lost pistol rounds"),
                        "Economy",
                    ),
                    None => continue,
                });
        }
    }
    // The backend's own order: your side first, then by rank, then level.
    let ours = board.self_team.clone();
    board.players.sort_by_key(|p| {
        (
            p.team != ours,
            std::cmp::Reverse(p.rank_tier),
            std::cmp::Reverse(p.level),
        )
    });
    for (team, rank, kd, won) in [
        ("Blue", "Platinum 1", 1.06, 52.0),
        ("Red", "Platinum 2", 1.15, 54.0),
    ] {
        board.team_stats.insert(
            team.to_owned(),
            overseer_core::TeamStats {
                rank: Some(rank.to_owned()),
                kd: Some(kd),
                win_rate: Some(won),
            },
        );
    }
    board
}

/// What the rest of the lobby does after a lost pistol round, and how often.
const ECONOMY: [(&str, &str, &str); 7] = [
    ("Derkovic#003", "forces", "4 of 6"),
    ("Aspasito#777", "saves", "6 of 8"),
    ("Lessless#2211", "forces", "5 of 6"),
    ("Zekkerz#zkn", "forces", "5 of 7"),
    (YOU, "saves", "4 of 6"),
    ("Demonwon#1337", "saves", "5 of 8"),
    ("Sacymono#2403", "saves", "6 of 7"),
];

/// What one picture is of.
enum Shot {
    Window(Box<Scene>),
    History(Box<History>),
    Settings(Box<Settings>),
    Overlay(Box<Board>),
    Lineups(Box<(Lineups, Bridge)>),
}

/// Agent select, where only your own team is known.
fn agent_select() -> Board {
    let mut board = lobby();
    board.players.retain(|p| p.team == board.self_team);
    Board {
        state: Some("PREGAME".to_owned()),
        state_label: Some("Agent Select".to_owned()),
        score: None,
        side: None,
        ..board
    }
}

/// Every picture, by name and size in points.
fn shots() -> Vec<(&'static str, Vec2, Shot)> {
    let board = || {
        Shot::Window(Box::new(
            Scene::of(Some(SELECTED))
                .with(lobby())
                .lived(career(&renamed(A_HISTORY))),
        ))
    };
    vec![
        ("board", vec2(1600.0, 980.0), board()),
        // The same board, tall enough for the whole of Vexlyn's panel.
        ("panel", vec2(1600.0, 1500.0), board()),
        (
            "pregame",
            vec2(1600.0, 955.0),
            Shot::Window(Box::new(Scene::of(Some(YOU)).with(agent_select()))),
        ),
        (
            "history",
            vec2(1600.0, 955.0),
            Shot::History(Box::new(History::showing(
                serde_json::from_str(&renamed(&past_games().to_string()))
                    .expect("renaming keeps the JSON whole"),
            ))),
        ),
        (
            "settings",
            vec2(700.0, 320.0),
            Shot::Settings(Box::default()),
        ),
        (
            "overlay",
            vec2(crate::overlay::WIDTH, 420.0),
            Shot::Overlay(Box::new(agent_select())),
        ),
    ]
}

/// Draws one picture.
fn paint(ui: &mut Ui, shot: &mut Shot) {
    match shot {
        Shot::Window(scene) => draw(ui, scene),
        Shot::History(history) => {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(design::colour::BG))
                .show(ui, |ui| {
                    let _picked = history.show(ui, 5, NOW_MS, true);
                });
        }
        Shot::Settings(settings) => {
            let machine = machine();
            let mut offline = crate::offline::Offline::default();
            offline.state = Some(crate::offline::State::default());
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(design::colour::BG))
                .show(ui, |ui| {
                    let _changed = view::settings(
                        ui,
                        settings,
                        (Quality::Rich, None, &machine),
                        (
                            view::Trouble::default(),
                            std::path::Path::new("."),
                            &mut offline,
                        ),
                    );
                });
        }
        Shot::Lineups(screen) => {
            let (lineups, bridge) = &mut **screen;
            // The app's default, attackers at the bottom.
            show_lineups(ui, lineups, bridge, MapTurn::Attack);
        }
        Shot::Overlay(board) => {
            crate::overlay::contents(ui, &mut |ui: &mut Ui| {
                let corner = ui.max_rect();
                overlay_board(ui, board);
                let _hide = crate::overlay::hide_button(ui, corner);
            });
        }
    }
}

/// The map and agent in the file at `path`, with six Brimstone incendiaries
/// on Haven's three sites, two a site, thrown from the attackers' side.
fn haven(path: &str) -> serde_json::Value {
    let mut atlas: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the atlas file reads"))
            .expect("the atlas file is a JSON object");
    let lineups = [
        (
            "a-long",
            "A",
            "A Long to Hell",
            [0.655, 0.25],
            [0.35, 0.125],
        ),
        (
            "a-lobby",
            "A",
            "A Lobby to Default",
            [0.665, 0.355],
            [0.405, 0.205],
        ),
        (
            "b-court",
            "B",
            "Courtyard to Back",
            [0.555, 0.555],
            [0.325, 0.465],
        ),
        (
            "b-doors",
            "B",
            "Mid Doors to Default",
            [0.686, 0.599],
            [0.40, 0.53],
        ),
        (
            "c-garage",
            "C",
            "Garage to Plat",
            [0.528, 0.636],
            [0.329, 0.802],
        ),
        (
            "c-lobby",
            "C",
            "C Lobby to Default",
            [0.665, 0.757],
            [0.42, 0.82],
        ),
    ]
    .map(|(id, site, title, stand, land)| {
        serde_json::json!({
            "id": id, "map": "Haven", "agent": "Brimstone", "ability": "Incendiary",
            "side": "attack", "site": site, "title": title, "stand": stand, "land": land,
        })
    });
    atlas.insert("lineups".to_owned(), lineups.into());
    atlas.insert("drawings".to_owned(), serde_json::json!({}));
    atlas.insert(
        "tools".to_owned(),
        serde_json::json!({ "ytdlp": true, "ffmpeg": true, "ffplay": true }),
    );
    atlas.into()
}

#[test]
#[ignore = "renders the website's pictures to a folder"]
fn the_site_pictures_render_to_the_folder() {
    let Ok(out) = std::env::var("SITE_OUT") else {
        return;
    };
    let mut shots = shots();
    if let Ok(atlas) = std::env::var("SITE_ATLAS") {
        shots.push((
            "lineups",
            vec2(1200.0, 820.0),
            Shot::Lineups(Box::new((
                Lineups::showing(haven(&atlas), None, "browse"),
                Bridge::start(std::path::Path::new("."), || {}),
            ))),
        ));
    }
    for (name, size, shot) in shots {
        // Each at enough pixels for its widest crop on a sharp phone: the
        // settings strip scrolls at a thousand points from a 700 point
        // render, and the board's key and panel crops are a part of it.
        let scale = match name {
            "settings" => 5.0,
            "board" | "panel" => 4.0,
            _ => 3.0,
        };
        let mut harness = Harness::builder()
            .with_size(size)
            .with_pixels_per_point(scale)
            .build_ui_state(
                |ui, state: &mut (bool, Shot)| {
                    if state.0 {
                        paint(ui, &mut state.1);
                    }
                },
                (false, shot),
            );
        design::install_fonts(&harness.ctx);
        harness.ctx.set_style_of(egui::Theme::Dark, design::style());
        harness.run();
        harness.state_mut().0 = true;
        harness.run();
        settle(&mut harness);
        harness
            .render()
            .expect("a frame renders")
            .save(format!("{out}/{name}.png"))
            .expect("the folder takes a picture");
    }
}

#[test]
fn the_site_lobby_reads_like_a_real_match() {
    let board = lobby();
    let named: Vec<&str> = board
        .players
        .iter()
        .filter_map(|p| p.name.as_deref())
        .collect();
    assert_eq!(named.len(), 10, "nobody on the site's board is hidden");
    assert!(named.contains(&SELECTED) && named.contains(&YOU));
    assert!(
        named.iter().all(|n| !n.contains([' ', '.'])),
        "handles have no spaces or periods: {named:?}"
    );
    let left = |text: &str| NAMES.iter().filter(|(from, _)| text.contains(from)).count();
    assert_eq!(named.iter().map(|n| left(n)).sum::<usize>(), 0);
    assert_eq!(left(&renamed(A_HISTORY)), 0);
    assert_eq!(left(&renamed(&past_games().to_string())), 0);
    let tiers: Vec<u32> = board.players.iter().filter_map(|p| p.rank_tier).collect();
    assert_eq!(tiers.len(), 10);
    let spread = tiers.iter().max().unwrap_or(&0) - tiers.iter().min().unwrap_or(&0);
    assert!(
        spread <= 7,
        "one lobby runs Gold to Diamond at most: {tiers:?}"
    );
    for p in &board.players {
        assert!(
            (16..=20).contains(&p.form.len()),
            "{:?} read {} games",
            p.name,
            p.form.len()
        );
        assert_eq!(p.smurf, !p.smurf_evidence.is_empty(), "{:?}", p.name);
    }
    for team in ["Blue", "Red"] {
        let habits = board
            .players
            .iter()
            .filter(|p| p.team.as_deref() == Some(team))
            .flat_map(|p| &p.auto_tags)
            .filter(|t| matches!(t.tag.as_deref(), Some("forces" | "saves")))
            .count();
        assert_eq!(habits, 5, "{team}'s forces and saves make five");
    }
}
