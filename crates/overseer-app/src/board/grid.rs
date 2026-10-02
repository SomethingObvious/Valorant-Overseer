//! Which statistics a row shows at this width, and where each one sits.
//! Every column's width and priority are in one table, and the heads and the
//! rows read their x from one [`Grid`] built once a frame, so a head cannot
//! drift off its own numbers. The face and the name are not a column, and
//! they always get their room first.

use overseer_ui::space;

/// How much a column matters when the window is too narrow for all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Priority {
    /// Never dropped. Without these there is no board.
    Core,
    /// The rest of the first glance.
    High,
    /// Worth a look once those are in.
    Mid,
    /// Interesting, and first to go.
    Low,
}

/// One statistic a row can show.
#[derive(Debug)]
pub(crate) struct Column {
    /// The key a person switches it off by, and the sort key.
    pub(crate) head: &'static str,
    /// What its head says. `head` is what settings save to disk, so it stays
    /// short and fixed, and this can say what the column is.
    pub(crate) label: &'static str,
    /// Drawn width in points at the window's size.
    pub(crate) width: f32,
    /// When it is dropped.
    pub(crate) priority: Priority,
    /// Whether it is a number, which sits right against its edge so the
    /// digits line up down the column.
    pub(crate) numeric: bool,
    /// What the column answers, for the settings screen.
    pub(crate) about: &'static str,
}

/// Every statistic, in reading order: where they are, how they play, how
/// new the account is, what history you have with them, how it is going.
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
        priority: Priority::Mid,
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

/// The room at the right of every row for the note mark, so the last
/// column never runs into it.
const TAIL: f32 = space::XL;

/// What every column needs, the gaps and the tail included.
pub(crate) fn full() -> f32 {
    COLUMNS.iter().map(|c| c.width + GAP).sum::<f32>() + TAIL
}

/// Where one column sits in a row, in points from the row's left edge.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placed {
    /// Which column.
    pub(crate) column: &'static Column,
    /// Its left edge.
    pub(crate) left: f32,
    /// Its right edge.
    pub(crate) right: f32,
}

/// The columns a board shows at one width, with their places.
#[derive(Debug, Clone)]
pub(crate) struct Grid {
    /// Every column shown, left to right.
    pub(crate) placed: Vec<Placed>,
    /// Whether the rank says the peak under it: when the peak is wanted but
    /// has no column of its own at this width.
    pub(crate) peak_under_rank: bool,
}

impl Grid {
    /// The grid for a row this wide, with the identity this wide and these
    /// columns switched off.
    pub(crate) fn new(width: f32, identity: f32, hidden: &[String]) -> Self {
        let room = width - identity - TAIL;
        // When the rank's word would cost a column that matters more, the
        // rank gives up its word and keeps its emblem. The emblem is the
        // rank to anybody who plays, and the level and the last five results
        // are not said anywhere else on the row.
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
        Self {
            placed,
            peak_under_rank,
        }
    }

    /// Where the first column starts and the last one ends, from the row's
    /// left edge.
    pub(crate) fn span(&self) -> Option<(f32, f32)> {
        Some((self.placed.first()?.left, self.placed.last()?.right))
    }
}

/// The air between two columns.
const GAP: f32 = space::LG;

/// The rank when room is tight: the emblem alone.
pub(crate) const RANK_EMBLEM: f32 = 44.0;

/// The peak column: the same emblem and name as the rank, and its act.
const PEAK: f32 = 164.0;

/// How wide a column is drawn, which for the rank depends on the room.
fn drawn_width(column: &Column, tight: bool) -> f32 {
    if tight && column.head == "rank" {
        RANK_EMBLEM
    } else {
        column.width
    }
}

/// What a set of columns needs.
fn width_of(keep: &[&Column], tight: bool) -> f32 {
    keep.iter().map(|c| drawn_width(c, tight) + GAP).sum()
}

/// Which columns fit in the room available, after the ones switched off.
///
/// Built by adding rather than by shedding, since dropping a wide column to
/// save a few points leaves room nothing is ever offered. The first pass
/// ignores `hidden`, so switching one column off can never take a different
/// one away with it.
pub(crate) fn columns_for(room: f32, hidden: &[String], tight: bool) -> Vec<&'static Column> {
    let shown = fill(room, &[], &[], tight);
    let kept: Vec<&Column> = shown
        .into_iter()
        .filter(|c| !hidden.iter().any(|h| h == c.head))
        .collect();
    fill(room, &kept, hidden, tight)
}

/// Adds columns in priority order, keeping any that still fit, and returns
/// them in table order.
fn fill(
    room: f32,
    start: &[&'static Column],
    hidden: &[String],
    tight: bool,
) -> Vec<&'static Column> {
    let visible = |c: &&Column| !hidden.iter().any(|h| h == c.head);
    let order = |c: &&Column| COLUMNS.iter().position(|o| o.head == c.head).unwrap_or(0);
    let mut keep: Vec<&Column> = start.to_vec();
    // The core columns are not negotiable: a window too narrow for them is a
    // window that gets a wide row, not a board without a rank.
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

    /// The core survives any width, and a narrow window keeps nothing else.
    #[test]
    fn a_narrow_row_keeps_the_rank_and_the_kd() {
        assert_eq!(heads(10.0, &[]), ["rank", "k/d"]);
    }

    /// A wide window keeps everything, in table order.
    #[test]
    fn a_wide_row_keeps_every_column_in_order() {
        let all: Vec<&str> = COLUMNS.iter().map(|c| c.head).collect();
        assert_eq!(heads(10_000.0, &[]), all);
    }

    /// Switching a column off can only give room to others, never take away
    /// a column that was showing.
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

    /// The places never overlap and never pass the edge of the row, at any
    /// width that has room for the core at all.
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
