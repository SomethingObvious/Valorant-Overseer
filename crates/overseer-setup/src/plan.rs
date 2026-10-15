//! What the wizard works out before it touches anything, kept apart from the
//! window so all of it can be tested without one.

use std::path::Path;

/// One thing the wizard checked about this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    /// What was looked at.
    pub(crate) what: String,
    /// What was found.
    pub(crate) detail: String,
    /// Whether it stops the install.
    pub(crate) blocking: bool,
}

impl Finding {
    /// Something that is fine.
    fn ok(what: &str, detail: impl Into<String>) -> Self {
        Self {
            what: what.to_owned(),
            detail: detail.into(),
            blocking: false,
        }
    }

    /// Something that stops the install, said with what to do about it.
    fn stop(what: &str, detail: impl Into<String>) -> Self {
        Self {
            what: what.to_owned(),
            detail: detail.into(),
            blocking: true,
        }
    }
}

/// Everything the wizard found, in the order it is worth reading.
#[derive(Debug, Clone, Default)]
pub(crate) struct Survey {
    /// The findings themselves.
    pub(crate) findings: Vec<Finding>,
    /// The region an install already here was set to, as its place in
    /// [`REGIONS`]. An update keeps it.
    pub(crate) region: Option<usize>,
}

impl Survey {
    /// Whether anything found stops the install.
    pub(crate) fn blocked(&self) -> bool {
        self.findings.iter().any(|f| f.blocking)
    }
}

/// Looks at the machine without changing anything, since nobody has agreed to
/// anything yet. A setup that `carried` the app brings its own installer, and
/// its folder may not exist until the install makes it.
pub(crate) fn survey(root: &Path, carried: bool) -> Survey {
    let mut findings = Vec::new();

    findings.push(if cfg!(target_os = "windows") {
        Finding::ok(
            "Windows",
            "This is the only platform the Riot client has a local API on",
        )
    } else {
        Finding::stop(
            "Windows",
            "Valorant Overseer reads the Riot client, which is Windows only",
        )
    });

    let scripts = root.join("scripts").join("install.ps1");
    findings.push(if carried {
        Finding::ok(
            "Installer",
            format!("Inside this setup, and it installs to {}", root.display()),
        )
    } else if scripts.is_file() {
        Finding::ok(
            "Installer",
            "Found, and it does the work this wizard asks for",
        )
    } else {
        Finding::stop(
            "Installer",
            "scripts\\install.ps1 is missing, so this copy is incomplete",
        )
    });

    let nearest = root.ancestors().find(|dir| dir.is_dir()).unwrap_or(root);
    findings.push(match writable(nearest) {
        // A release's files are stripped of comments, so unpacking one over a
        // checkout would overwrite the source and any work not yet committed.
        Ok(()) if carried && root.join(".git").exists() => Finding::stop(
            "This folder",
            "It's a copy of the source, and this setup would overwrite it. Set OVERSEER_ROOT to install somewhere else.",
        ),
        Ok(()) => Finding::ok("This folder", "Writable, so nothing needs admin rights"),
        Err(why) => Finding::stop("This folder", why),
    });

    // The installer fetches a pinned Python when there is none, so this is
    // never a reason to stop.
    let venv = root.join(".venv").join("Scripts").join("python.exe");
    findings.push(if venv.is_file() {
        Finding::ok(
            "Python",
            "Already set up here, so the install will be a quick repair",
        )
    } else {
        Finding::ok(
            "Python",
            "Not set up yet, so the installer fetches a pinned copy and checks it",
        )
    });

    if let Some(version) = installed(root, "version") {
        findings.push(Finding::ok(
            "Already installed",
            format!("Version {version}. Updating keeps your lineups, notes, settings and region"),
        ));
    }
    let region = installed(root, "region")
        .and_then(|saved| REGIONS.iter().position(|(key, _)| *key == saved));

    Survey { findings, region }
}

/// How many write probes this process has made, to name each one apart.
static PROBES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Whether a directory can be written to, said the way a person would.
fn writable(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    // Named for this probe alone. Two surveys of one folder at once (the tests
    // do this) would otherwise trip over each other's half-deleted file.
    let probe = root.join(format!(
        ".overseer-write-test-{}-{}",
        std::process::id(),
        PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    match std::fs::write(&probe, b"x") {
        Ok(()) => {
            drop(std::fs::remove_file(&probe));
            Ok(())
        }
        Err(e) => Err(format!("Can't write here: {e}")),
    }
}

/// The version already installed, from the marker the installer writes.
/// One field of the marker an install leaves, like its version or region.
fn installed(root: &Path, field: &str) -> Option<String> {
    let raw = std::fs::read_to_string(root.join(".overseer").join("installed.json")).ok()?;
    // One field out of a file the installer owns, so a string search does,
    // and a marker that gains a field cannot stop the wizard starting.
    let at = raw.find(&format!("\"{field}\""))?;
    let rest = raw.get(at..)?;
    let open = rest.find(':')?;
    let tail = rest.get(open + 1..)?.trim_start();
    let quoted = tail.strip_prefix('"')?;
    let end = quoted.find('"')?;
    quoted.get(..end).map(str::to_owned)
}

/// The regions the backend knows, and what they are called.
pub(crate) const REGIONS: [(&str, &str); 6] = [
    ("na", "North America"),
    ("eu", "Europe"),
    ("ap", "Asia Pacific"),
    ("kr", "Korea"),
    ("latam", "Latin America"),
    ("br", "Brazil"),
];

/// The arguments the installer is given for a region.
pub(crate) fn install_args(script: &Path, region: &str) -> Vec<String> {
    vec![
        "-NoProfile".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-File".to_owned(),
        script.display().to_string(),
        "-Region".to_owned(),
        region.to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{installed, survey};

    /// A folder that is not an install must say so rather than offering to
    /// install into it.
    #[test]
    fn a_folder_that_is_not_an_install_is_blocked() {
        let found = survey(Path::new("Z:/nowhere-at-all"), false);
        assert!(found.blocked());
        assert!(
            found
                .findings
                .iter()
                .any(|f| f.blocking && f.what == "Installer")
        );
    }

    /// This repository is an install as well, and a release unpacked over it
    /// would replace the source with stripped copies.
    #[test]
    fn a_carried_app_never_unpacks_over_a_checkout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(survey(&root, true).blocked());
        assert!(!survey(&root, false).blocked(), "a repair of it is fine");
    }

    /// The marker is read with a string search, so these are the cases a
    /// search could get wrong.
    #[test]
    fn a_version_is_read_out_of_a_marker() {
        let dir = std::env::temp_dir().join("overseer-setup-test");
        let overseer = dir.join(".overseer");
        std::fs::create_dir_all(&overseer).unwrap();
        let file = overseer.join("installed.json");

        std::fs::write(&file, r#"{"pip": "1", "version": "2.34.0", "x": 1}"#).unwrap();
        assert_eq!(installed(&dir, "version").as_deref(), Some("2.34.0"));

        std::fs::write(&file, r#"{"version":"9.9.9"}"#).unwrap();
        assert_eq!(installed(&dir, "version").as_deref(), Some("9.9.9"));

        std::fs::write(&file, "{}").unwrap();
        assert_eq!(installed(&dir, "version"), None);

        std::fs::write(&file, "not json at all").unwrap();
        assert_eq!(installed(&dir, "version"), None);

        // An update keeps the region set before, and one it doesn't know
        // is asked for again.
        std::fs::write(&file, r#"{"version": "2.1.0", "region": "eu"}"#).unwrap();
        assert_eq!(survey(&dir, false).region, Some(1));
        std::fs::write(&file, r#"{"version": "2.1.0", "region": "moon"}"#).unwrap();
        assert_eq!(survey(&dir, false).region, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
