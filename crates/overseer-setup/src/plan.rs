//! What the wizard knows before it touches anything.
//!
//! Kept apart from the window on purpose. What this machine has, what a
//! profile needs, and what the installer will be asked to do are all
//! questions with answers that can be checked without opening a window, and
//! everything checkable lives here.

use std::path::Path;

/// Which front ends to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Profile {
    /// The window only.
    #[default]
    App,
    /// The terminal front end only.
    Cli,
    /// Both, sharing one backend and one set of settings.
    Both,
}

impl Profile {
    /// What the installer is told.
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Cli => "cli",
            Self::Both => "both",
        }
    }

    /// What the person chooses between.
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::App => "The window",
            Self::Cli => "The terminal",
            Self::Both => "Both",
        }
    }

    /// Why they would choose it.
    pub(crate) const fn about(self) -> &'static str {
        match self {
            Self::App => {
                "A real window: every column, the detail panel, and settings you can click."
            }
            Self::Cli => {
                "The scoreboard in a terminal. Lighter, keyboard only, and it starts instantly."
            }
            Self::Both => {
                "Install the two of them. They share one backend and one set of settings."
            }
        }
    }

    /// Which shortcut the installer should make.
    pub(crate) const fn shortcut(self) -> &'static str {
        match self {
            Self::App | Self::Both => "the window",
            Self::Cli => "the terminal",
        }
    }

    /// The three, in the order they are offered.
    pub(crate) const ALL: [Self; 3] = [Self::App, Self::Cli, Self::Both];
}

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
}

impl Survey {
    /// Whether anything found stops the install.
    pub(crate) fn blocked(&self) -> bool {
        self.findings.iter().any(|f| f.blocking)
    }
}

/// Looks at the machine, without changing any of it.
///
/// Everything here is a read: a file that exists, a directory that can be
/// written to, a version already recorded. A wizard that alters something
/// before the person has agreed to anything is a wizard nobody trusts.
pub(crate) fn survey(root: &Path) -> Survey {
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
    findings.push(if scripts.is_file() {
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

    findings.push(match writable(root) {
        Ok(()) => Finding::ok("This folder", "Writable, so nothing needs administrator"),
        Err(why) => Finding::stop("This folder", why),
    });

    // Python is what the backend runs on. The installer will fetch a pinned
    // one if it is missing, so this is worth saying rather than worth
    // stopping for.
    let venv = root.join(".venv").join("Scripts").join("python.exe");
    findings.push(if venv.is_file() {
        Finding::ok(
            "Python",
            "Already set up here, so the install will be a quick repair",
        )
    } else {
        Finding::ok(
            "Python",
            "Not set up yet; the installer fetches a pinned copy and checks it",
        )
    });

    if let Some(version) = installed_version(root) {
        findings.push(Finding::ok(
            "Already installed",
            format!("Version {version}, being repaired"),
        ));
    }

    Survey { findings }
}

/// Whether a directory can be written to, said the way a person would.
fn writable(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    let probe = root.join(".overseer-write-test");
    match std::fs::write(&probe, b"x") {
        Ok(()) => {
            drop(std::fs::remove_file(&probe));
            Ok(())
        }
        Err(e) => Err(format!("Cannot write here: {e}")),
    }
}

/// The version already installed, from the marker the installer writes.
fn installed_version(root: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(root.join(".overseer").join("installed.json")).ok()?;
    // Deliberately not a JSON parser: this is one field out of a file the
    // installer owns, and a wizard that refuses to start because a marker
    // gained a field would be worse than one that says nothing.
    let at = raw.find("\"version\"")?;
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

/// The arguments the installer is given for a profile and a region.
pub(crate) fn install_args(script: &Path, profile: Profile, region: &str) -> Vec<String> {
    vec![
        "-NoProfile".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-File".to_owned(),
        script.display().to_string(),
        "-Region".to_owned(),
        region.to_owned(),
        "-Frontend".to_owned(),
        profile.key().to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Profile, REGIONS, Survey, install_args, installed_version, survey};

    #[test]
    fn every_profile_can_describe_itself() {
        for profile in Profile::ALL {
            assert!(!profile.key().is_empty());
            assert!(!profile.title().is_empty());
            assert!(!profile.about().is_empty());
            assert!(!profile.shortcut().is_empty());
        }
        // The keys are what the installer switches on, so they must differ.
        let keys: Vec<&str> = Profile::ALL.iter().map(|p| p.key()).collect();
        assert_eq!(keys.len(), 3);
        assert_ne!(keys.first(), keys.get(1));
        assert_ne!(keys.get(1), keys.get(2));
    }

    /// A folder that is not an install must say so rather than offering to
    /// install into it.
    #[test]
    fn a_folder_that_is_not_an_install_is_blocked() {
        let found = survey(Path::new("Z:/nowhere-at-all"));
        assert!(found.blocked());
        assert!(
            found
                .findings
                .iter()
                .any(|f| f.blocking && f.what == "Installer")
        );
    }

    /// The real copy is not blocked, which is the case that matters most.
    #[test]
    fn this_install_is_ready() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let found = survey(&root);
        assert!(!found.blocked(), "{:?}", found.findings);
        assert!(
            found.findings.iter().any(|f| f.what == "Already installed"),
            "the marker should have been read"
        );
    }

    /// The marker is read with a finger rather than a parser, so the cases
    /// worth pinning are the ones a finger gets wrong.
    #[test]
    fn a_version_is_read_out_of_a_marker() {
        let dir = std::env::temp_dir().join("overseer-setup-test");
        let overseer = dir.join(".overseer");
        std::fs::create_dir_all(&overseer).unwrap();
        let file = overseer.join("installed.json");

        std::fs::write(&file, r#"{"pip": "1", "version": "2.34.0", "x": 1}"#).unwrap();
        assert_eq!(installed_version(&dir).as_deref(), Some("2.34.0"));

        std::fs::write(&file, r#"{"version":"9.9.9"}"#).unwrap();
        assert_eq!(installed_version(&dir).as_deref(), Some("9.9.9"));

        std::fs::write(&file, "{}").unwrap();
        assert_eq!(installed_version(&dir), None);

        std::fs::write(&file, "not json at all").unwrap();
        assert_eq!(installed_version(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_installer_is_told_the_profile_and_the_region() {
        let args = install_args(Path::new("C:/x/install.ps1"), Profile::Cli, "eu");
        assert!(args.iter().any(|a| a == "cli"));
        assert!(args.iter().any(|a| a == "eu"));
        assert!(args.iter().any(|a| a == "-ExecutionPolicy"));
        // Not -Profile: that is a PowerShell automatic variable, and the
        // installer would refuse the parameter.
        assert!(args.iter().any(|a| a == "-Frontend"));
    }

    #[test]
    fn a_survey_with_nothing_wrong_is_not_blocked() {
        assert!(!Survey::default().blocked());
        assert_eq!(REGIONS.len(), 6);
    }
}
