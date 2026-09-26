//! Ordering and filtering the board.
//!
//! Both are things a window does better than a terminal, and both are
//! answers to questions a person asks out loud in agent select: "who is the
//! best player here", and "where is the one called Day". Neither changes what
//! the backend sent; they change the order it is read in.
//!
//! Sorting happens inside a team rather than across the board. The two sides
//! are the first thing the eye uses, and a sorted list that mixes them
//! answers a question nobody asked.

use overseer_core::Player;

/// Which way a sorted column runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// Smallest or earliest first.
    Up,
    /// Largest or latest first.
    Down,
}

impl Direction {
    /// The other one.
    pub(crate) const fn flipped(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

/// How the board is ordered, if it has been touched at all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Sort {
    /// The column heading, or nothing for the order the backend sent.
    pub(crate) column: Option<String>,
    /// Which way that column runs.
    pub(crate) direction: Option<Direction>,
}

impl Sort {
    /// What clicking a heading does.
    ///
    /// A first click sorts by the thing you clicked, the useful way round: a
    /// K/D is asked about best first, a name is asked about A first. A second
    /// click reverses it, and a third puts the board back the way the backend
    /// sent it, because "undo" should not need a different gesture.
    pub(crate) fn clicked(&mut self, column: &str) {
        if self.column.as_deref() != Some(column) {
            self.column = Some(column.to_owned());
            self.direction = Some(default_direction(column));
            return;
        }
        match self.direction {
            Some(direction) if direction == default_direction(column) => {
                self.direction = Some(direction.flipped());
            }
            _ => {
                self.column = None;
                self.direction = None;
            }
        }
    }

    /// The arrow to draw on a heading, if it is the sorted one.
    pub(crate) fn arrow(&self, column: &str) -> Option<Direction> {
        if self.column.as_deref() == Some(column) {
            self.direction
        } else {
            None
        }
    }
}

/// Which way a column is worth reading first.
fn default_direction(column: &str) -> Direction {
    match column {
        // Words read from A, and a rank reads from the top because the
        // question is who the best player in the lobby is.
        "player" | "agent" => Direction::Up,
        _ => Direction::Down,
    }
}

/// The value a column sorts on.
///
/// One scale for everything, so a column of numbers and a column of words can
/// share a comparison. Missing sorts last whichever way the column runs,
/// because a row with no data is never the answer to "who is the best".
#[derive(Debug, Clone, PartialEq)]
enum Key {
    /// A number, largest last.
    Number(f64),
    /// Words, compared without case.
    Text(String),
    /// Nothing to compare.
    Missing,
}

/// What one player sorts as, for one column.
fn key_of(column: &str, player: &Player) -> Key {
    let number = |value: Option<f64>| value.map_or(Key::Missing, Key::Number);
    match column {
        "player" => player
            .name
            .clone()
            .map_or(Key::Missing, |n| Key::Text(n.to_lowercase())),
        "agent" => player
            .agent
            .clone()
            .map_or(Key::Missing, |a| Key::Text(a.to_lowercase())),
        "rank" => player
            .rank_tier
            .map_or(Key::Missing, |t| Key::Number(f64::from(t))),
        "rr" => player.rr.map_or(Key::Missing, |r| Key::Number(r as f64)),
        "peak" => player
            .peak_rank_tier
            .map_or(Key::Missing, |t| Key::Number(f64::from(t))),
        "k/d" => number(player.kd),
        "hs" => number(player.hs_pct),
        "win" => number(player.win_rate),
        "map" => number(player.map_win_rate.as_ref().and_then(|m| m.win_rate)),
        "met" => Key::Number(f64::from(player.met())),
        "lvl" => player
            .level
            .map_or(Key::Missing, |l| Key::Number(f64::from(l))),
        "last 5" => Key::Number(wins(player)),
        _ => Key::Missing,
    }
}

/// How many of the recent results were wins, which is what "last 5" means
/// when it is asked to be a number.
fn wins(player: &Player) -> f64 {
    let won = player
        .form
        .iter()
        .filter(|r| matches!(r.chars().next(), Some('W' | 'w')))
        .count();
    won as f64
}

/// Puts a team in order. Nothing sorted leaves the backend's order alone.
pub(crate) fn apply(players: &mut [&Player], sort: &Sort) {
    let (Some(column), Some(direction)) = (sort.column.as_deref(), sort.direction) else {
        return;
    };
    players.sort_by(|left, right| {
        let (a, b) = (key_of(column, left), key_of(column, right));
        order(&a, &b, direction)
    });
}

/// Compares two keys, with missing always last.
fn order(a: &Key, b: &Key, direction: Direction) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Key::Missing, Key::Missing) => Ordering::Equal,
        (Key::Missing, _) => Ordering::Greater,
        (_, Key::Missing) => Ordering::Less,
        (Key::Number(x), Key::Number(y)) => {
            let natural = x.partial_cmp(y).unwrap_or(Ordering::Equal);
            if direction == Direction::Down {
                natural.reverse()
            } else {
                natural
            }
        }
        (Key::Text(x), Key::Text(y)) => {
            let natural = x.cmp(y);
            if direction == Direction::Down {
                natural.reverse()
            } else {
                natural
            }
        }
        // Two columns cannot hold both shapes, so this cannot happen; equal
        // is the answer that changes nothing if it ever does.
        _ => Ordering::Equal,
    }
}

/// Whether a player survives the filter.
///
/// Matches a name or an agent, without case, anywhere in the word. A filter
/// that only matched the start would be useless for a tag, and one that
/// matched the rank would answer a question the columns already answer.
pub(crate) fn matches(player: &Player, filter: &str) -> bool {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let hay = |value: Option<&str>| value.is_some_and(|v| v.to_lowercase().contains(&needle));
    hay(player.name.as_deref()) || hay(player.agent.as_deref())
}

#[cfg(test)]
mod tests {
    use overseer_core::Player;

    use super::{Direction, Sort, apply, matches};

    fn player(name: &str, kd: Option<f64>, level: Option<u32>) -> Player {
        Player {
            name: Some(name.to_owned()),
            agent: Some("KAY/O".to_owned()),
            kd,
            level,
            ..Player::default()
        }
    }

    /// Three clicks on one heading: sorted, reversed, and back to the order
    /// the backend sent. Undo should not need a different gesture.
    #[test]
    fn a_third_click_puts_it_back() {
        let mut sort = Sort::default();
        sort.clicked("k/d");
        assert_eq!(sort.arrow("k/d"), Some(Direction::Down));
        sort.clicked("k/d");
        assert_eq!(sort.arrow("k/d"), Some(Direction::Up));
        sort.clicked("k/d");
        assert_eq!(sort.arrow("k/d"), None);
        assert_eq!(sort.column, None);
    }

    /// Clicking a different heading starts that one afresh rather than
    /// carrying the last one's direction over.
    #[test]
    fn a_new_column_starts_the_useful_way_round() {
        let mut sort = Sort::default();
        sort.clicked("k/d");
        sort.clicked("k/d");
        assert_eq!(sort.arrow("k/d"), Some(Direction::Up));
        sort.clicked("player");
        assert_eq!(
            sort.arrow("player"),
            Some(Direction::Up),
            "names read from A"
        );
        assert_eq!(sort.arrow("k/d"), None);
        sort.clicked("lvl");
        assert_eq!(
            sort.arrow("lvl"),
            Some(Direction::Down),
            "numbers read best first"
        );
    }

    /// A player with no number is never the answer to "who is the best", so
    /// they sort last whichever way the column runs.
    #[test]
    fn missing_values_sort_last_both_ways() {
        let alice = player("Alice#EU", Some(1.5), Some(40));
        let nobody = player("Nobody#EU", None, None);
        let bob = player("Bob#EU", Some(0.8), Some(300));
        for direction in [Direction::Up, Direction::Down] {
            let mut list = vec![&nobody, &alice, &bob];
            let sort = Sort {
                column: Some("k/d".to_owned()),
                direction: Some(direction),
            };
            apply(&mut list, &sort);
            assert_eq!(
                list.last().and_then(|p| p.name.as_deref()),
                Some("Nobody#EU"),
                "{direction:?} put a missing value somewhere other than last"
            );
        }
    }

    #[test]
    fn sorting_runs_both_ways() {
        let alice = player("Alice#EU", Some(1.5), Some(40));
        let bob = player("Bob#EU", Some(0.8), Some(300));
        let mut list = vec![&bob, &alice];
        apply(
            &mut list,
            &Sort {
                column: Some("k/d".to_owned()),
                direction: Some(Direction::Down),
            },
        );
        assert_eq!(
            list.first().and_then(|p| p.name.as_deref()),
            Some("Alice#EU")
        );
        apply(
            &mut list,
            &Sort {
                column: Some("k/d".to_owned()),
                direction: Some(Direction::Up),
            },
        );
        assert_eq!(list.first().and_then(|p| p.name.as_deref()), Some("Bob#EU"));
    }

    /// Nothing sorted leaves the backend's order exactly as it arrived.
    #[test]
    fn no_sort_changes_nothing() {
        let alice = player("Alice#EU", Some(1.5), None);
        let bob = player("Bob#EU", Some(0.8), None);
        let mut list = vec![&bob, &alice];
        apply(&mut list, &Sort::default());
        assert_eq!(list.first().and_then(|p| p.name.as_deref()), Some("Bob#EU"));
    }

    #[test]
    fn the_filter_matches_a_name_or_an_agent_anywhere() {
        let target = player("Day#9932", Some(1.0), None);
        assert!(matches(&target, ""));
        assert!(matches(&target, "day"), "the start of a name");
        assert!(matches(&target, "99"), "part of a tag");
        assert!(matches(&target, "KAY"), "an agent");
        assert!(matches(&target, "  kay/o  "), "spaces around it");
        assert!(!matches(&target, "viper"));
    }
}
