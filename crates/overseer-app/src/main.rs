//! The Valorant Overseer window.
//!
//! P0: prove the expensive claims before any of the look is built. It opens on
//! the integrated GPU, connects to the running backend, draws the board, and
//! costs nothing at all while sitting still. `--probe` prints what it found and
//! exits, which is how the gate checks the claims rather than trusting them.
//!
//! The plan, including why this is egui rather than a web view, is in
//! `crates/README.md`.

mod app;
mod board;
mod hotkey;
mod notes;
mod overlay;
mod panel;
mod perf;
mod probe;
mod settings;
mod shot;
mod sort;
mod tray;
mod view;

use std::path::PathBuf;

use eframe::NativeOptions;
use egui::ViewportBuilder;

/// The window's starting size. Wide enough for the board and the panel, and
/// under the height of a 1080p screen with a task bar on it.
const INITIAL_SIZE: [f32; 2] = [1280.0, 800.0];
/// Below this the board cannot draw a row without wrapping it.
const MINIMUM_SIZE: [f32; 2] = [520.0, 360.0];

fn main() -> eframe::Result {
    let root = install_root();
    if std::env::args().any(|a| a == "--probe") {
        probe::report(&root);
        return Ok(());
    }

    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Valorant Overseer")
            .with_inner_size(INITIAL_SIZE)
            .with_min_inner_size(MINIMUM_SIZE)
            .with_app_id("valorant-overseer"),
        // Asking for the integrated adapter is the single largest performance
        // decision in the app, and it is about the game rather than about us:
        // on a laptop with both, VALORANT wants the discrete GPU, and a
        // scoreboard has no business competing for it. Everything drawn here
        // is flat shapes, text and a shader that touches every pixel once.
        wgpu_options: probe::low_power_wgpu(),
        ..NativeOptions::default()
    };

    eframe::run_native(
        "Valorant Overseer",
        options,
        Box::new(move |cc| Ok(Box::new(app::Overseer::new(cc, &root)))),
    )
}

/// The directory holding `.overseer`, which is the directory holding the app.
///
/// An override comes first so the window can be run against a checkout while
/// an installed copy is also on the machine.
fn install_root() -> PathBuf {
    if let Some(from_env) = std::env::var_os("OVERSEER_ROOT") {
        return PathBuf::from(from_env);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            // target/debug/overseer.exe during development, and the install
            // directory itself once shipped. Walk up until .overseer is found
            // so both work without a flag.
            exe.ancestors()
                .find(|dir| dir.join(".overseer").is_dir())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("."))
}
