//! What the window remembers between runs. It is a small JSON file in
//! .overseer, not eframe's own storage, so a person can find it and fix it by
//! hand.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::overlay::Corner;

/// How much the window is allowed to spend on looking good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Quality {
    /// Start rich, and drop to [`Self::Efficient`] if the frames say so.
    #[default]
    Auto,
    /// The same board with no movement and no shader.
    Efficient,
    /// Everything the design allows.
    Rich,
}

impl Quality {
    /// What to show in the settings, and in the line announcing a downgrade.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Efficient => "Efficient",
            Self::Rich => "Full",
        }
    }
}

/// Which way up a lineup map is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MapTurn {
    /// The attackers' spawn at the bottom, the way you walk onto it.
    #[default]
    Attack,
    /// The defenders' spawn at the bottom.
    Defend,
    /// The way Riot's own minimap is drawn.
    Drawn,
}

impl MapTurn {
    /// Every choice, in the order the settings list them.
    pub(crate) const ALL: [Self; 3] = [Self::Attack, Self::Defend, Self::Drawn];

    /// What the settings call it, and a sentence about it.
    pub(crate) const fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::Attack => (
                "Attackers at the Bottom",
                "Turn each map so the attackers' spawn is at the bottom",
            ),
            Self::Defend => (
                "Defenders at the Bottom",
                "Turn each map so the defenders' spawn is at the bottom",
            ),
            Self::Drawn => (
                "As Riot Draws It",
                "Leave each map the way the game's own minimap is drawn",
            ),
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
    /// The detail panel's width, once it has been dragged to one. Without
    /// one it fits the window.
    pub(crate) panel_width: Option<f32>,
    /// Whether the window minimizes itself when agent select ends. VALORANT
    /// minimizes when it loses the focus, so without this, closing a
    /// browser opened over the game brings this window up instead.
    pub(crate) step_aside: bool,
    /// Whether the overlay is on, and where.
    #[serde(flatten)]
    pub(crate) overlay: OverlaySettings,
    /// Whether the other side is drawn above your own.
    pub(crate) enemies_first: bool,
    /// How many past games the History screen shows.
    pub(crate) history_games: u32,
    /// How the Lineups screen plays clips and draws its maps.
    #[serde(flatten)]
    pub(crate) lineups: LineupSettings,
}

/// The overlay's settings, kept in the same file under the same names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct OverlaySettings {
    /// Whether the overlay shows itself in agent select and the first
    /// seconds of a match. The hotkey shows it any time either way.
    #[serde(rename = "overlay")]
    pub(crate) auto: bool,
    /// Which corner it parks in.
    pub(crate) corner: Corner,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        // It only ever shows itself before the first round's fight, so it
        // can be on from the start.
        Self {
            auto: true,
            corner: Corner::default(),
        }
    }
}

/// The Lineups screen's settings, kept in the same file under the same names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LineupSettings {
    /// How fast a lineup's clip plays when it first shows.
    pub(crate) clip_speed: f32,
    /// How loud it plays, from 0 to 100.
    pub(crate) clip_volume: f32,
    /// Whether picking a lineup plays its clip straight away.
    pub(crate) clip_autoplay: bool,
    /// Whether a clip starts over when it ends, the way short videos do.
    pub(crate) clip_loop: bool,
    /// How much of the window's height a clip may take beside the map.
    pub(crate) clip_size: f32,
    /// Which way up the lineup maps are drawn.
    pub(crate) map_turn: MapTurn,
    /// Whether every lineup's name shows on the map, not only the picked
    /// one's, when Lineups opens.
    pub(crate) lineup_names: bool,
    /// The colour an ability's area is drawn in, from the drawing colours,
    /// or by the lineup's side when there is none.
    pub(crate) area_colour: Option<String>,
    /// The agent a new lineup starts with.
    pub(crate) default_agent: String,
}

impl Default for LineupSettings {
    fn default() -> Self {
        Self {
            clip_speed: 1.25,
            // Silent until asked, since a clip often plays while the game's
            // own sound matters more.
            clip_volume: 0.0,
            clip_autoplay: true,
            clip_loop: true,
            clip_size: 0.63,
            map_turn: MapTurn::Attack,
            lineup_names: false,
            area_colour: None,
            default_agent: "Brimstone".to_owned(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        // Everything on, so somebody who never opens the settings still sees
        // what the app can do.
        Self {
            quality: Quality::Auto,
            hidden_columns: Vec::new(),
            panel: true,
            panel_width: None,
            step_aside: true,
            overlay: OverlaySettings::default(),
            // The five you cannot see in game are the five worth the top of
            // the window.
            enemies_first: true,
            // Two sessions' worth, and one page of Riot's history.
            history_games: 10,
            lineups: LineupSettings::default(),
        }
    }
}

impl Settings {
    /// How a lineup's clip plays when it first shows.
    pub(crate) const fn clip(&self) -> crate::lineups::Prefs {
        let lineups = &self.lineups;
        crate::lineups::Prefs {
            speed: lineups.clip_speed,
            volume: lineups.clip_volume,
            autoplay: lineups.clip_autoplay,
            looping: lineups.clip_loop,
            size: lineups.clip_size,
        }
    }
}

/// `.overseer/app.json`, beside the bridge's file.
fn path(root: &Path) -> PathBuf {
    root.join(".overseer").join("app.json")
}

/// Reads the settings, or the defaults when the file is missing or broken.
pub(crate) fn load(root: &Path) -> Settings {
    read_or_set_aside(&path(root))
}

/// Reads one of the window's JSON files, or the default when it is missing.
/// A file that is there but will not read is renamed to
/// `<file>.unreadable-<unix seconds>` first, because the next save writes the
/// default over it and whatever was in it would be gone.
pub(crate) fn read_or_set_aside<T: DeserializeOwned + Default>(file: &Path) -> T {
    match std::fs::read(file) {
        Ok(raw) => {
            if let Ok(value) = serde_json::from_slice(&raw) {
                return value;
            }
        }
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => return T::default(),
        Err(_) => {}
    }
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let mut aside = file.as_os_str().to_owned();
    aside.push(format!(".unreadable-{seconds}"));
    let aside = PathBuf::from(aside);
    match std::fs::rename(file, &aside) {
        Ok(()) => println!("unreadable, kept as {}", aside.display()),
        Err(why) => println!(
            "{} is unreadable and the next save replaces it, since moving it failed: {why}",
            file.display()
        ),
    }
    T::default()
}

/// Writes the settings, and gives up quietly if it cannot. The change has
/// already happened on screen, and a failed write is not worth interrupting.
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

    /// The footer and the settings screen both print a tier's name.
    #[test]
    fn every_tier_has_a_name() {
        for tier in [Quality::Auto, Quality::Efficient, Quality::Rich] {
            assert!(!tier.label().is_empty());
        }
    }

    /// A hand-broken file reads as the defaults and is moved aside intact,
    /// and saving after that works.
    #[test]
    fn a_broken_file_reads_as_the_defaults() {
        let root = std::env::temp_dir().join("overseer-settings-test");
        let dir = root.join(".overseer");
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.json"), "{ this is not json").unwrap();
        assert_eq!(load(&root).quality, Quality::Auto);
        let kept: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|file| {
                file.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("app.json.unreadable-"))
            })
            .map(|file| std::fs::read_to_string(file).unwrap())
            .collect();
        assert_eq!(kept, vec!["{ this is not json"]);
        assert!(!dir.join("app.json").exists());

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

    /// The groups are kept under the names the file always had, so a file
    /// from before they were grouped reads the same.
    #[test]
    fn grouped_settings_keep_their_names_in_the_file() {
        let read: Settings = serde_json::from_str(
            r#"{ "overlay": true, "corner": "top-left", "clip_speed": 2.0, "lineup_names": true }"#,
        )
        .unwrap();
        assert!(read.lineups.lineup_names);
        assert!(read.overlay.auto);
        assert_eq!(read.overlay.corner, crate::overlay::Corner::TopLeft);
        assert!((read.lineups.clip_speed - 2.0).abs() < f32::EPSILON);
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written.get("overlay"), Some(&serde_json::json!(true)));
        assert_eq!(written.get("corner"), Some(&serde_json::json!("top-left")));
        assert_eq!(written.get("clip_speed"), Some(&serde_json::json!(2.0)));
        assert!(written.get("lineups").is_none(), "no nesting in the file");
    }

    /// The first run, with nothing written yet.
    #[test]
    fn nothing_written_yet_is_the_defaults() {
        let root = std::path::Path::new("Z:/nowhere-at-all");
        assert_eq!(load(root).quality, Quality::Auto);
    }
}
