use overseer_core::Player;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Sort {
    pub(crate) column: Option<String>,
    pub(crate) direction: Option<Direction>,
}

impl Sort {
    pub(crate) fn clicked(&mut self, column: &str) {
        if self.column.as_deref() != Some(column) {
            self.column = Some(column.to_owned());
            self.direction = Some(Direction::Down);
        } else if self.direction == Some(Direction::Down) {
            self.direction = Some(Direction::Up);
        } else {
            self.column = None;
            self.direction = None;
        }
    }

    pub(crate) fn arrow(&self, column: &str) -> Option<Direction> {
        if self.column.as_deref() == Some(column) {
            self.direction
        } else {
            None
        }
    }
}

fn key_of(column: &str, player: &Player) -> Option<f64> {
    match column {
        "rank" => player.rank_tier.map(f64::from),
        "rr" => player.rr.map(|r| r as f64),
        "peak" => player.peak_rank_tier.map(f64::from),
        "k/d" if crate::board::few_games(player) => None,
        "k/d" => player.kd,
        "games" => player.kd.map(|_| player.form.len() as f64),
        "hs" => player.hs_pct,
        "win" => player.win_rate,
        "map" => player.map_win_rate.as_ref().and_then(|m| m.win_rate),
        "met" => Some(f64::from(player.met())),
        "lvl" => player.level.map(f64::from),
        "last 5" => Some(wins(player)),
        _ => None,
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

pub(crate) fn apply(players: &mut [&Player], sort: &Sort) {
    let (Some(column), Some(direction)) = (sort.column.as_deref(), sort.direction) else {
        return;
    };
    players.sort_by(|a, b| order(key_of(column, a), key_of(column, b), direction));
}

fn order(a: Option<f64>, b: Option<f64>, direction: Direction) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let natural = match (a, b) {
        (None, None) => return Ordering::Equal,
        (None, _) => return Ordering::Greater,
        (_, None) => return Ordering::Less,
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
    };
    if direction == Direction::Down {
        natural.reverse()
    } else {
        natural
    }
}

#[cfg(test)]
mod tests {
    use overseer_core::Player;

    use super::{Direction, Sort, apply};

    fn player(name: &str, kd: Option<f64>, level: Option<u32>) -> Player {
        Player {
            name: Some(name.to_owned()),
            agent: Some("KAY/O".to_owned()),
            kd,
            level,
            ..Player::default()
        }
    }

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
}
