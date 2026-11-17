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
use overseer_ui::{colour, motion, space};

use crate::notes::Notes;
use crate::sort::{self, Sort};
use rows::{ALLY, ENEMY, Look, Metrics, SMALL};

/// The margin down both sides of the board, wide enough on the left for a
/// party's tab to sit outside the rows.
pub(crate) const GUTTER: f32 = 22.0;

pub(crate) use card::HEIGHT as OVERLAY_HEIGHT;
pub(crate) use card::STRIP_MIDDLE;

/// egui's spacing under every widget: `item_spacing.y` in the style.
const ITEM: f32 = space::SM;

const NARROW: f32 = 640.0;

/// How long the red slab takes to wipe off the enemy block when a lobby
/// lands.
const STINGER: f32 = 1.95;

const COUNT: f32 = 0.40;

const COUNT_DELAY: f32 = 0.10;

/// Whose side a block of rows is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Enemy,
    Ally,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Window,
    Overlay,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Scene<'a> {
    pub(crate) board: &'a Board,
    pub(crate) sort: &'a Sort,
    pub(crate) selected: Option<&'a str>,
    pub(crate) notes: &'a Notes,
    pub(crate) hidden: &'a [String],
    /// Whether the enemy block comes first.
    pub(crate) enemies_first: bool,
    pub(crate) place: Place,
    pub(crate) still: bool,
    pub(crate) since: f32,
    /// Whether the window has lost the backend, so nothing on it is live.
    pub(crate) lost: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Touched {
    pub(crate) drew: f32,
    pub(crate) clicked: Option<String>,
    pub(crate) hovered: Option<String>,
    pub(crate) heading: Option<&'static str>,
    pub(crate) worth: Option<String>,
    pub(crate) waiting: Option<Asked>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asked {
    LastStats,
    Reconnect,
}

pub(crate) fn teams(board: &Board, enemies_first: bool) -> [(Side, &str); 2] {
    let ours = board.self_team.as_deref().unwrap_or("Blue");
    let theirs = if free_for_all(board) {
        EVERYONE
    } else if ours == "Blue" {
        "Red"
    } else {
        "Blue"
    };
    if enemies_first {
        [(Side::Enemy, theirs), (Side::Ally, ours)]
    } else {
        [(Side::Ally, ours), (Side::Enemy, theirs)]
    }
}

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

pub(crate) fn on(player: &overseer_core::Player, team: &str) -> bool {
    if team == EVERYONE {
        !player.is_self
    } else {
        player.team.as_deref() == Some(team)
    }
}

pub(crate) fn side_of(board: &Board, player: &overseer_core::Player) -> Side {
    let ours = board.self_team.as_deref();
    if ours.is_some() && player.team.as_deref() == ours {
        Side::Ally
    } else {
        Side::Enemy
    }
}

pub(crate) fn roster<'a>(
    board: &'a Board,
    team: &str,
    sort: &Sort,
) -> Vec<&'a overseer_core::Player> {
    let mut players: Vec<&overseer_core::Player> =
        board.players.iter().filter(|p| on(p, team)).collect();
    sort::apply(&mut players, sort);
    players
}

pub(crate) fn order(board: &Board, sort: &Sort, enemies_first: bool) -> Vec<String> {
    teams(board, enemies_first)
        .iter()
        .flat_map(|(_side, team)| roster(board, team, sort))
        .filter_map(|p| p.puuid.clone())
        .collect()
}

pub(crate) fn step(order: &[String], from: Option<&str>, down: bool) -> Option<String> {
    let at = from.and_then(|id| order.iter().position(|o| o == id));
    let next = match (at, down) {
        (None, _) => 0,
        (Some(i), true) => (i + 1).min(order.len().saturating_sub(1)),
        (Some(i), false) => i.saturating_sub(1),
    };
    order.get(next).cloned()
}

pub(crate) fn next_after(among: &[String], from: Option<&str>) -> Option<String> {
    let first = among.first()?;
    let at = from.and_then(|id| among.iter().position(|f| f == id));
    Some(
        at.map_or(first, |i| among.get(i + 1).unwrap_or(first))
            .clone(),
    )
}

pub(crate) fn settling(since: f32, still: bool) -> bool {
    !still && since < (COUNT_DELAY + COUNT).max(STINGER) + 0.05
}

fn expo(t: f32) -> f32 {
    if t >= 1.0 {
        1.0
    } else {
        1.0 - (-10.0 * t.max(0.0)).exp2()
    }
}

pub(crate) fn draw(ui: &mut Ui, scene: &Scene<'_>) -> Touched {
    let mut touched = Touched::default();
    let order = teams(scene.board, scene.enemies_first);
    let frame = egui::Frame::NONE.inner_margin(Margin::symmetric(GUTTER as i8, 0));
    match scene.place {
        Place::Window => {
            let scrolled = ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
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

fn content(ui: &mut Ui, scene: &Scene<'_>, order: &[(Side, &str); 2], touched: &mut Touched) {
    ui.add_space(space::LG);
    let narrow = ui.available_width() < NARROW;
    let identity = identity(narrow);
    let mut hidden = scene.hidden.to_vec();
    if scene.board.players.iter().all(|p| p.met() == 0) {
        hidden.push("met".to_owned());
    }
    if scene.board.map.is_none() {
        hidden.push("map".to_owned());
    }
    let grid = grid::Grid::new(ui.available_width(), identity, &hidden);
    let mut shown = 0;
    // The column heads go over the first block drawn. In agent select the
    // enemy block is empty, so that is your own five.
    let mut headed = false;
    for (side, team) in order {
        let players = roster(scene.board, team, scene.sort);
        if players.is_empty() {
            continue;
        }
        shown += players.len();
        ui.push_id(team, |ui| {
            if heads::team(ui, scene.board, *side, team) {
                touched.worth = Some((*team).to_owned());
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
        return;
    }
    ladder::draw(ui, scene.board, order);
    foot::notice(ui, scene.board);
    foot::session(ui, scene.board);
}

pub(crate) fn full_width() -> f32 {
    2.0f32.mul_add(GUTTER, identity(false) + grid::full())
}

fn identity(narrow: bool) -> f32 {
    if narrow {
        SMALL.crop() + space::LG + 116.0
    } else {
        ENEMY.crop() + space::LG + 196.0
    }
}

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
            nothing: rows::nothing_yet(scene.board),
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
