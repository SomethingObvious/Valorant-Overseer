//! The board, drawn as a broadcast graphic. The window, the overlay and the
//! snapshot tests all draw it through [`draw`] from one [`Scene`], so a
//! snapshot shows the same board the window does.

mod card;
mod foot;
pub(crate) mod grid;
mod heads;
mod ladder;
pub(crate) mod paint;
mod rows;
pub(crate) mod tip;

pub(crate) use grid::COLUMNS;
pub(crate) use rows::{few_games, fit, shown_name};

use egui::{Margin, Rect, ScrollArea, Ui, pos2};
use overseer_core::Board;
use overseer_ui::{Face, colour, motion, size, space};

use crate::notes::Notes;
use crate::sort::{self, Sort};
use rows::{ALLY, ENEMY, Look, Metrics, SMALL};

/// The margin down both sides of the board, wide enough on the left for a
/// party's tab to sit outside the rows.
pub(crate) const GUTTER: f32 = 22.0;

pub(crate) use card::HEIGHT as OVERLAY_HEIGHT;

/// egui's spacing under every widget: `item_spacing.y` in the style.
const ITEM: f32 = space::SM;

/// Under this content width the board draws its narrow rows.
const NARROW: f32 = 640.0;

/// How long the red slab takes to wipe off the enemy block when a lobby
/// lands.
const STINGER: f32 = 0.28;

/// How long the numerals take to count up, and how long after the lobby
/// lands they start.
const COUNT: f32 = 0.40;

/// See [`COUNT`].
const COUNT_DELAY: f32 = 0.10;

/// Whose side a block of rows is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    /// The other team, or everybody else in a deathmatch.
    Enemy,
    /// Yours.
    Ally,
}

/// Where the board is being drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// The window: both teams, the heads, the ladder and the session.
    Window,
    /// Over the game: the enemy only, smaller, nothing to click.
    Overlay,
}

/// Everything a board needs to be drawn, and nothing it could change.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scene<'a> {
    /// What the bridge last sent.
    pub(crate) board: &'a Board,
    /// How the rows are ordered.
    pub(crate) sort: &'a Sort,
    /// What the search is narrowing to.
    pub(crate) filter: &'a str,
    /// Who is chosen, by account id.
    pub(crate) selected: Option<&'a str>,
    /// What has been written about whom.
    pub(crate) notes: &'a Notes,
    /// Which columns are switched off.
    pub(crate) hidden: &'a [String],
    /// Whether the enemy block comes first.
    pub(crate) enemies_first: bool,
    /// Where it is being drawn.
    pub(crate) place: Place,
    /// The efficient tier: nothing moves and nothing blurs.
    pub(crate) still: bool,
    /// How long ago this lobby landed, in seconds.
    pub(crate) since: f32,
}

/// What the pointer did to the board this frame, and how tall it came out.
#[derive(Debug, Default)]
pub(crate) struct Touched {
    /// How tall the board drew.
    pub(crate) drew: f32,
    /// The account whose row was clicked.
    pub(crate) clicked: Option<String>,
    /// The account whose row the pointer is over.
    pub(crate) hovered: Option<String>,
    /// The column head that was clicked.
    pub(crate) heading: Option<&'static str>,
    /// The side whose worth-a-look chip was clicked.
    pub(crate) worth: Option<String>,
}

/// The two sides, in reading order, as `(side, team id)`.
///
/// Enemies come first by default, unlike the game's own scoreboard, because
/// the other five are why the app is open.
pub(crate) fn teams(board: &Board, enemies_first: bool) -> [(Side, String); 2] {
    let ours = board.self_team.as_deref().unwrap_or("Blue").to_owned();
    let theirs = if free_for_all(board) {
        EVERYONE
    } else if ours == "Blue" {
        "Red"
    } else {
        "Blue"
    }
    .to_owned();
    if enemies_first {
        [(Side::Enemy, theirs), (Side::Ally, ours)]
    } else {
        [(Side::Ally, ours), (Side::Enemy, theirs)]
    }
}

/// The team id that means everybody but you, in a lobby with no sides.
pub(crate) const EVERYONE: &str = "*";

/// Whether this lobby has more than two teams. That means a deathmatch,
/// where Riot puts every player on a team of their own.
fn free_for_all(board: &Board) -> bool {
    let mut seen: Vec<&str> = Vec::new();
    for team in board.players.iter().filter_map(|p| p.team.as_deref()) {
        if !seen.contains(&team) {
            seen.push(team);
        }
    }
    seen.len() > 2
}

/// Whether a player belongs under a team id, [`EVERYONE`] included.
pub(crate) fn on(player: &overseer_core::Player, team: &str) -> bool {
    if team == EVERYONE {
        !player.is_self
    } else {
        player.team.as_deref() == Some(team)
    }
}

/// Whose side an account is on: yours if it shares your team, the
/// enemy's otherwise, including when the backend has not said which team
/// you are, because the other five are the ones worth being careful about.
pub(crate) fn side_of(board: &Board, player: &overseer_core::Player) -> Side {
    let ours = board.self_team.as_deref();
    if ours.is_some() && player.team.as_deref() == ours {
        Side::Ally
    } else {
        Side::Enemy
    }
}

/// Who is on one side, in the order the board draws them: sorted by the
/// clicked heading, minus anybody the search hides. The arrow keys walk this
/// same list, so they only step through people on screen.
pub(crate) fn roster<'a>(
    board: &'a Board,
    team: &str,
    sort: &Sort,
    filter: &str,
) -> Vec<&'a overseer_core::Player> {
    let mut players: Vec<&overseer_core::Player> =
        board.players.iter().filter(|p| on(p, team)).collect();
    sort::apply(&mut players, sort);
    players.retain(|p| sort::matches(p, filter));
    players
}

/// Every account on screen, top to bottom, by id.
pub(crate) fn order(board: &Board, sort: &Sort, filter: &str, enemies_first: bool) -> Vec<String> {
    teams(board, enemies_first)
        .iter()
        .flat_map(|(_side, team)| roster(board, team, sort, filter))
        .filter_map(|p| p.puuid.clone())
        .collect()
}

/// The account a step up or down from `from`, stopping at either end, or
/// the first one when nothing was chosen or the choice is not on screen.
pub(crate) fn step(order: &[String], from: Option<&str>, down: bool) -> Option<String> {
    let at = from.and_then(|id| order.iter().position(|o| o == id));
    let next = match (at, down) {
        (None, _) => 0,
        (Some(i), true) => (i + 1).min(order.len().saturating_sub(1)),
        (Some(i), false) => i.saturating_sub(1),
    };
    order.get(next).cloned()
}

/// The next of `among` after `from`, wrapping round to the first.
pub(crate) fn next_after(among: &[String], from: Option<&str>) -> Option<String> {
    let first = among.first()?;
    let at = from.and_then(|id| among.iter().position(|f| f == id));
    Some(
        at.map_or(first, |i| among.get(i + 1).unwrap_or(first))
            .clone(),
    )
}

/// Whether the board is still arriving, and so wants frames.
///
/// The count-up and the stinger are painted from how long ago the lobby
/// landed, and nothing else asks for frames while they run.
pub(crate) fn settling(since: f32, still: bool) -> bool {
    !still && since < (COUNT_DELAY + COUNT).max(STINGER) + 0.05
}

/// Exponential ease out: fast, then settling.
fn expo(t: f32) -> f32 {
    if t >= 1.0 {
        1.0
    } else {
        1.0 - (-10.0 * t.max(0.0)).exp2()
    }
}

/// Draws the board, and says what the pointer did to it.
pub(crate) fn draw(ui: &mut Ui, scene: &Scene<'_>) -> Touched {
    let mut touched = Touched::default();
    let order = teams(scene.board, scene.enemies_first);
    let frame = egui::Frame::NONE.inner_margin(Margin::symmetric(GUTTER as i8, 0));
    match scene.place {
        Place::Window => {
            let scrolled = ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Past the width every column fits in, more room is only
                    // a longer way for the eye to travel from a name to its
                    // K/D, so the board stops there and sits in the middle.
                    let room = ui.available_rect_before_wrap();
                    let spare = ((room.width() - full_width()) / 2.0).max(0.0);
                    let inside = Rect::from_min_max(
                        pos2(room.left() + spare, room.top()),
                        pos2(room.right() - spare, room.bottom()),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(inside), |ui| {
                        frame.show(ui, |ui| content(ui, scene, &order, &mut touched));
                    });
                });
            touched.drew = scrolled.content_size.y;
        }
        Place::Overlay => {
            touched = frame.show(ui, |ui| card::overlay(ui, scene, &order)).inner;
        }
    }
    touched
}

/// Everything inside the margins.
fn content(ui: &mut Ui, scene: &Scene<'_>, order: &[(Side, String); 2], touched: &mut Touched) {
    ui.add_space(space::LG);
    // A window too small for the full row gets the smaller face, and a
    // name column that gives up its last forty points before the numbers
    // give up anything.
    let narrow = ui.available_width() < NARROW;
    let identity = identity(narrow);
    // A column with nothing in it for anybody in the lobby is dropped for
    // this lobby, which gives its room to one that has something to say.
    // Only for a column whose emptiness cannot change mid-match: who you
    // have met is read from history once, not filled in as the game goes.
    let mut hidden = scene.hidden.to_vec();
    if scene.board.players.iter().all(|p| p.met() == 0) {
        hidden.push("met".to_owned());
    }
    // No map, as in the party lobby, and every row would say "-".
    if scene.board.map.is_none() {
        hidden.push("map".to_owned());
    }
    let grid = grid::Grid::new(ui.available_width(), identity, &hidden);
    let mut shown = 0;
    // The column heads go over the first block drawn. In agent select the
    // enemy block is empty, so that is your own five.
    let mut headed = false;
    for (side, team) in order {
        let players = roster(scene.board, team, scene.sort, scene.filter);
        if players.is_empty() {
            continue;
        }
        shown += players.len();
        ui.push_id(team, |ui| {
            if heads::team(ui, scene.board, *side, team).worth {
                touched.worth = Some(team.clone());
            }
            if !headed {
                headed = true;
                if let Some(head) = heads::columns(ui, &grid, scene.sort) {
                    touched.heading = Some(head);
                }
            }
            block(ui, scene, (*side, team, narrow), &players, &grid, touched);
        });
        ui.add_space(space::XL);
    }
    if shown == 0 {
        nobody(ui, scene.filter);
        return;
    }
    ladder::draw(ui, scene.board, order);
    foot::notice(ui, scene.board);
    foot::session(ui, scene.board);
}

/// How wide the board has to be to show every column at full size, its
/// gutters included. The window gives anything past this to the panel.
pub(crate) fn full_width() -> f32 {
    2.0f32.mul_add(GUTTER, identity(false) + grid::full())
}

/// How wide the face and the name are, before the first column.
fn identity(narrow: bool) -> f32 {
    if narrow {
        SMALL.crop() + space::LG + 116.0
    } else {
        ENEMY.crop() + space::LG + 196.0
    }
}

/// One side's rows, with the stinger over the enemy's while a lobby lands.
fn block(
    ui: &mut Ui,
    scene: &Scene<'_>,
    (side, team, narrow): (Side, &str, bool),
    players: &[&overseer_core::Player],
    grid: &grid::Grid,
    touched: &mut Touched,
) {
    let metrics: Metrics = match (side, narrow) {
        (Side::Enemy, true) => SMALL,
        (Side::Enemy, false) => ENEMY,
        (Side::Ally, _) => ALLY,
    };
    let counted = if scene.still {
        1.0
    } else {
        motion::eased(((scene.since - COUNT_DELAY) / COUNT).clamp(0.0, 1.0))
    };
    let top = ui.cursor().top();
    let mut placed = Vec::with_capacity(players.len());
    for player in players {
        let id = player.puuid.as_deref();
        let look = Look {
            side,
            metrics,
            selected: id.is_some() && id == scene.selected,
            noted: id.is_some_and(|id| scene.notes.has(id)),
            tag: id.and_then(|id| scene.notes.tag(id)),
            note: id.and_then(|id| scene.notes.text(id)),
            counted,
            still: scene.still,
        };
        let response = rows::row(ui, player, grid, &look);
        placed.push(response.rect);
        if response.clicked() {
            touched.clicked.clone_from(&player.puuid);
        }
        if response.hovered() {
            touched.hovered.clone_from(&player.puuid);
        }
    }
    let members: Vec<&overseer_core::Player> =
        scene.board.players.iter().filter(|p| on(p, team)).collect();
    let tints = paint::party_tints(&members, side);
    for party in paint::spines(players, &tints) {
        if let (Some(first), Some(last)) = (placed.get(party.rows.0), placed.get(party.rows.1)) {
            paint::spine(ui.painter(), &party, first.union(*last));
        }
    }
    if side == Side::Enemy && !scene.still && scene.since < STINGER {
        // The stinger: the plate's red wipes off the rows left to right,
        // leading with the slant, once per lobby.
        let area = Rect::from_min_max(
            pos2(ui.min_rect().left(), top),
            pos2(ui.max_rect().right(), ui.cursor().top()),
        );
        let from = area
            .width()
            .mul_add(expo(scene.since / STINGER), area.left());
        let cover = Rect::from_min_max(pos2(from, area.top()), area.max);
        if cover.width() > 1.0 {
            ui.painter()
                .add(paint::slant(cover, true, false, colour::ENEMY));
        }
    }
}

/// What to say when the search has hidden everybody. With no search it says
/// nothing, since an empty board then means agent select.
fn nobody(ui: &mut Ui, filter: &str) {
    if filter.trim().is_empty() {
        return;
    }
    ui.add_space(space::XXL);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Nobody matches \"{}\". Esc clears the search.",
                filter.trim()
            ))
            .color(colour::TEXT_DIM)
            .font(Face::Body.at(size::BODY)),
        );
    });
    ui.add_space(space::XXL);
}

#[cfg(test)]
mod tests {
    use super::{GUTTER, grid::Grid, identity, next_after, step};

    /// The overlay has room for the rank, the K/D and the last five, with
    /// or without the met column.
    #[test]
    fn the_overlay_keeps_the_kd_and_the_last_five() {
        let room = 2.0f32.mul_add(-GUTTER, crate::overlay::WIDTH);
        for hidden in [vec![], vec!["met".to_owned()]] {
            let grid = Grid::new(room, identity(true), &hidden);
            let heads: Vec<&str> = grid.placed.iter().map(|p| p.column.head).collect();
            for wanted in ["rank", "k/d", "last 5"] {
                assert!(heads.contains(&wanted), "the overlay shows {heads:?}");
            }
        }
    }

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    /// The arrows stop at either end rather than wrapping: the board is a
    /// column, and going off the bottom of a column to its top is a jump
    /// nobody asked for.
    #[test]
    fn a_step_stops_at_either_end() {
        let order = ids(&["a", "b", "c"]);
        assert_eq!(step(&order, Some("b"), true).as_deref(), Some("c"));
        assert_eq!(step(&order, Some("c"), true).as_deref(), Some("c"));
        assert_eq!(step(&order, Some("a"), false).as_deref(), Some("a"));
        assert_eq!(step(&order, Some("b"), false).as_deref(), Some("a"));
    }

    /// Somebody the search has hidden, or nobody at all, starts at the top.
    #[test]
    fn a_step_from_nowhere_starts_at_the_top() {
        let order = ids(&["a", "b"]);
        assert_eq!(step(&order, None, true).as_deref(), Some("a"));
        assert_eq!(step(&order, Some("gone"), false).as_deref(), Some("a"));
        assert_eq!(step(&[], Some("a"), true), None);
    }

    /// Worth-a-look wraps: it is a cycle through a handful of accounts.
    #[test]
    fn the_next_flagged_account_wraps_round() {
        let flagged = ids(&["x", "y"]);
        assert_eq!(next_after(&flagged, None).as_deref(), Some("x"));
        assert_eq!(next_after(&flagged, Some("x")).as_deref(), Some("y"));
        assert_eq!(next_after(&flagged, Some("y")).as_deref(), Some("x"));
        assert_eq!(
            next_after(&flagged, Some("elsewhere")).as_deref(),
            Some("x")
        );
        assert_eq!(next_after(&[], Some("x")), None);
    }
}
