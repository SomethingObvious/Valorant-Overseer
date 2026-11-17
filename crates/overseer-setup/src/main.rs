//! The installer as a window: it asks its questions, then runs
//! `scripts/install.ps1` with the log on screen, so there is only one installer
//! to keep right. `--silent --region na` skips the window.

#![windows_subsystem = "windows"]

mod payload;
mod plan;
mod run;
mod shot;
mod wizard;

use std::path::PathBuf;

use eframe::NativeOptions;
use egui::ViewportBuilder;

const SIZE: [f32; 2] = [720.0, 520.0];

fn main() -> eframe::Result {
    let carried = std::env::current_exe()
        .ok()
        .filter(|exe| payload::carried(exe).is_some());
    let root = match (&carried, payload::home()) {
        (Some(_), Some(home)) => home,
        _ => install_root(),
    };
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--silent") {
        let region = flag(&args, "--region").unwrap_or_else(|| "na".to_owned());
        std::process::exit(run::silent(&root, &region, carried.as_deref()));
    }

    let mut viewport = ViewportBuilder::default()
        .with_title("Valorant Overseer Setup")
        .with_inner_size(SIZE)
        .with_min_inner_size(SIZE)
        .with_resizable(false)
        .with_decorations(false);
    if let Some(icon) = overseer_ui::art::logo_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = NativeOptions {
        viewport,
        ..NativeOptions::default()
    };
    eframe::run_native(
        "Valorant Overseer setup",
        options,
        Box::new(move |cc| Ok(Box::new(wizard::Wizard::new(cc, root, carried)))),
    )
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).filter(|v| !v.starts_with("--")).cloned()
}

fn install_root() -> PathBuf {
    if let Some(from_env) = std::env::var_os("OVERSEER_ROOT") {
        return PathBuf::from(from_env);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.ancestors()
                .find(|dir| dir.join("scripts").join("install.ps1").is_file())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("."))
}
