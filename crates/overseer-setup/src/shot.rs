//! Each wizard step rendered offscreen and compared with a PNG, because an
//! installer is seen once by everybody and looked at again by nobody.

#![cfg(test)]

use egui::{Ui, vec2};
use egui_kittest::Harness;

use crate::plan::{Finding, Survey};
use crate::wizard;

/// The findings for a machine with nothing wrong with it, or for one whose
/// folder can't be written to.
fn findings(blocked: bool) -> Survey {
    let folder = if blocked {
        // A real Windows error, and longer than the row has room for.
        Finding {
            what: "This folder".to_owned(),
            detail: "Can't write here: The process cannot access the file because it is being used by another process. (os error 32)".to_owned(),
            blocking: true,
        }
    } else {
        Finding {
            what: "This folder".to_owned(),
            detail: "Writable, so nothing needs admin rights".to_owned(),
            blocking: false,
        }
    };
    Survey {
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
            folder,
            Finding {
                what: "Python".to_owned(),
                detail: "Already set up here, so the install will be a quick repair".to_owned(),
                blocking: false,
            },
            Finding {
                what: "Already installed".to_owned(),
                detail: "Version 2.34.0, and this install will repair it".to_owned(),
                blocking: false,
            },
        ],
    }
}

/// What `scripts/install.ps1 -Region na` prints for a quick
/// repair, without the blank lines the wizard drops. The install scene shows
/// the first five lines and the done scene shows all of them.
fn log(done: bool) -> Vec<String> {
    let lines = [
        "  OVERSEER SETUP",
        "  Installs or repairs everything the app needs. Run it again any time.",
        "==> Checking this PC",
        "  + Windows x64, writable folder, enough disk space.",
        "  + Python 3.12.10 x64 ready.",
        "  + The existing install is healthy, so there is nothing to reinstall.",
        "  + Region saved: na.",
        "  + Shortcuts ready on the desktop and in the Start menu. Right-click the running app on the taskbar and pick Pin to taskbar to keep it there.",
        "  + Setup complete.",
        "  The shortcut opens the window.",
    ];
    let shown = if done { lines.len() } else { 5 };
    lines.iter().take(shown).map(|s| (*s).to_owned()).collect()
}

/// Which frame to draw.
enum Scene {
    /// Nothing yet: fonts land on the frame after they are installed.
    Blank,
    /// Step one.
    Look(Survey),
    /// Step two.
    Choose(usize),
    /// Step three, with an outcome or without one.
    Install(Vec<String>, Option<bool>),
    /// The title bar, at a step.
    Head(wizard::Step),
    /// The footer, at a step and an outcome.
    Foot(wizard::Step, Option<bool>),
}

/// Draws one scene through the wizard's own functions.
fn draw(ui: &mut Ui, scene: &mut Scene) {
    match scene {
        Scene::Blank => {}
        Scene::Look(survey) => wizard::look(ui, survey),
        Scene::Choose(region) => wizard::choose(ui, region),
        Scene::Install(lines, outcome) => wizard::install(ui, lines, *outcome),
        Scene::Head(step) => {
            let _pressed = wizard::header(ui, *step);
        }
        Scene::Foot(step, outcome) => {
            let _pressed = wizard::footer(ui, *step, *outcome, false);
        }
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
        ("choose", Scene::Choose(1)),
        ("install", Scene::Install(log(false), None)),
        ("install-done", Scene::Install(log(true), Some(true))),
        ("head", Scene::Head(wizard::Step::Choose)),
        ("foot-choose", Scene::Foot(wizard::Step::Choose, None)),
        ("foot-done", Scene::Foot(wizard::Step::Install, Some(true))),
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
