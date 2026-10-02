//! Runs the installer on a thread and passes its output back line by line,
//! because fetching Python takes a minute and a window that stops painting
//! that long gets force quit.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::Builder;

use crate::payload;
use crate::plan::install_args;

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

    /// The next thing it says within `timeout`, so a test can wait without a
    /// sleep loop.
    #[cfg(test)]
    fn next_within(&self, timeout: std::time::Duration) -> Option<Line> {
        self.lines.recv_timeout(timeout).ok()
    }
}

/// Starts the installer and returns something to read it with. A setup that
/// `carried` the app unpacks it first.
pub(crate) fn start<W>(root: &Path, region: &str, carried: Option<&Path>, wake: W) -> Running
where
    W: Fn() + Clone + Send + 'static,
{
    let (tx, lines) = channel();
    let script = root.join("scripts").join("install.ps1");
    let args = install_args(&script, region);
    let root = root.to_path_buf();
    let carried = carried.map(Path::to_path_buf);
    // Kept back, because a thread that never starts takes its sender with it.
    let (tx_back, wake_back) = (tx.clone(), wake.clone());
    let spawned = Builder::new()
        .name("overseer-install".to_owned())
        .spawn(move || {
            if let Some(exe) = carried {
                if let Err(why) = payload::unpack(&exe, &root) {
                    fail(&tx, &wake, &format!("  x {why}"));
                    return;
                }
                drop(tx.send(Line::Said(format!(
                    "  + Unpacked the app into {}",
                    root.display()
                ))));
                wake();
            }
            watch(&args, &root, &tx, &wake);
        });
    if let Err(e) = spawned {
        fail(
            &tx_back,
            &wake_back,
            &format!("Couldn't start the installer: {e}"),
        );
    }
    Running { lines }
}

/// Runs the installer, passes on every line it prints, then says how it ended.
fn watch<W>(args: &[String], root: &Path, tx: &Sender<Line>, wake: &W)
where
    W: Fn() + Clone + Send,
{
    let child = Command::new("powershell.exe")
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => {
            fail(tx, wake, &format!("Couldn't start the installer: {e}"));
            return;
        }
    };
    // Stdout and stderr are read at the same time, because the installer
    // blocks as soon as it fills a pipe that nobody is reading. pip and
    // PowerShell itself say what went wrong on stderr.
    std::thread::scope(|s| {
        if let Some(err) = child.stderr.take() {
            let (err_tx, err_wake) = (tx.clone(), wake.clone());
            let reader = Builder::new().spawn_scoped(s, move || pass(err, &err_tx, &err_wake));
            if let Err(e) = reader {
                drop(tx.send(Line::Said(format!(
                    "Couldn't read the installer's errors: {e}"
                ))));
            }
        }
        if let Some(out) = child.stdout.take() {
            pass(out, tx, wake);
        }
    });
    let ok = child.wait().is_ok_and(|status| status.success());
    drop(tx.send(Line::Done(ok)));
    wake();
}

/// Sends each line from one of the installer's pipes until it closes.
fn pass(pipe: impl Read, tx: &Sender<Line>, wake: &impl Fn()) {
    // Bytes rather than `lines()`, which gives up at the first line that isn't
    // UTF-8 and loses the rest. Windows PowerShell writes in the console's
    // code page, so an accent in the user's folder name is enough.
    for line in BufReader::new(pipe).split(b'\n').map_while(Result::ok) {
        let text = String::from_utf8_lossy(&line);
        let text = text.strip_suffix('\r').unwrap_or(&text);
        drop(tx.send(Line::Said(text.to_owned())));
        wake();
    }
}

/// Tells the window the install failed, and why.
fn fail(tx: &Sender<Line>, wake: &impl Fn(), why: &str) {
    drop(tx.send(Line::Said(why.to_owned())));
    drop(tx.send(Line::Done(false)));
    wake();
}

/// Runs the installer with no window, for a repair or a script, and returns
/// its exit code.
pub(crate) fn silent(root: &Path, region: &str, carried: Option<&Path>) -> i32 {
    if let Some(exe) = carried
        && payload::unpack(exe, root).is_err()
    {
        return 1;
    }
    let script = root.join("scripts").join("install.ps1");
    let args = install_args(&script, region);
    Command::new("powershell.exe")
        .args(&args)
        .current_dir(root)
        .status()
        .map_or(1, |status| status.code().unwrap_or(1))
}

/// Opens the window, which is what the desktop shortcut opens.
pub(crate) fn open(root: &Path) {
    drop(
        Command::new(root.join("overseer.exe"))
            .current_dir(root)
            .spawn(),
    );
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::{Line, Running, silent, start};

    /// Everything the installer says and how it ended, or no ending when it
    /// goes quiet for `patience`.
    fn finish(running: &Running, patience: Duration) -> (Vec<String>, Option<bool>) {
        let mut said = Vec::new();
        while let Some(line) = running.next_within(patience) {
            match line {
                Line::Said(text) => said.push(text),
                Line::Done(ok) => return (said, Some(ok)),
            }
        }
        (said, None)
    }

    /// The window has no other way out of the running step.
    #[test]
    fn a_missing_installer_still_finishes() {
        let running = start(Path::new("Z:/nowhere"), "na", None, || {});
        let (_, done) = finish(&running, Duration::from_secs(5));
        assert_eq!(done, Some(false), "the installer never reported an ending");
    }

    /// 100 KB on stderr while stdout is still open, starting with a byte
    /// that isn't UTF-8. Reading one pipe at a time leaves both sides waiting
    /// on each other.
    #[test]
    fn a_noisy_stderr_does_not_stall_the_installer() {
        let root = std::env::temp_dir().join("overseer-setup-noisy");
        let scripts = root.join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(
            scripts.join("install.ps1"),
            "[Console]::Out.WriteLine('  + before')\n\
             $b = [byte[]](@(0x82, 10) + @(120) * 100000 + @(10))\n\
             [Console]::OpenStandardError().Write($b, 0, $b.Length)\n\
             [Console]::Out.WriteLine('  + after')\n",
        )
        .unwrap();

        let running = start(&root, "na", None, || {});
        let (said, done) = finish(&running, Duration::from_secs(30));
        // A stalled installer still has the folder open, and the assert below
        // says more about that than a failed delete would.
        drop(std::fs::remove_dir_all(&root));

        let sizes: Vec<usize> = said.iter().map(String::len).collect();
        assert_eq!(done, Some(true), "stalled after lines of {sizes:?} bytes");
        let (out, err): (Vec<&String>, Vec<&String>) =
            said.iter().partition(|line| line.starts_with("  + "));
        assert_eq!(out, ["  + before", "  + after"]);
        let wall = "x".repeat(100_000);
        assert!(
            err == ["\u{FFFD}", wall.as_str()],
            "stderr came through as lines of {sizes:?} bytes"
        );
    }

    /// A script checks the exit code.
    #[test]
    fn silent_reports_a_failure_as_a_failure() {
        assert_ne!(silent(Path::new("Z:/nowhere"), "na", None), 0);
    }
}
