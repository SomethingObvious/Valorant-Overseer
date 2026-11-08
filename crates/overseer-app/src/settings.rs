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

/// Whether the board still opens with the strip for a first look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Welcome {
    /// Not dismissed yet.
    #[default]
    Due,
    /// Got It was pressed.
    Seen,
}

/// How much Overseer holds back to leave the game room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Pc {
    /// Decided from what the PC has.
    #[default]
    Auto,
    /// Clips cut at full quality.
    Fast,
    /// Clips cut at 720p, which takes about a quarter less to play.
    Slow,
}

impl Pc {
    /// Every choice, in the order the settings list them.
    pub(crate) const ALL: [Self; 3] = [Self::Auto, Self::Fast, Self::Slow];

    /// What the settings call it, and a sentence about it.
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

/// The choices for how many games each row reads, the backend's own.
pub(crate) const GAMES_READ: [u16; 3] = [5, 10, 20];
/// The choices for how many days back those games may go, 0 for any age.
pub(crate) const GAMES_WITHIN: [u16; 5] = [7, 14, 30, 90, 0];

/// How many days your party's games stay on disk, 0 for not at all.
pub(crate) const PARTY_KEPT: [u16; 4] = [0, 7, 14, 30];

/// Everything the window remembers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// How much to spend on looking good.
    pub(crate) quality: Quality,
    /// Whether this PC is slow, decided from what it has unless set.
    pub(crate) pc: Pc,
    /// Columns switched off by hand, by their heading.
    pub(crate) hidden_columns: Vec<String>,
    /// Whether the detail panel takes the right of the window.
    pub(crate) panel: bool,
    /// The detail panel's width, once it has been dragged to one. Without
    /// one it fits the window.
    pub(crate) panel_width: Option<f32>,
    /// Whether the window minimizes itself when agent select ends. VALORANT
    /// minimizes when it loses the focus, so without this, closing a
    /// browser opened over the game can bring this window up instead.
    pub(crate) minimize_in_matches: bool,
    /// Whether the overlay is on, and where.
    #[serde(flatten)]
    pub(crate) overlay: OverlaySettings,
    /// Whether the other side is drawn above your own.
    pub(crate) enemies_first: bool,
    /// How many past games the History screen shows.
    pub(crate) history_games: u32,
    /// How many games each row's K/D and reads come from, one of
    /// [`GAMES_READ`].
    pub(crate) games_read: u16,
    /// How many days back those games may go, one of [`GAMES_WITHIN`].
    pub(crate) games_within: u16,
    /// How many days your party's games stay on disk, one of [`PARTY_KEPT`].
    pub(crate) party_kept: u16,
    /// How the Lineups screen plays clips and draws its maps.
    #[serde(flatten)]
    pub(crate) lineups: LineupSettings,
    /// Whether to look for a newer release, and what was last found.
    #[serde(flatten)]
    pub(crate) updates: UpdateSettings,
    /// Whether the board still opens with the strip for a first look.
    pub(crate) welcome: Welcome,
    /// Which tags the rows show, and in what order.
    pub(crate) tags: TagSettings,
}

/// Every kind of tag the backend gives, in the order a row draws them until
/// Settings says otherwise, and the words each kind can be. A leaderboard
/// place is its own word, so it goes by "leaderboard" here.
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
        "Scout",
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

/// Which tags the rows and the panel show, and which kind comes first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TagSettings {
    /// The kinds, first to last.
    pub(crate) order: Vec<String>,
    /// Kinds switched off whole.
    pub(crate) hidden_kinds: Vec<String>,
    /// Single words switched off.
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
    /// The kinds first to last, with any this file never named in their
    /// default places after the rest.
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

    /// Moves `kind` one place earlier, or later when `later`.
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

    /// Whether `kind` is switched off whole.
    pub(crate) fn hides_kind(&self, kind: &str) -> bool {
        self.hidden_kinds
            .iter()
            .any(|k| k.eq_ignore_ascii_case(kind))
    }

    /// Whether the word `word` is switched off on its own.
    pub(crate) fn hides_word(&self, word: &str) -> bool {
        let word = if word.starts_with('#') {
            "leaderboard"
        } else {
            word
        };
        self.hidden.iter().any(|w| w.eq_ignore_ascii_case(word))
    }

    /// Switches a kind or a word on when it was off, and off when it was on.
    pub(crate) fn flip(list: &mut Vec<String>, name: &str) {
        if list.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            list.retain(|n| !n.eq_ignore_ascii_case(name));
        } else {
            list.push(name.to_owned());
        }
    }

    /// `tags` without the ones switched off, in the kinds' order, each
    /// carrying its place so the row keeps it.
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

/// The daily look for a newer release, kept in the same file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct UpdateSettings {
    /// Whether to ask GitHub, once a day, which release is newest.
    pub(crate) check_updates: bool,
    /// When it last asked, in seconds since 1970.
    pub(crate) update_checked_at: Option<i64>,
    /// The newest release it was told of.
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
    /// The key or mouse button that shows and hides it in game.
    #[serde(rename = "overlay_key")]
    pub(crate) key: crate::hotkey::Binding,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        // It only ever shows itself before the first round's fight, so it
        // can be on from the start.
        Self {
            auto: true,
            corner: Corner::default(),
            key: crate::hotkey::Binding::DEFAULT,
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
    /// What is said about lineups from before their map changed.
    #[serde(flatten)]
    pub(crate) redraws: RedrawSettings,
}

/// What is said about lineups saved before a game update changed their map,
/// kept in the same file under the same names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RedrawSettings {
    /// The map redraws already said, as the map's name and when it changed,
    /// so each is said once.
    pub(crate) redraws_seen: Vec<String>,
    /// Whether each such lineup is marked until it is confirmed.
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
            redraws: RedrawSettings::default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        // Everything on, so somebody who never opens the settings still sees
        // what the app can do.
        Self {
            quality: Quality::Auto,
            pc: Pc::Auto,
            hidden_columns: Vec::new(),
            panel: true,
            panel_width: None,
            minimize_in_matches: false,
            overlay: OverlaySettings::default(),
            // The five you cannot see in game are the five worth the top of
            // the window.
            enemies_first: true,
            // Two sessions' worth, and one page of Riot's history.
            history_games: 10,
            // Every row reads five first and works back to this.
            games_read: 20,
            // A month, so an account picked up again after a break reads as
            // it plays now.
            games_within: 30,
            // Two weeks covers a few sessions without the folder growing for
            // games nobody looks at again.
            party_kept: 14,
            lineups: LineupSettings::default(),
            updates: UpdateSettings::default(),
            welcome: Welcome::Due,
            tags: TagSettings::default(),
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

/// Files that were there but couldn't be read at all this run.
static UNREAD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Whether `file` may be written: not when it couldn't be read, since that
/// would replace what's in it with the defaults.
pub(crate) fn may_write(file: &Path) -> bool {
    !UNREAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|unread| unread == file)
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
        // Antivirus or a sync tool holding the file is not a broken file.
        // What's in it was never seen, so it is never written over.
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

/// Writes the settings, and gives up quietly if it cannot. The change has
/// already happened on screen, and a failed write is not worth interrupting.
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
        drop(std::fs::write(file, text));
    }
}

#[cfg(test)]
mod tests {
    use super::{Quality, Settings, load, save};

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

    /// A tag switched off goes, alone or with its kind, and the rest come in
    /// the kinds' order with the board's order kept inside each kind.
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
            tag("aggressive", "Scout"),
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
        settings.shift("Scout", false);
        settings.shift("Scout", false);
        settings.shift("Weapon", true);
        assert_eq!(
            settings.kinds().get(..4),
            Some(&["Economy", "Weapon", "Agent", "Scout"][..])
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
        // A file that names a kind twice, or one there isn't, still lists
        // every kind once.
        settings.order = vec!["Map".into(), "map".into(), "Nope".into()];
        let kinds = settings.kinds();
        assert_eq!(kinds.len(), super::TAG_KINDS.len());
        assert_eq!(kinds.first(), Some(&"Map"));
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
}
