//! Running the installer, and reading its output as it comes.
//!
//! On a thread with a channel, for the same reason the window's bridge is:
//! the installer takes a minute when it has to fetch Python, and a window
//! that stops painting for a minute is a window somebody force quits.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

use crate::plan::{Profile, install_args};

/// What the installer says while it works.
#[derive(Debug, Clone)]
pub(crate) enum Line {
    /// A line it printed.
    Said(String),
    /// It finished, with whether it worked.
    Done(bool),
}

/// A running installer.
#[derive(Debug)]
pub(crate) struct Running {
    lines: Receiver<Line>,
}

impl Running {
    /// Everything it has said since the last look.
    pub(crate) fn drain(&self) -> Vec<Line> {
        self.lines.try_iter().collect()
    }

    /// The next thing it says, or nothing within the time given.
    ///
    /// Only the test uses this: the window is driven by repaints and reads
    /// whatever has arrived, while a test wants to wait for an answer
    /// without polling in a sleep loop, which this repository disallows for
    /// good reasons that also apply here.
    #[cfg(test)]
    fn next_within(&self, timeout: std::time::Duration) -> Option<Line> {
        self.lines.recv_timeout(timeout).ok()
    }
}

/// Starts the installer and returns something to read it with.
pub(crate) fn start<W>(root: &Path, profile: Profile, region: &str, wake: W) -> Running
where
    W: Fn() + Send + 'static,
{
    let (tx, lines) = channel();
    let script = root.join("scripts").join("install.ps1");
    let args = install_args(&script, profile, region);
    let root = root.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("overseer-install".to_owned())
        .spawn(move || {
            let child = Command::new("powershell.exe")
                .args(&args)
                .current_dir(&root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let mut child = match child {
                Ok(child) => child,
                Err(e) => {
                    drop(tx.send(Line::Said(format!("Could not start the installer: {e}"))));
                    drop(tx.send(Line::Done(false)));
                    wake();
                    return;
                }
            };
            // Both streams matter: the installer says what it is doing on one and
            // why it stopped on the other, and a wizard that shows only the happy
            // half is a wizard that says "failed" with no reason under it.
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    drop(tx.send(Line::Said(line)));
                    wake();
                }
            }
            if let Some(err) = child.stderr.take() {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    drop(tx.send(Line::Said(line)));
                    wake();
                }
            }
            let ok = child.wait().is_ok_and(|status| status.success());
            drop(tx.send(Line::Done(ok)));
            wake();
        });
    if let Err(e) = spawned {
        // The channel is already closed in this case, so the window will see
        // nothing at all unless it is told here.
        println!("could not start the installer thread: {e}");
    }
    Running { lines }
}

/// Runs the installer with no window at all, for a repair or a script.
///
/// The output goes where the caller's does, which for a console is the
/// console and for a scheduled task is the log it already keeps.
pub(crate) fn silent(root: &Path, profile: Profile, region: &str) -> i32 {
    let script = root.join("scripts").join("install.ps1");
    let args = install_args(&script, profile, region);
    Command::new("powershell.exe")
        .args(&args)
        .current_dir(root)
        .status()
        .map_or(1, |status| status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Line, silent, start};
    use crate::plan::Profile;

    /// A machine with no installer at all still has to finish rather than
    /// hang, because the window has no other way out of the running step.
    #[test]
    fn a_missing_installer_still_finishes() {
        let running = start(Path::new("Z:/nowhere"), Profile::App, "na", || {});
        let mut done = None;
        let deadline = std::time::Duration::from_secs(5);
        while let Some(line) = running.next_within(deadline) {
            if let Line::Done(ok) = line {
                done = Some(ok);
                break;
            }
        }
        assert_eq!(done, Some(false), "the installer never reported an ending");
    }

    /// The silent path returns the installer's own exit code, which is what
    /// a script checks.
    #[test]
    fn silent_reports_a_failure_as_a_failure() {
        assert_ne!(silent(Path::new("Z:/nowhere"), Profile::App, "na"), 0);
    }
}
