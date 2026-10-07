//! The Valorant Overseer window.
//!
//! It opens on the integrated GPU, draws the board the backend sends, and
//! costs nothing while nothing changes. `--probe` prints what it found and
//! exits, so the gate can check those claims. Why this is egui and not a web
//! view is in `crates/README.md`.

// A console program started from the shortcut gets a terminal of its own.
// A script that wants the startup lines still gets them through the handles
// it passes.
#![windows_subsystem = "windows"]

mod app;
mod board;
mod career;
mod controls;
mod header;
mod history;
mod hotkey;
mod instance;
mod lineups;
mod machine;
mod notes;
mod offline;
mod overlay;
mod panel;
mod perf;
mod probe;
mod settings;
mod shot;
mod sort;
mod tray;
mod view;

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use eframe::NativeOptions;
use egui::ViewportBuilder;

/// Wide enough for the board and the panel, and short enough for a 1080p
/// screen with a task bar.
const INITIAL_SIZE: [f32; 2] = [1280.0, 800.0];
/// Below this the board cannot draw a row without wrapping it.
const MINIMUM_SIZE: [f32; 2] = [520.0, 360.0];

fn main() -> eframe::Result {
    let root = install_root();
    if std::env::args().any(|a| a == "--probe") {
        probe::report(&root);
        return Ok(());
    }
    // A second click on the shortcut shows the window already open.
    let listener = match instance::claim() {
        instance::Start::Second => return Ok(()),
        instance::Start::First(listener) => Some(listener),
        instance::Start::Unguarded => None,
    };
    serve(&root);

    let mut viewport = ViewportBuilder::default()
        .with_title("Valorant Overseer")
        .with_inner_size(INITIAL_SIZE)
        .with_min_inner_size(MINIMUM_SIZE)
        // The masthead is the title bar, with its own buttons.
        .with_decorations(false)
        .with_app_id("valorant-overseer");
    if let Some(icon) = overseer_ui::art::logo_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = NativeOptions {
        viewport,
        // The integrated adapter, so a laptop with two GPUs leaves the
        // discrete one to VALORANT. Flat shapes, text and one shader that
        // touches each pixel once need nothing more.
        wgpu_options: probe::low_power_wgpu(),
        ..NativeOptions::default()
    };

    eframe::run_native(
        "Valorant Overseer",
        options,
        Box::new(move |cc| Ok(Box::new(app::Overseer::new(cc, &root, listener)))),
    )
}

/// Windows' `CREATE_NO_WINDOW`, for every program the window starts.
pub(crate) const NO_WINDOW: u32 = 0x0800_0000;
/// Windows' `BELOW_NORMAL_PRIORITY_CLASS`, for the backend and the clip
/// programs. The backend fetches whole lobbies' match histories in bursts,
/// and the game should win the CPU when they meet. Everything the backend
/// starts inherits it.
pub(crate) const BELOW_NORMAL: u32 = 0x0000_4000;

/// Starts the backend under `run.py`. With no install to run, the window just
/// waits for a backend.
fn serve(root: &Path) {
    let python = root.join(".venv").join("Scripts").join("python.exe");
    let launcher = root.join("backend").join("run.py");
    if !python.is_file() || !launcher.is_file() {
        return;
    }
    // `--parent` ends the backend when this process ends. Everything it
    // starts shares its hidden console, so nothing flashes on screen and it
    // can still be stopped cleanly.
    let started = std::process::Command::new(python)
        .arg(launcher)
        .args(["--prod", "--parent"])
        .arg(std::process::id().to_string())
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(NO_WINDOW | BELOW_NORMAL)
        .spawn();
    if let Err(why) = started {
        println!("backend not started: {why}");
    }
}

/// The directory holding `.overseer`, which is the directory holding the app.
fn install_root() -> PathBuf {
    // Lets a checkout run while an installed copy is also on the machine.
    if let Some(from_env) = std::env::var_os("OVERSEER_ROOT") {
        return PathBuf::from(from_env);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            // The exe sits in crates/target in a checkout and in the install
            // directory once shipped, and walking up finds both.
            exe.ancestors()
                .find(|dir| dir.join(".overseer").is_dir())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("."))
}
