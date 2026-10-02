//! Ordering and filtering the board. Neither changes what the backend sent,
//! only the order it is read in. Sorting stays inside each team, because the
//! two sides are the first thing the eye goes to.

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
    /// A click on a heading. The first sorts the useful way round, the second
    /// reverses, and the third goes back to the backend's order.
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
        // Names read from A. Everything else reads best first.
        "player" | "agent" => Direction::Up,
        _ => Direction::Down,
    }
}

/// The value a column sorts on. Missing sorts last whichever way the column
/// runs, since a row with no data never answers "who is best".
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
        "rank" => number(player.rank_tier.map(f64::from)),
        "rr" => number(player.rr.map(|r| r as f64)),
        "peak" => number(player.peak_rank_tier.map(f64::from)),
        "k/d" => number(player.kd),
        "games" => number(player.kd.map(|_| player.form.len() as f64)),
        "hs" => number(player.hs_pct),
        "win" => number(player.win_rate),
        "map" => number(player.map_win_rate.as_ref().and_then(|m| m.win_rate)),
        "met" => Key::Number(f64::from(player.met())),
        "lvl" => number(player.level.map(f64::from)),
        "last 5" => Key::Number(wins(player)),
        _ => Key::Missing,
    }
}

/// How many of the recent results were wins, which is "last 5" as a number.
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
    let natural = match (a, b) {
        (Key::Missing, Key::Missing) => return Ordering::Equal,
        (Key::Missing, _) => return Ordering::Greater,
        (_, Key::Missing) => return Ordering::Less,
        (Key::Number(x), Key::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Key::Text(x), Key::Text(y)) => x.cmp(y),
        // A column never mixes numbers and words, and equal changes nothing
        // if one ever does.
        _ => return Ordering::Equal,
    };
    if direction == Direction::Down {
        natural.reverse()
    } else {
        natural
    }
}

/// Whether a player's name or agent contains the filter, ignoring case.
/// Anywhere in the word, not just the start, so a tag finds its player.
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
    /// the backend sent.
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

    /// A different heading starts afresh instead of taking the last one's
    /// direction.
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

    /// A player with no number sorts last whichever way the column runs.
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
