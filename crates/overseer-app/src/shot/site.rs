//! The website's and the README's pictures, rendered at twice the size from
//! the same made-up players as the snapshots, so no real player's name is
//! ever published. Run with
//! `SITE_OUT=<folder> cargo test -p overseer-app site -- --ignored`, then crop
//! and save them as WebP into `docs/assets/shots`.

use egui::{Ui, Vec2, vec2};
use egui_kittest::Harness;
use overseer_core::Board;

use super::{NOW_MS, Scene, a_history, draw, past_games, sample};
use crate::history::History;
use crate::notes::Notes;
use crate::settings::{Quality, Settings};
use crate::sort::Sort;
use crate::view;
use overseer_ui as design;

/// What one picture is of.
enum Shot {
    Window(Box<Scene>),
    History(Box<History>),
    Settings(Box<Settings>),
    Overlay(Box<Board>),
}

/// Agent select, where only your own team is known.
fn agent_select() -> Board {
    let full = sample();
    Board {
        state: Some("PREGAME".to_owned()),
        state_label: Some("Agent Select".to_owned()),
        score: None,
        side: None,
        players: full
            .players
            .iter()
            .filter(|p| p.team == full.self_team)
            .cloned()
            .collect(),
        ..sample()
    }
}

/// Every picture, by name and size in points.
fn shots() -> Vec<(&'static str, Vec2, Shot)> {
    vec![
        (
            "board",
            vec2(1600.0, 955.0),
            Shot::Window(Box::new(Scene::of(Some("NeonLock#VAL")).lived(a_history()))),
        ),
        (
            "pregame",
            vec2(1600.0, 955.0),
            Shot::Window(Box::new(Scene::of(Some("Day#9932")).with(agent_select()))),
        ),
        (
            "history",
            vec2(1600.0, 955.0),
            Shot::History(Box::new(History::showing(past_games()))),
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
            let machine = crate::machine::Machine {
                memory: Some(16 << 30),
                threads: 8,
                card: true,
            };
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
        Shot::Overlay(board) => {
            crate::overlay::contents(ui, &mut |ui: &mut Ui| {
                let corner = ui.max_rect();
                let _drew = crate::board::draw(
                    ui,
                    &crate::board::Scene {
                        board,
                        sort: &Sort::default(),
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
        }
    }
}

#[test]
#[ignore = "renders the website's pictures to a folder"]
fn the_site_pictures_render_to_the_folder() {
    let Ok(out) = std::env::var("SITE_OUT") else {
        return;
    };
    for (name, size, shot) in shots() {
        let mut harness = Harness::builder()
            .with_size(size)
            .with_pixels_per_point(2.0)
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
        let mut frames = 0;
        while harness.ctx.has_requested_repaint() && frames < 120 {
            harness.run();
            frames += 1;
        }
        harness
            .render()
            .expect("a frame renders")
            .save(format!("{out}/{name}.png"))
            .expect("the folder takes a picture");
    }
}
