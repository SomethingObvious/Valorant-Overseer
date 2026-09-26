//! What the window remembers between runs.
//!
//! Its own small file beside the terminal front end's, rather than eframe's
//! persistence, because this is a handful of choices a person made and they
//! belong somewhere they can be read, edited and deleted. A settings file
//! nobody can find is a settings file nobody can fix.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How much the window is allowed to spend on looking good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Quality {
    /// Start rich, and drop to [`Self::Efficient`] if the frames say so.
    #[default]
    Auto,
    /// No movement, no shader, nothing that costs a frame. Same information,
    /// same layout, calmer.
    Efficient,
    /// Everything the design allows.
    Rich,
}

impl Quality {
    /// What to show in the settings, and in the line announcing a downgrade.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Efficient => "efficient",
            Self::Rich => "rich",
        }
    }
}

/// Everything the window remembers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// How much to spend on looking good.
    pub(crate) quality: Quality,
    /// Columns switched off by hand, by their heading.
    pub(crate) hidden_columns: Vec<String>,
    /// Whether the detail panel takes the right of the window.
    pub(crate) panel: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // Everything on. A person who wants less can say so, and will find
        // the switch; a person who never opens the settings should be seeing
        // what the app can do rather than a subset somebody chose for them.
        Self {
            quality: Quality::Auto,
            hidden_columns: Vec::new(),
            panel: true,
        }
    }
}

/// Where the file lives: beside the bridge's, in the install's own directory.
fn path(root: &Path) -> PathBuf {
    root.join(".overseer").join("app.json")
}

/// Reads the settings, or the defaults.
///
/// A missing file is the ordinary first run. A corrupt one is somebody having
/// edited it, and the defaults are a better answer than a refusal to start.
pub(crate) fn load(root: &Path) -> Settings {
    std::fs::read_to_string(path(root))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Writes the settings, and says nothing if it cannot.
///
/// This is called when a person changes something, and the something has
/// already happened on screen by then. A disk that will not take it is worth
/// no interruption at all.
pub(crate) fn save(root: &Path, settings: &Settings) {
    let file = path(root);
    if let Some(dir) = file.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        drop(std::fs::write(file, text));
    }
}

#[cfg(test)]
mod tests {
    use super::{Quality, Settings, load, save};

    /// Every tier has to be able to say its own name, because the footer
    /// and the settings screen both print it.
    #[test]
    fn every_tier_has_a_name() {
        for tier in [Quality::Auto, Quality::Efficient, Quality::Rich] {
            assert!(!tier.label().is_empty());
        }
    }

    /// A file somebody has broken must not stop the app: the defaults are an
    /// answer, and refusing to start over a settings file is not.
    #[test]
    fn a_broken_file_reads_as_the_defaults() {
        let root = std::env::temp_dir().join("overseer-settings-test");
        let dir = root.join(".overseer");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.json"), "{ this is not json").unwrap();
        assert_eq!(load(&root).quality, Quality::Auto);

        save(
            &root,
            &Settings {
                quality: Quality::Efficient,
                ..Settings::default()
            },
        );
        assert_eq!(load(&root).quality, Quality::Efficient);
        assert!(load(&root).panel, "a saved file lost a field it never set");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A machine with nothing written yet is the commonest case of all.
    #[test]
    fn nothing_written_yet_is_the_defaults() {
        let root = std::path::Path::new("Z:/nowhere-at-all");
        assert_eq!(load(root).quality, Quality::Auto);
    }
}
