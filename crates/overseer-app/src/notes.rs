//! Notes on players, the things no API knows, like "instalocks Reyna", and
//! on your own games.
//!
//! A player's are kept by account id because names change and repeat, a
//! game's by its match id, and both in plain JSON in the install's own
//! directory so they can be read and deleted by hand.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Note {
    pub(crate) text: String,
    pub(crate) tags: Vec<String>,
    pub(crate) name: String,
}

impl Note {
    pub(crate) fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.tags.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Notes {
    entries: BTreeMap<String, Note>,
}

impl Notes {
    pub(crate) fn get(&self, puuid: &str) -> Note {
        self.entries.get(puuid).cloned().unwrap_or_default()
    }

    pub(crate) fn text(&self, puuid: &str) -> Option<&str> {
        self.entries
            .get(puuid)
            .map(|n| n.text.trim())
            .filter(|t| !t.is_empty())
    }

    pub(crate) fn tag(&self, puuid: &str) -> Option<&str> {
        self.entries
            .get(puuid)
            .and_then(|n| n.tags.first())
            .map(String::as_str)
    }

    pub(crate) fn common_tags(&self, limit: usize) -> Vec<String> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for tag in self.entries.values().flat_map(|n| n.tags.iter()) {
            *counts.entry(tag.as_str()).or_default() += 1;
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        counts
            .into_iter()
            .take(limit)
            .map(|(t, _)| t.to_owned())
            .collect()
    }

    pub(crate) fn has(&self, puuid: &str) -> bool {
        self.entries.get(puuid).is_some_and(|n| !n.is_empty())
    }

    pub(crate) fn set(&mut self, puuid: &str, note: Note) {
        if note.is_empty() {
            self.entries.remove(puuid);
        } else {
            self.entries.insert(puuid.to_owned(), note);
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum About {
    Players,
    Games,
}

fn path(root: &Path, about: About) -> PathBuf {
    root.join(".overseer").join(match about {
        About::Players => "notes.json",
        About::Games => "game-notes.json",
    })
}

pub(crate) fn load(root: &Path, about: About) -> Notes {
    crate::settings::read_or_set_aside(&path(root, about))
}

pub(crate) fn save(root: &Path, about: About, notes: &Notes) -> bool {
    let file = path(root, about);
    if !crate::settings::may_write(&file) {
        return false;
    }
    if let Some(dir) = file.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return false;
    }
    let Ok(text) = serde_json::to_string_pretty(notes) else {
        return false;
    };
    if std::fs::read_to_string(&file).is_ok_and(|old| old == text) {
        return true;
    }
    crate::settings::write_whole(&file, &text)
}

/// Splits a line of comma separated tags, lowercased and trimmed so
/// "Instalock" and "instalock " are one tag, and keeps the first 8.
pub(crate) fn parse_tags(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in line.split(',') {
        let tag = raw.trim().to_lowercase();
        if tag.is_empty() || out.iter().any(|t| t == &tag) {
            continue;
        }
        out.push(tag);
        if out.len() == 8 {
            break;
        }
    }
    out
}

pub(crate) fn tags_line(tags: &[String]) -> String {
    tags.iter()
        .map(|tag| shown_tag(tag))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn shown_tag(tag: &str) -> String {
    tag.split(' ')
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map_or_else(String::new, |first| {
                let upper: String = first.to_uppercase().collect();
                if upper.to_lowercase() == first.to_string() {
                    upper + letters.as_str()
                } else {
                    word.to_owned()
                }
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{About, Note, Notes, load, parse_tags, save, tags_line};

    #[test]
    fn tags_show_capitalized() {
        let kept = vec!["odin".to_owned(), "one tap".to_owned()];
        assert_eq!(tags_line(&kept), "Odin, One Tap");
        assert_eq!(parse_tags(&tags_line(&kept)), kept);
        let odd = vec!["\u{131}l\u{131}k".to_owned(), "\u{df}ig".to_owned()];
        assert_eq!(parse_tags(&tags_line(&odd)), odd);
    }

    #[test]
    fn tags_alone_are_worth_keeping() {
        let mut notes = Notes::default();
        notes.set(
            "p1",
            Note {
                tags: vec!["duo".to_owned()],
                ..Note::default()
            },
        );
        assert!(notes.has("p1"));
    }

    #[test]
    fn notes_survive_the_disk_and_a_broken_file_does_not_bite() {
        let root = std::env::temp_dir().join("overseer-notes-test");
        let dir = root.join(".overseer");
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&dir).unwrap();

        let mut notes = Notes::default();
        notes.set(
            "p1",
            Note {
                text: "instalocks Reyna".to_owned(),
                tags: vec!["instalock".to_owned()],
                name: "Day#9932".to_owned(),
            },
        );
        assert!(save(&root, About::Players, &notes));
        assert!(save(&root, About::Games, &Notes::default()));
        assert!(!load(&root, About::Games).has("p1"));
        let read = load(&root, About::Players);
        assert_eq!(read.get("p1").text, "instalocks Reyna");
        assert_eq!(read.get("p1").tags, vec!["instalock"]);
        assert!(!read.has("nobody"));

        std::fs::write(dir.join("notes.json"), "{ not json").unwrap();
        let empty = load(&root, About::Players);
        assert_eq!(empty.len(), 0);
        assert!(save(&root, About::Players, &empty));
        let kept: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|file| {
                file.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("notes.json.unreadable-"))
            })
            .map(|file| std::fs::read_to_string(file).unwrap())
            .collect();
        assert_eq!(kept, vec!["{ not json"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
