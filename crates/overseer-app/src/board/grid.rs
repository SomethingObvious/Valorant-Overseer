use overseer_ui::space;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Priority {
    Core,
    High,
    Mid,
    Low,
}

#[derive(Debug)]
pub(crate) struct Column {
    pub(crate) head: &'static str,
    pub(crate) label: &'static str,
    pub(crate) width: f32,
    pub(crate) priority: Priority,
    pub(crate) numeric: bool,
    pub(crate) about: &'static str,
}

pub(crate) const COLUMNS: [Column; 11] = [
    Column {
        head: "rank",
        label: "Rank",
        width: 164.0,
        priority: Priority::Core,
        numeric: false,
        about: "Current competitive rank",
    },
    Column {
        head: "peak",
        label: "Peak",
        width: PEAK,
        priority: Priority::Low,
        numeric: false,
        about: "Highest rank, with the act",
    },
    Column {
        head: "k/d",
        label: "K/D",
        width: 72.0,
        priority: Priority::Core,
        numeric: true,
        about: "Kills per death, recent games",
    },
    Column {
        head: "games",
        label: "Games",
        width: 48.0,
        priority: Priority::Low,
        numeric: true,
        about: "Games the K/D is based on",
    },
    Column {
        head: "hs",
        label: "HS",
        width: 52.0,
        priority: Priority::Low,
        numeric: true,
        about: "Headshot %, recent games",
    },
    Column {
        head: "win",
        label: "Win",
        width: 56.0,
        priority: Priority::Mid,
        numeric: true,
        about: "Win rate this act",
    },
    Column {
        head: "lvl",
        label: "Lvl",
        width: 48.0,
        priority: Priority::High,
        numeric: true,
        about: "Account level",
    },
    Column {
        head: "met",
        label: "Met",
        width: 44.0,
        priority: Priority::Mid,
        numeric: true,
        about: "Games with or against you",
    },
    Column {
        head: "map",
        label: "Map",
        width: 60.0,
        priority: Priority::Low,
        numeric: true,
        about: "Win rate on this map",
    },
    Column {
        head: "rr",
        label: "RR",
        width: 60.0,
        priority: Priority::Low,
        numeric: true,
        about: "Rank rating in their tier",
    },
    Column {
        head: "last 5",
        label: "Last 5 Games",
        width: 60.0,
        priority: Priority::High,
        numeric: false,
        about: "Their last five results",
    },
];

const TAIL: f32 = space::XL;

pub(crate) fn full() -> f32 {
    COLUMNS.iter().map(|c| c.width + GAP).sum::<f32>() + TAIL
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Placed {
    pub(crate) column: &'static Column,
    pub(crate) left: f32,
    pub(crate) right: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct Grid {
    pub(crate) placed: Vec<Placed>,
    pub(crate) peak_under_rank: bool,
    pub(crate) games_shown: bool,
    pub(crate) win_under_kd: bool,
    pub(crate) dropped: Vec<&'static str>,
}

impl Grid {
    pub(crate) fn new(width: f32, identity: f32, hidden: &[String]) -> Self {
        let room = width - identity - TAIL;
        let wanted: f32 = COLUMNS
            .iter()
            .filter(|c| c.priority <= Priority::High && !hidden.iter().any(|h| h == c.head))
            .map(|c| c.width + GAP)
            .sum();
        let tight = room < wanted;
        let mut left = identity;
        let placed = columns_for(room, hidden, tight)
            .into_iter()
            .map(|column| {
                let width = drawn_width(column, tight);
                let at = Placed {
                    column,
                    left,
                    right: left + width,
                };
                left += width + GAP;
                at
            })
            .collect::<Vec<Placed>>();
        let peak_under_rank =
            !hidden.iter().any(|h| h == "peak") && !placed.iter().any(|p| p.column.head == "peak");
        let games_shown = placed.iter().any(|p| p.column.head == "games");
        let win_under_kd = !hidden.iter().any(|h| h == "win")
            && !placed.iter().any(|p| p.column.head == "win")
            && placed.iter().any(|p| p.column.head == "k/d");
        let dropped = COLUMNS
            .iter()
            .filter(|c| !hidden.iter().any(|h| h == c.head))
            .filter(|c| !placed.iter().any(|p| p.column.head == c.head))
            .filter(|c| !(c.head == "peak" && peak_under_rank))
            .filter(|c| !(c.head == "win" && win_under_kd))
            .map(|c| c.label)
            .collect();
        Self {
            placed,
            peak_under_rank,
            games_shown,
            win_under_kd,
            dropped,
        }
    }

    pub(crate) fn span(&self) -> Option<(f32, f32)> {
        Some((self.placed.first()?.left, self.placed.last()?.right))
    }
}

const GAP: f32 = space::LG;

const RANK_EMBLEM: f32 = 44.0;

const PEAK: f32 = 164.0;

fn drawn_width(column: &Column, tight: bool) -> f32 {
    if tight && column.head == "rank" {
        RANK_EMBLEM
    } else {
        column.width
    }
}

fn width_of(keep: &[&Column], tight: bool) -> f32 {
    keep.iter().map(|c| drawn_width(c, tight) + GAP).sum()
}

fn columns_for(room: f32, hidden: &[String], tight: bool) -> Vec<&'static Column> {
    let shown = fill(room, &[], &[], tight);
    let kept: Vec<&Column> = shown
        .into_iter()
        .filter(|c| !hidden.iter().any(|h| h == c.head))
        .collect();
    fill(room, &kept, hidden, tight)
}

fn fill(
    room: f32,
    start: &[&'static Column],
    hidden: &[String],
    tight: bool,
) -> Vec<&'static Column> {
    let visible = |c: &&Column| !hidden.iter().any(|h| h == c.head);
    let order = |c: &&Column| COLUMNS.iter().position(|o| o.head == c.head).unwrap_or(0);
    let mut keep: Vec<&Column> = start.to_vec();
    for column in COLUMNS.iter().filter(|c| c.priority == Priority::Core) {
        if visible(&column) && !keep.iter().any(|c| c.head == column.head) {
            keep.push(column);
        }
    }
    for priority in [Priority::High, Priority::Mid, Priority::Low] {
        for column in COLUMNS
            .iter()
            .filter(|c| c.priority == priority)
            .filter(visible)
        {
            if keep.iter().any(|c| c.head == column.head) {
                continue;
            }
            let mut candidate = keep.clone();
            candidate.push(column);
            if width_of(&candidate, tight) <= room {
                keep = candidate;
            }
        }
    }
    keep.sort_by_key(order);
    keep
}

#[cfg(test)]
mod tests {
    use super::{COLUMNS, Grid, Priority, columns_for};

    fn heads(room: f32, hidden: &[&str]) -> Vec<&'static str> {
        let hidden: Vec<String> = hidden.iter().map(|h| (*h).to_owned()).collect();
        columns_for(room, &hidden, false)
            .iter()
            .map(|c| c.head)
            .collect()
    }

    #[test]
    fn hiding_one_column_never_hides_another() {
        for room in (200..900).step_by(20) {
            let room = room as f32;
            let before = heads(room, &[]);
            for column in &COLUMNS {
                if column.priority == Priority::Core {
                    continue;
                }
                let after = heads(room, &[column.head]);
                for kept in &before {
                    if *kept != column.head {
                        assert!(
                            after.contains(kept),
                            "at {room} hiding {} took {kept} away",
                            column.head
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn places_run_left_to_right_inside_the_row() {
        for width in (600..1400).step_by(50) {
            let width = width as f32;
            let grid = Grid::new(width, 300.0, &[]);
            let mut edge = 300.0;
            for placed in &grid.placed {
                assert!(placed.left >= edge, "{} overlaps", placed.column.head);
                edge = placed.right;
            }
            assert!(edge <= width, "the last column runs off at {width}");
        }
    }
}
