//! What you know about somebody that Riot does not.
//!
//! "Instalocks Reyna", "duos with the Jett", "toxic when losing". None of
//! that is in any API, all of it decides how a match goes, and a scoreboard
//! that cannot hold it makes you keep it in your head.
//!
//! Kept by account id rather than by name, because names change and repeat
//! and the whole point of a note is that it survives both. Kept in this
//! install's own directory in plain JSON, because a note about a person is
//! the most private thing this app touches and it should be somewhere you
//! can read, edit and delete without asking anybody.

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
    /// The name they had when you wrote it, so a file read by a human is
    /// readable by one. Never used to find anybody.
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

    /// Whether this account has anything written about it.
    pub(crate) fn has(&self, puuid: &str) -> bool {
        self.entries.get(puuid).is_some_and(|n| !n.is_empty())
    }

    /// Writes a note, or removes it when there is nothing left in it.
    ///
    /// Emptying a note deletes it rather than storing a blank, because the
    /// file is meant to be readable and a hundred empty entries is not.
    pub(crate) fn set(&mut self, puuid: &str, note: Note) {
        if note.is_empty() {
            self.entries.remove(puuid);
        } else {
            self.entries.insert(puuid.to_owned(), note);
        }
    }

    /// How many accounts have something written about them. Only the tests
    /// ask, and they ask because an empty map is a stronger claim than one
    /// absent key.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Where the notes live.
fn path(root: &Path) -> PathBuf {
    root.join(".overseer").join("notes.json")
}

/// Reads the notes, or an empty set.
///
/// A missing file is the ordinary case. A broken one is somebody having
/// edited it, and losing the app over that would be worse than starting
/// empty, so the file is left alone and the window says nothing.
pub(crate) fn load(root: &Path) -> Notes {
    std::fs::read_to_string(path(root))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Writes the notes. Returns whether it worked, because losing one silently
/// is the one failure here that would actually annoy somebody.
pub(crate) fn save(root: &Path, notes: &Notes) -> bool {
    let file = path(root);
    if let Some(dir) = file.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return false;
    }
    serde_json::to_string_pretty(notes).is_ok_and(|text| std::fs::write(&file, text).is_ok())
}

/// Splits a line of comma separated tags into tidy ones.
///
/// Lowercased and trimmed so "Instalock" and "instalock " are one tag rather
/// than three, deduplicated, and capped: a row of forty chips is not a tag
/// list, it is a note that wanted to be typed in the box above.
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

    /// Round trip through the disk, and a broken file starts empty rather
    /// than taking the window down with it.
    #[test]
    fn notes_survive_the_disk_and_a_broken_file_does_not_bite() {
        let root = std::env::temp_dir().join("overseer-notes-test");
        let dir = root.join(".overseer");
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
        assert_eq!(load(&root).len(), 0);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
