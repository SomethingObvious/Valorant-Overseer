use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::overlay::Corner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Quality {
    #[default]
    Auto,
    Efficient,
    Rich,
}

impl Quality {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Efficient => "Efficient",
            Self::Rich => "Full",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Welcome {
    #[default]
    Due,
    Seen,
}

/// Whether your Discord profile shows what you are playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Discord {
    #[default]
    Off,
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Pc {
    #[default]
    Auto,
    Fast,
    /// Clips cut at 720p, which takes about a quarter less to play.
    Slow,
}

impl Pc {
    pub(crate) const ALL: [Self; 3] = [Self::Auto, Self::Fast, Self::Slow];

    pub(crate) const fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::Auto => (
                "Auto",
                "Decide from this PC's memory, processor and graphics",
            ),
            Self::Fast => ("Fast PC", "Clips cut at full quality"),
            Self::Slow => (
                "Slow PC",
                "Clips cut at 720p, which takes about a quarter less to play",
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MapTurn {
    #[default]
    Attack,
    Defend,
    /// The way Riot's own minimap is drawn.
    Drawn,
}

impl MapTurn {
    pub(crate) const ALL: [Self; 3] = [Self::Attack, Self::Defend, Self::Drawn];

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

pub(crate) const GAMES_READ: [u16; 3] = [5, 10, 20];
/// The choices for how many days back those games may go, 0 for any age.
pub(crate) const GAMES_WITHIN: [u16; 5] = [7, 14, 30, 90, 0];

/// How many days your party's games stay on disk, 0 for not at all.
pub(crate) const PARTY_KEPT: [u16; 4] = [0, 7, 14, 30];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) quality: Quality,
    pub(crate) pc: Pc,
    pub(crate) hidden_columns: Vec<String>,
    pub(crate) panel: bool,
    pub(crate) panel_width: Option<f32>,
    /// Whether the window minimizes itself when agent select ends. VALORANT
    /// minimizes when it loses the focus, so without this, closing a
    /// browser opened over the game can bring this window up instead.
    pub(crate) minimize_in_matches: bool,
    #[serde(flatten)]
    pub(crate) overlay: OverlaySettings,
    pub(crate) enemies_first: bool,
    pub(crate) games_read: u16,
    pub(crate) games_within: u16,
    pub(crate) party_kept: u16,
    /// Whether your Discord profile shows what you are playing.
    pub(crate) discord: Discord,
    #[serde(flatten)]
    pub(crate) lineups: LineupSettings,
    #[serde(flatten)]
    pub(crate) updates: UpdateSettings,
    pub(crate) welcome: Welcome,
    pub(crate) tags: TagSettings,
}

pub(crate) const TAG_KINDS: [(&str, &[&str]); 9] = [
    (
        "Weapon",
        &[
            "op", "outlaw", "marshal", "judge", "bucky", "odin", "ares", "spectre", "sheriff",
        ],
    ),
    ("Economy", &["forces", "saves"]),
    ("Agent", &["one-trick", "off main"]),
    ("Rank", &["leaderboard", "deranked"]),
    ("Duels", &["entry", "clutch", "aced", "planter"]),
    (
        "Tendency",
        &[
            "aggressive",
            "passive",
            "fights first",
            "waits",
            "first bloods",
            "few first bloods",
            "dies first",
            "rarely dies first",
            "fast attack",
            "slow attack",
            "fast defense",
            "slow defense",
            "dies early",
            "dies late",
            "long range",
            "close range",
            "plays apart",
            "plays close",
            "plants",
            "rarely plants",
        ],
    ),
    ("Form", &["hot", "cold"]),
    ("History", &["rival", "teammate"]),
    ("Map", &["strong map"]),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TagSettings {
    pub(crate) order: Vec<String>,
    pub(crate) hidden_kinds: Vec<String>,
    pub(crate) hidden: Vec<String>,
}

impl Default for TagSettings {
    fn default() -> Self {
        Self {
            order: TAG_KINDS
                .iter()
                .map(|(kind, _)| (*kind).to_owned())
                .collect(),
            hidden_kinds: Vec::new(),
            hidden: Vec::new(),
        }
    }
}

impl TagSettings {
    pub(crate) fn kinds(&self) -> Vec<&'static str> {
        let mut kinds: Vec<&'static str> = self
            .order
            .iter()
            .filter_map(|o| {
                TAG_KINDS
                    .iter()
                    .map(|(kind, _)| *kind)
                    .find(|kind| kind.eq_ignore_ascii_case(o))
            })
            .fold(Vec::new(), |mut out, kind| {
                if !out.contains(&kind) {
                    out.push(kind);
                }
                out
            });
        for (kind, _) in TAG_KINDS {
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        kinds
    }

    pub(crate) fn shift(&mut self, kind: &str, later: bool) {
        let mut kinds = self.kinds();
        if let Some(at) = kinds.iter().position(|k| *k == kind) {
            let to = if later { at + 1 } else { at.wrapping_sub(1) };
            if to < kinds.len() {
                kinds.swap(at, to);
            }
        }
        self.order = kinds.into_iter().map(str::to_owned).collect();
    }

    pub(crate) fn hides_kind(&self, kind: &str) -> bool {
        self.hidden_kinds
            .iter()
            .any(|k| k.eq_ignore_ascii_case(kind))
    }

    pub(crate) fn hides_word(&self, word: &str) -> bool {
        let word = if word.starts_with('#') {
            "leaderboard"
        } else {
            word
        };
        self.hidden.iter().any(|w| w.eq_ignore_ascii_case(word))
    }

    pub(crate) fn flip(list: &mut Vec<String>, name: &str) {
        if list.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            list.retain(|n| !n.eq_ignore_ascii_case(name));
        } else {
            list.push(name.to_owned());
        }
    }

    pub(crate) fn applied(&self, tags: &[overseer_core::AutoTag]) -> Vec<overseer_core::AutoTag> {
        let kinds = self.kinds();
        let place = |kind: Option<&str>| {
            let at = kind
                .and_then(|kind| kinds.iter().position(|k| k.eq_ignore_ascii_case(kind)))
                .unwrap_or(kinds.len());
            u8::try_from(at).unwrap_or(u8::MAX)
        };
        let mut shown: Vec<overseer_core::AutoTag> = tags
            .iter()
            .filter(|t| {
                !t.kind.as_deref().is_some_and(|k| self.hides_kind(k))
                    && !t.tag.as_deref().is_some_and(|w| self.hides_word(w))
            })
            .map(|t| overseer_core::AutoTag {
                order: Some(place(t.kind.as_deref())),
                ..t.clone()
            })
            .collect();
        shown.sort_by_key(|t| t.order);
        shown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct UpdateSettings {
    /// Whether to ask GitHub, once a day, which release is newest.
    pub(crate) check_updates: bool,
    /// When it last asked, in seconds since 1970.
    pub(crate) update_checked_at: Option<i64>,
    pub(crate) newest_release: Option<String>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_updates: true,
            update_checked_at: None,
            newest_release: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct OverlaySettings {
    #[serde(rename = "overlay")]
    pub(crate) auto: bool,
    pub(crate) corner: Corner,
    #[serde(rename = "overlay_key")]
    pub(crate) key: crate::hotkey::Binding,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            auto: true,
            corner: Corner::default(),
            key: crate::hotkey::Binding::DEFAULT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LineupSettings {
    pub(crate) clip_speed: f32,
    /// How loud it plays, from 0 to 100.
    pub(crate) clip_volume: f32,
    pub(crate) clip_autoplay: bool,
    pub(crate) clip_loop: bool,
    pub(crate) clip_size: f32,
    pub(crate) map_turn: MapTurn,
    pub(crate) lineup_names: bool,
    pub(crate) area_colour: Option<String>,
    pub(crate) default_agent: String,
    #[serde(flatten)]
    pub(crate) redraws: RedrawSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RedrawSettings {
    pub(crate) redraws_seen: Vec<String>,
    pub(crate) mark_redrawn: bool,
}

impl Default for RedrawSettings {
    fn default() -> Self {
        Self {
            redraws_seen: Vec::new(),
            mark_redrawn: true,
        }
    }
}

impl Default for LineupSettings {
    fn default() -> Self {
        Self {
            clip_speed: 1.25,
            clip_volume: 0.0,
            clip_autoplay: true,
            clip_loop: true,
            clip_size: 0.63,
            map_turn: MapTurn::Attack,
            lineup_names: false,
            area_colour: None,
            default_agent: "Brimstone".to_owned(),
            redraws: RedrawSettings::default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quality: Quality::Auto,
            pc: Pc::Auto,
            hidden_columns: Vec::new(),
            panel: true,
            panel_width: None,
            minimize_in_matches: false,
            overlay: OverlaySettings::default(),
            enemies_first: true,
            // Two sessions' worth, and one page of Riot's history.
            games_read: 20,
            games_within: 30,
            party_kept: 14,
            // Off, since Discord shows it to everyone who can see your
            // profile, and that is for you to decide.
            discord: Discord::Off,
            lineups: LineupSettings::default(),
            updates: UpdateSettings::default(),
            welcome: Welcome::Due,
            tags: TagSettings::default(),
        }
    }
}

impl Settings {
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

fn path(root: &Path) -> PathBuf {
    root.join(".overseer").join("app.json")
}

pub(crate) fn load(root: &Path) -> Settings {
    read_or_set_aside(&path(root))
}

static UNREAD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn may_write(file: &Path) -> bool {
    !UNREAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|unread| unread == file)
}

pub(crate) fn read_or_set_aside<T: DeserializeOwned + Default>(file: &Path) -> T {
    match std::fs::read(file) {
        Ok(raw) => {
            if let Ok(value) = serde_json::from_slice(&raw) {
                return value;
            }
        }
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => return T::default(),
        Err(why) => {
            println!(
                "{} couldn't be read, so it won't be saved over: {why}",
                file.display()
            );
            UNREAD
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(file.to_path_buf());
            return T::default();
        }
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

pub(crate) fn save(root: &Path, settings: &Settings) {
    let file = path(root);
    if !may_write(&file) {
        return;
    }
    if let Some(dir) = file.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _written = write_whole(&file, &text);
    }
}

/// Writes `text` to `file` through a file beside it, so a full disk or a
/// crash part way leaves the old file and not half of the new one. Synced
/// before the rename, since NTFS keeps the rename through a power cut but
/// not the data, and the file would come back empty. Says whether it worked.
pub(crate) fn write_whole(file: &Path, text: &str) -> bool {
    let mut fresh = file.as_os_str().to_owned();
    fresh.push(".new");
    let written = std::fs::File::create(&fresh).and_then(|mut out| {
        std::io::Write::write_all(&mut out, text.as_bytes())?;
        out.sync_all()
    });
    written.is_ok() && std::fs::rename(&fresh, file).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{Quality, Settings, load, save, write_whole};

    #[test]
    fn a_failed_write_keeps_the_old_file() {
        let dir = std::env::temp_dir().join(format!("overseer-whole-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        assert!(write_whole(&file, "old"));
        std::fs::create_dir(dir.join("settings.json.new")).unwrap();
        assert!(!write_whole(&file, "new"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
        std::fs::remove_dir_all(&dir).unwrap();
    }

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

    #[test]
    fn tags_follow_the_settings() {
        let tag = |word: &str, kind: &str| overseer_core::AutoTag {
            tag: Some(word.to_owned()),
            kind: Some(kind.to_owned()),
            ..overseer_core::AutoTag::default()
        };
        let board = [
            tag("hot", "Form"),
            tag("op", "Weapon"),
            tag("aggressive", "Tendency"),
            tag("#12", "Rank"),
            tag("forces", "Economy"),
            tag("marshal", "Weapon"),
        ];
        let words = |tags: &[overseer_core::AutoTag]| -> Vec<String> {
            tags.iter().filter_map(|t| t.tag.clone()).collect()
        };
        let mut settings = super::TagSettings::default();
        assert_eq!(
            words(&settings.applied(&board)),
            ["op", "marshal", "forces", "#12", "aggressive", "hot"]
        );
        settings.shift("Tendency", false);
        settings.shift("Tendency", false);
        settings.shift("Weapon", true);
        assert_eq!(
            settings.kinds().get(..4),
            Some(&["Economy", "Weapon", "Agent", "Tendency"][..])
        );
        super::TagSettings::flip(&mut settings.hidden_kinds, "economy");
        super::TagSettings::flip(&mut settings.hidden, "leaderboard");
        super::TagSettings::flip(&mut settings.hidden, "op");
        assert_eq!(
            words(&settings.applied(&board)),
            ["marshal", "aggressive", "hot"]
        );
        super::TagSettings::flip(&mut settings.hidden, "op");
        assert_eq!(
            words(&settings.applied(&board)),
            ["op", "marshal", "aggressive", "hot"]
        );
        settings.order = vec!["Map".into(), "map".into(), "Nope".into()];
        let kinds = settings.kinds();
        assert_eq!(kinds.len(), super::TAG_KINDS.len());
        assert_eq!(kinds.first(), Some(&"Map"));
    }

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
}
