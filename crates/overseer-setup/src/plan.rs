use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) what: String,
    pub(crate) detail: String,
    pub(crate) blocking: bool,
}

impl Finding {
    fn ok(what: &str, detail: impl Into<String>) -> Self {
        Self {
            what: what.to_owned(),
            detail: detail.into(),
            blocking: false,
        }
    }

    fn stop(what: &str, detail: impl Into<String>) -> Self {
        Self {
            what: what.to_owned(),
            detail: detail.into(),
            blocking: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Survey {
    pub(crate) findings: Vec<Finding>,
    pub(crate) region: Option<usize>,
}

impl Survey {
    pub(crate) fn blocked(&self) -> bool {
        self.findings.iter().any(|f| f.blocking)
    }
}

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
        let ours = env!("CARGO_PKG_VERSION");
        findings.push(if carried && newer(&version, ours) {
            Finding::stop(
                "Already installed",
                format!("Version {version} is newer than this setup, which is {ours}. Download the latest setup from the releases page instead"),
            )
        } else {
            Finding::ok(
                "Already installed",
                format!("Version {version}. Updating keeps your lineups, notes, settings and region"),
            )
        });
    }
    let region = installed(root, "region")
        .and_then(|saved| REGIONS.iter().position(|(key, _)| *key == saved));

    Survey { findings, region }
}

static PROBES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn writable(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
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
        Err(e) => Err(cant_write(&e)),
    }
}

/// Why a folder can't be written to, in words, with Windows' own code kept
/// at the end for anybody who searches it.
fn cant_write(e: &std::io::Error) -> String {
    let said = match e.raw_os_error() {
        Some(5) => {
            "Windows won't let setup write here. Pick a folder in your user folder, or run setup again from your Downloads"
        }
        Some(32 | 33) => {
            "Another program has this folder open. Close Overseer if it's running, then press Check Again"
        }
        Some(112) => "The drive is full. Free up some space, then press Check Again",
        _ => "Can't write here",
    };
    format!("{said}. ({e})")
}

fn installed(root: &Path, field: &str) -> Option<String> {
    let raw = std::fs::read_to_string(root.join(".overseer").join("installed.json")).ok()?;
    // One field out of a file the installer owns, so a string search does,
    // and a marker that gains a field cannot stop the wizard starting.
    // Only at the top, since the marker also has a Python version nested in it.
    let key = format!("\"{field}\"");
    let at = raw.match_indices(&key).map(|(i, _)| i).find(|&i| {
        let before = raw.get(..i).unwrap_or_default();
        before.matches('{').count() == before.matches('}').count() + 1
    })?;
    let rest = raw.get(at..)?;
    let open = rest.find(':')?;
    let tail = rest.get(open + 1..)?.trim_start();
    let quoted = tail.strip_prefix('"')?;
    let end = quoted.find('"')?;
    quoted.get(..end).map(str::to_owned)
}

/// Whether version `a` is newer than `b`, both like 2.3.0. One that doesn't
/// read as three numbers is never newer.
fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u32>> {
        let parts: Vec<u32> = v
            .split('.')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        (parts.len() == 3).then_some(parts)
    };
    matches!((parts(a), parts(b)), (Some(a), Some(b)) if a > b)
}

pub(crate) const REGIONS: [(&str, &str); 6] = [
    ("na", "North America"),
    ("eu", "Europe"),
    ("ap", "Asia Pacific"),
    ("kr", "Korea"),
    ("latam", "Latin America"),
    ("br", "Brazil"),
];

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

    use super::{installed, newer, survey};

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

    #[test]
    fn a_carried_app_never_unpacks_over_a_checkout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(survey(&root, true).blocked());
        assert!(!survey(&root, false).blocked(), "a repair of it is fine");
    }

    #[test]
    fn only_a_higher_version_is_newer() {
        assert!(newer("2.10.0", "2.3.0"));
        assert!(!newer("2.3.0", "2.3.0"));
        assert!(!newer("2.2.9", "2.3.0"));
        assert!(!newer("2.4.0-rc.1", "2.3.0"));
    }

    #[test]
    fn a_version_is_read_out_of_a_marker() {
        let dir = std::env::temp_dir().join("overseer-setup-test");
        let overseer = dir.join(".overseer");
        std::fs::create_dir_all(&overseer).unwrap();
        let file = overseer.join("installed.json");

        std::fs::write(&file, r#"{"pip": "1", "version": "2.34.0", "x": 1}"#).unwrap();
        assert_eq!(installed(&dir, "version").as_deref(), Some("2.34.0"));

        std::fs::write(
            &file,
            r#"{"python": {"version": "3.12.10"}, "version": "2.3.0"}"#,
        )
        .unwrap();
        assert_eq!(installed(&dir, "version").as_deref(), Some("2.3.0"));
        std::fs::write(&file, r#"{"version":"9.9.9"}"#).unwrap();
        assert_eq!(installed(&dir, "version").as_deref(), Some("9.9.9"));

        std::fs::write(&file, "{}").unwrap();
        assert_eq!(installed(&dir, "version"), None);

        std::fs::write(&file, "not json at all").unwrap();
        assert_eq!(installed(&dir, "version"), None);

        std::fs::write(&file, r#"{"version": "2.1.0", "region": "eu"}"#).unwrap();
        assert_eq!(survey(&dir, false).region, Some(1));
        std::fs::write(&file, r#"{"version": "2.1.0", "region": "moon"}"#).unwrap();
        assert_eq!(survey(&dir, false).region, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
