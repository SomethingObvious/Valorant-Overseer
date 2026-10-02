//! Notes on players, the things no API knows, like "instalocks Reyna".
//!
//! They are kept by account id because names change and repeat, and in plain
//! JSON in the install's own directory so they can be read and deleted by
//! hand.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What you wrote about one account.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Note {
    /// The note itself, in your words.
    pub(crate) text: String,
    /// Short labels, for the things you write about more than one person.
    pub(crate) tags: Vec<String>,
    /// Their name when you wrote it, so the file makes sense to a person.
    /// Lookups never use it.
    pub(crate) name: String,
}

impl Note {
    /// Whether there is anything here worth keeping.
    pub(crate) fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.tags.is_empty()
    }
}

/// Every note, by account id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Notes {
    entries: BTreeMap<String, Note>,
}

impl Notes {
    /// The note for an account, or an empty one.
    pub(crate) fn get(&self, puuid: &str) -> Note {
        self.entries.get(puuid).cloned().unwrap_or_default()
    }

    /// What you wrote about this account, if anything.
    pub(crate) fn text(&self, puuid: &str) -> Option<&str> {
        self.entries
            .get(puuid)
            .map(|n| n.text.trim())
            .filter(|t| !t.is_empty())
    }

    /// The first tag given to this account, if any.
    pub(crate) fn tag(&self, puuid: &str) -> Option<&str> {
        self.entries
            .get(puuid)
            .and_then(|n| n.tags.first())
            .map(String::as_str)
    }

    /// The tags you have used most, most first, ties alphabetical.
    pub(crate) fn common_tags(&self, limit: usize) -> Vec<String> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for tag in self.entries.values().flat_map(|n| n.tags.iter()) {
            *counts.entry(tag.as_str()).or_default() += 1;
        }
        // The map is alphabetical and the sort is stable, which breaks ties.
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        counts
            .into_iter()
            .take(limit)
            .map(|(t, _)| t.to_owned())
            .collect()
    }

    /// Whether anything is written about them.
    pub(crate) fn has(&self, puuid: &str) -> bool {
        self.entries.get(puuid).is_some_and(|n| !n.is_empty())
    }

    /// Writes a note, or removes it once it is empty so the file keeps no
    /// blanks.
    pub(crate) fn set(&mut self, puuid: &str, note: Note) {
        if note.is_empty() {
            self.entries.remove(puuid);
        } else {
            self.entries.insert(puuid.to_owned(), note);
        }
    }

    /// How many accounts have a note.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// `.overseer/notes.json`.
fn path(root: &Path) -> PathBuf {
    root.join(".overseer").join("notes.json")
}

/// Reads the notes, or none when the file is missing or broken.
pub(crate) fn load(root: &Path) -> Notes {
    crate::settings::read_or_set_aside(&path(root))
}

/// Writes the notes, and says whether that worked.
pub(crate) fn save(root: &Path, notes: &Notes) -> bool {
    let file = path(root);
    if let Some(dir) = file.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return false;
    }
    serde_json::to_string_pretty(notes).is_ok_and(|text| std::fs::write(&file, text).is_ok())
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

/// The tags, as they are typed back into the box.
pub(crate) fn tags_line(tags: &[String]) -> String {
    tags.join(", ")
}

#[cfg(test)]
mod tests {
    use super::{Note, Notes, load, parse_tags, save, tags_line};

    #[test]
    fn tags_are_tidied_deduplicated_and_capped() {
        assert_eq!(
            parse_tags("Instalock, instalock ,  toxic"),
            vec!["instalock", "toxic"]
        );
        assert_eq!(parse_tags("   "), Vec::<String>::new());
        assert_eq!(parse_tags(",,,"), Vec::<String>::new());
        assert_eq!(parse_tags("a,b,c,d,e,f,g,h,i,j").len(), 8);
        assert_eq!(tags_line(&parse_tags("one,two")), "one, two");
    }

    /// The tags offered first are the ones used most, ties alphabetical.
    #[test]
    fn the_most_used_tags_come_first() {
        let mut notes = Notes::default();
        for (id, tags) in [
            ("a", "toxic, op"),
            ("b", "toxic"),
            ("c", "flanks, op, toxic"),
        ] {
            notes.set(
                id,
                Note {
                    text: String::new(),
                    tags: parse_tags(tags),
                    name: String::new(),
                },
            );
        }
        assert_eq!(notes.common_tags(2), vec!["toxic", "op"]);
        assert_eq!(notes.common_tags(9), vec!["toxic", "op", "flanks"]);
    }

    /// Emptying a note removes it, so the file stays readable by a person.
    #[test]
    fn an_emptied_note_is_deleted() {
        let mut notes = Notes::default();
        notes.set(
            "p1",
            Note {
                text: "instalocks".to_owned(),
                ..Note::default()
            },
        );
        assert!(notes.has("p1"));
        assert_eq!(notes.len(), 1);

        notes.set(
            "p1",
            Note {
                text: "   ".to_owned(),
                ..Note::default()
            },
        );
        assert!(!notes.has("p1"));
        assert_eq!(notes.len(), 0);
    }

    /// A note with only tags is still a note.
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

    /// Notes survive the disk, and a broken file reads as none but is kept
    /// aside, so the save that follows cannot write over it.
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
        assert!(save(&root, &notes));
        let read = load(&root);
        assert_eq!(read.get("p1").text, "instalocks Reyna");
        assert_eq!(read.get("p1").tags, vec!["instalock"]);
        assert!(!read.has("nobody"));

        std::fs::write(dir.join("notes.json"), "{ not json").unwrap();
        let empty = load(&root);
        assert_eq!(empty.len(), 0);
        assert!(save(&root, &empty));
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
