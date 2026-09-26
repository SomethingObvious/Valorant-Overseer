//! The wizard's look, as a test.
//!
//! An installer is the first thing anybody sees and the thing nobody looks at
//! again, which is exactly the combination that lets it rot. Each of the
//! three steps is rendered offscreen and compared with a PNG, for the same
//! reason the window's screens are.

#![cfg(test)]

use egui::{Ui, vec2};
use egui_kittest::Harness;

use crate::plan::{Finding, Profile, Survey};
use crate::wizard;

/// What the wizard would show for a machine with nothing wrong with it, and
/// for one that cannot be installed on.
fn findings(blocked: bool) -> Survey {
    let mut survey = Survey {
        findings: vec![
            Finding {
                what: "Windows".to_owned(),
                detail: "This is the only platform the Riot client has a local API on".to_owned(),
                blocking: false,
            },
            Finding {
                what: "Installer".to_owned(),
                detail: "Found, and it does the work this wizard asks for".to_owned(),
                blocking: false,
            },
            Finding {
                what: "This folder".to_owned(),
                detail: "Writable, so nothing needs administrator".to_owned(),
                blocking: false,
            },
            Finding {
                what: "Python".to_owned(),
                detail: "Already set up here, so the install will be a quick repair".to_owned(),
                blocking: false,
            },
            Finding {
                what: "Already installed".to_owned(),
                detail: "Version 2.34.0, being repaired".to_owned(),
                blocking: false,
            },
        ],
    };
    if blocked {
        survey.findings.push(Finding {
            what: "Disk".to_owned(),
            detail: "Needs 400 MB free and there are 120 MB".to_owned(),
            blocking: true,
        });
    }
    survey
}

/// What the log looks like partway through a real install.
fn log() -> Vec<String> {
    [
        "==> Checking this PC ...",
        "  + Windows x64, writable folder, enough disk space.",
        "  + Python 3.12.10 x64 ready.",
        "  ! repair needed: a dependency hash changed",
        "  . Installing dependencies (this is the slow part) ...",
        "  + Region saved: na.",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

/// Which frame to draw.
enum Scene {
    /// Nothing yet: fonts land on the frame after they are installed.
    Blank,
    /// Step one.
    Look(Survey),
    /// Step two.
    Choose(Profile, usize),
    /// Step three, with an outcome or without one.
    Install(Vec<String>, Option<bool>),
}

/// Draws one scene through the wizard's own functions.
fn draw(ui: &mut Ui, scene: &mut Scene) {
    match scene {
        Scene::Blank => {}
        Scene::Look(survey) => wizard::look(ui, survey),
        Scene::Choose(profile, region) => wizard::choose(ui, profile, region),
        Scene::Install(lines, outcome) => wizard::install(ui, lines, *outcome, Profile::Both),
    }
}

#[test]
fn every_step_is_unchanged() {
    let mut harness = Harness::builder()
        .with_size(vec2(720.0, 420.0))
        .build_ui_state(|ui, state: &mut Scene| draw(ui, state), Scene::Blank);
    overseer_ui::install_fonts(&harness.ctx);
    harness
        .ctx
        .set_style_of(egui::Theme::Dark, overseer_ui::style());
    harness.run();

    for (name, scene) in [
        ("look", Scene::Look(findings(false))),
        ("look-blocked", Scene::Look(findings(true))),
        ("choose", Scene::Choose(Profile::Cli, 1)),
        ("install", Scene::Install(log(), None)),
        ("install-done", Scene::Install(log(), Some(true))),
    ] {
        *harness.state_mut() = scene;
        harness.run();
        let mut frames = 0;
        while harness.ctx.has_requested_repaint() && frames < 120 {
            harness.run();
            frames += 1;
        }
        harness.snapshot(name);
    }
}
