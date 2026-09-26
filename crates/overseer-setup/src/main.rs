//! The installer, as a window.
//!
//! Three steps: what this machine has, what you want installed, and then the
//! doing of it with the log in front of you rather than behind a spinner.
//!
//! It does not reimplement the installer. `scripts/install.ps1` is tested,
//! knows how to fetch a pinned Python, repair a virtual environment and write
//! the markers, and a second copy of that in Rust would be a second thing to
//! keep right. This asks the questions, in a window that looks like the app,
//! and then runs it.
//!
//! `--silent --profile app|cli|both --region na` skips the window entirely,
//! for a repair or a scripted reinstall.

#![windows_subsystem = "windows"]

mod plan;
mod run;
mod shot;
mod wizard;

use std::path::PathBuf;

use eframe::NativeOptions;
use egui::ViewportBuilder;

/// The window's size. Big enough for the log, small enough to feel like a
/// dialogue rather than an application.
const SIZE: [f32; 2] = [720.0, 520.0];

fn main() -> eframe::Result {
    let root = install_root();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--silent") {
        let profile = flag(&args, "--profile").map_or(plan::Profile::App, |v| match v.as_str() {
            "cli" => plan::Profile::Cli,
            "both" => plan::Profile::Both,
            _ => plan::Profile::App,
        });
        let region = flag(&args, "--region").unwrap_or_else(|| "na".to_owned());
        std::process::exit(run::silent(&root, profile, &region));
    }

    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Valorant Overseer setup")
            .with_inner_size(SIZE)
            .with_min_inner_size(SIZE)
            .with_resizable(false),
        ..NativeOptions::default()
    };
    eframe::run_native(
        "Valorant Overseer setup",
        options,
        Box::new(move |cc| Ok(Box::new(wizard::Wizard::new(cc, root)))),
    )
}

/// The value after a flag, when there is one.
fn flag(args: &[String], name: &str) -> Option<String> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).filter(|v| !v.starts_with("--")).cloned()
}

/// The directory holding the install, which is the one holding `scripts`.
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

#[cfg(test)]
mod tests {
    use super::flag;

    #[test]
    fn a_flag_without_a_value_reads_as_absent() {
        let args: Vec<String> = ["setup.exe", "--silent", "--profile", "cli", "--region"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(flag(&args, "--profile").as_deref(), Some("cli"));
        assert_eq!(
            flag(&args, "--region"),
            None,
            "a trailing flag has no value"
        );
        assert_eq!(flag(&args, "--nothing"), None);
    }

    /// A flag followed by another flag has not been given a value, however
    /// much it looks like one.
    #[test]
    fn a_flag_does_not_eat_the_next_flag() {
        let args: Vec<String> = ["setup.exe", "--profile", "--silent"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(flag(&args, "--profile"), None);
    }
}
