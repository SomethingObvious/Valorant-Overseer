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
mod update;
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
    // The video process, which plays lineup clips in the window named after
    // the flag, and nothing else.
    let mut args = std::env::args().skip_while(|a| a != "--video").skip(1);
    if let Some(parent) = args.next().and_then(|p| p.parse::<isize>().ok()) {
        std::process::exit(overseer_native::serve(parent));
    }
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
    let broken = serve(&root);

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
    // Taken by whichever run gets as far as making the app.
    let listener = std::cell::Cell::new(listener);
    let made = std::cell::Cell::new(false);
    let run = |draw: probe::Draw| {
        let mut options = NativeOptions {
            viewport: viewport.clone(),
            renderer: if draw == probe::Draw::OpenGl {
                eframe::Renderer::Glow
            } else {
                eframe::Renderer::Wgpu
            },
            // The integrated adapter, so a laptop with two GPUs leaves the
            // discrete one to VALORANT. Flat shapes, text and one shader that
            // touches each pixel once need nothing more.
            wgpu_options: probe::low_power_wgpu(draw),
            ..NativeOptions::default()
        };

        // Windows shows a window as it maximizes it, so one saved maximized
        // would show blank before its first frame. It opens unmaximized at its
        // saved size, which is the screen it filled, and maximizes once drawn.
        let maximize = std::rc::Rc::new(std::cell::Cell::new(false));
        let saved = std::rc::Rc::clone(&maximize);
        options.window_builder = Some(Box::new(move |builder| {
            saved.set(builder.maximized == Some(true));
            builder.with_maximized(false)
        }));

        eframe::run_native(
            "Valorant Overseer",
            options,
            Box::new(|cc| {
                made.set(true);
                Ok(Box::new(app::Overseer::new(
                    cc,
                    &root,
                    (listener.take(), maximize.get(), broken.clone()),
                )))
            }),
        )
    };
    // Each way that fails before the app is made gives way to the next. One
    // that fails after it ran has nothing to do with drawing.
    let mut ran = Ok(());
    for draw in probe::DRAWS {
        if draw == probe::Draw::Software {
            overseer_native::one_core();
        }
        ran = run(draw);
        if ran.is_ok() || made.get() {
            break;
        }
    }
    ran
}

/// Windows' `CREATE_NO_WINDOW`, for every program the window starts.
pub(crate) const NO_WINDOW: u32 = 0x0800_0000;
/// Windows' `BELOW_NORMAL_PRIORITY_CLASS`, for the backend and the clip
/// programs. The backend fetches whole lobbies' match histories in bursts,
/// and the game should win the CPU when they meet. Everything the backend
/// starts inherits it.
pub(crate) const BELOW_NORMAL: u32 = 0x0000_4000;

/// Starts the backend under `run.py`, or says why it can't. With no install
/// to run, the window just waits for a backend.
fn serve(root: &Path) -> Option<String> {
    let python = root.join(".venv").join("Scripts").join("python.exe");
    let launcher = root.join("backend").join("run.py");
    if !python.is_file() || !launcher.is_file() {
        return None;
    }
    if let Some(why) = broken(root) {
        return Some(why);
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
    started
        .err()
        .map(|why| format!("Overseer's backend didn't start: {why}."))
}

/// Why the install in `root` can't run, when the Python its environment was
/// made from is gone. Updating or removing Python does that, and the
/// environment's own python.exe then exits as soon as it starts.
fn broken(root: &Path) -> Option<String> {
    let settings = std::fs::read_to_string(root.join(".venv").join("pyvenv.cfg")).ok()?;
    let home = settings.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "home").then(|| value.trim().to_owned())
    })?;
    let home = Path::new(&home);
    (!home.join("python.exe").is_file()).then(|| {
        let name = home.file_name().map_or_else(|| home.display().to_string(), |n| n.to_string_lossy().into_owned());
        format!(
            "Its Python ({name}) is gone, which happens when Python is updated or removed. Run              install.bat in Overseer's folder to repair it. Your settings and lineups are kept."
        )
    })
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

#[cfg(test)]
mod tests {
    use super::broken;

    /// An environment whose Python is gone is caught before it is started,
    /// and one whose Python is there, or no environment at all, is let be.
    #[test]
    fn a_venv_whose_python_is_gone_is_caught() {
        let root = std::env::temp_dir().join("overseer-broken-test");
        let venv = root.join(".venv");
        let python = root.join("python");
        std::fs::create_dir_all(&venv).expect("a test folder");
        std::fs::create_dir_all(&python).expect("a test folder");
        drop(std::fs::remove_file(venv.join("pyvenv.cfg")));
        assert_eq!(broken(&root), None, "no environment, nothing to say");
        let gone = root.join("Python312");
        let made = |home: &std::path::Path| {
            std::fs::write(
                venv.join("pyvenv.cfg"),
                format!(
                    "home = {}
version = 3.12.10
",
                    home.display()
                ),
            )
            .expect("a test file");
        };
        made(&gone);
        assert!(broken(&root).is_some_and(|why| why.contains("(Python312)")));
        std::fs::write(python.join("python.exe"), b"").expect("a test file");
        made(&python);
        assert_eq!(broken(&root), None);
        drop(std::fs::remove_dir_all(&root));
    }
}
