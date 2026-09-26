//! The board, drawn as a broadcast graphic: the enemy team on a solid slab
//! with a face at the head of every row, your own team quieter under it, and
//! the lobby on one ladder beneath.
//!
//! One entry point, [`draw`], from one plain description of what to show,
//! [`Scene`]. The window, the overlay and the snapshot tests all go through
//! it. They used to assemble the board three ways, and a picture of the
//! board that is not the board the window draws is a picture of nothing.

mod foot;
pub(crate) mod grid;
mod heads;
mod ladder;
pub(crate) mod paint;
mod rows;

use egui::{Margin, Rect, ScrollArea, Ui, pos2};
use overseer_core::Board;
use overseer_ui::{Face, colour, motion, size, space};

use crate::notes::Notes;
use crate::sort::{self, Sort};
use rows::{ALLY, ENEMY, Look, Metrics, OVERLAY};

pub(crate) use grid::COLUMNS;

/// The margin down both sides of the board.
///
/// Wide enough on the left for a party bracket to sit in it without
/// touching a row: a bracket inside the rows would be one more column, and
/// against the window's edge it would look like a rendering fault.
pub(crate) const GUTTER: f32 = 18.0;

/// How tall the overlay's board is with a full enemy team: the plate and
/// five rows, each with its gap, and the air under the last.
pub(crate) const OVERLAY_HEIGHT: f32 =
    heads::PLATE - 8.0 + space::SM + (OVERLAY.height + rows::GAP) * 5.0 + space::MD;

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
    /// The other five: the reason the app is open.
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
/// Enemies first by default, which is a deliberate break from the game's own
/// scoreboard. The game already shows you your side at the top; the reason
/// to open this app is the other five, and putting them second means the
/// answer you came for is below the one you already had.
pub(crate) fn teams(board: &Board, enemies_first: bool) -> [(Side, String); 2] {
    let ours = board.self_team.as_deref().unwrap_or("Blue").to_owned();
    let theirs = if ours == "Blue" { "Red" } else { "Blue" }.to_owned();
    if enemies_first {
        [(Side::Enemy, theirs), (Side::Ally, ours)]
    } else {
        [(Side::Ally, ours), (Side::Enemy, theirs)]
    }
}

/// Whether the board is still arriving, and so wants frames.
///
/// The only motion on the board that nothing else asks frames for: it is
/// painted from how long ago the lobby landed, so the window has to keep
/// itself awake until it is done, and not a frame longer.
pub(crate) fn settling(since: f32, still: bool) -> bool {
    !still && since < (COUNT_DELAY + COUNT).max(STINGER) + 0.05
}

/// Exponential ease out: fast, then settling. The broadcast's own curve.
fn expo(t: f32) -> f32 {
    if t >= 1.0 {
        1.0
    } else {
        1.0 - 2f32.powf(-10.0 * t.max(0.0))
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
                    frame.show(ui, |ui| content(ui, scene, &order, &mut touched));
                });
            touched.drew = scrolled.content_size.y;
        }
        Place::Overlay => {
            let drew = frame
                .show(ui, |ui| content(ui, scene, &order, &mut touched))
                .response
                .rect
                .height();
            touched.drew = drew;
        }
    }
    touched
}

/// Everything inside the margins.
fn content(ui: &mut Ui, scene: &Scene<'_>, order: &[(Side, String); 2], touched: &mut Touched) {
    let overlay = scene.place == Place::Overlay;
    ui.add_space(if overlay { 0.0 } else { space::LG });
    // Narrow is the overlay and any window too small for the full row:
    // the smaller face, and a name column that gives up its last forty
    // points before the numbers give up anything.
    let narrow = overlay || ui.available_width() < NARROW;
    let identity = if narrow {
        OVERLAY.crop() + space::LG + 132.0
    } else {
        ENEMY.crop() + space::LG + 196.0
    };
    // A column with nothing in it for anybody in the lobby is dropped for
    // this lobby, which gives its room to one that has something to say.
    // Only for a column whose emptiness cannot change mid-match: who you
    // have met is read from history once, not filled in as the game goes.
    let mut hidden = scene.hidden.to_vec();
    if scene.board.players.iter().all(|p| p.met() == 0) {
        hidden.push("met".to_owned());
    }
    let grid = grid::Grid::new(ui.available_width(), identity, &hidden);
    let mut shown = 0;
    for (side, team) in order {
        if overlay && *side == Side::Ally {
            continue;
        }
        let mut players = scene.board.team(team);
        sort::apply(&mut players, scene.sort);
        players.retain(|p| sort::matches(p, scene.filter));
        if players.is_empty() {
            continue;
        }
        shown += players.len();
        ui.push_id(team, |ui| {
            if heads::team(ui, scene.board, *side, team, overlay).worth {
                touched.worth = Some(team.clone());
            }
            if !overlay
                && *side == Side::Enemy
                && let Some(head) = heads::columns(ui, &grid, scene.sort)
            {
                touched.heading = Some(head);
            }
            block(ui, scene, (*side, narrow), &players, &grid, touched);
        });
        ui.add_space(if overlay { space::MD } else { space::XL });
    }
    if shown == 0 {
        nobody(ui, scene.filter);
        return;
    }
    if !overlay {
        ladder::draw(ui, scene.board, order);
        foot::notice(ui, scene.board);
        foot::session(ui, scene.board);
    }
}

/// One side's rows, with the stinger over the enemy's while a lobby lands.
fn block(
    ui: &mut Ui,
    scene: &Scene<'_>,
    (side, narrow): (Side, bool),
    players: &[&overseer_core::Player],
    grid: &grid::Grid,
    touched: &mut Touched,
) {
    let metrics: Metrics = match (side, narrow) {
        (Side::Enemy, true) => OVERLAY,
        (Side::Enemy, false) => ENEMY,
        (Side::Ally, _) => ALLY,
    };
    let counted = if scene.still {
        1.0
    } else {
        motion::eased(((scene.since - COUNT_DELAY) / COUNT).clamp(0.0, 1.0))
    };
    let brackets = paint::brackets(players);
    let top = ui.cursor().top();
    for (i, player) in players.iter().enumerate() {
        let id = player.puuid.as_deref();
        let look = Look {
            side,
            metrics,
            selected: id.is_some() && id == scene.selected,
            noted: id.is_some_and(|id| scene.notes.has(id)),
            bracket: brackets.get(i).copied().flatten(),
            counted,
            still: scene.still,
            reasons: scene.place == Place::Window,
        };
        let response = rows::row(ui, player, grid, &look);
        if response.clicked() {
            touched.clicked.clone_from(&player.puuid);
        }
        if response.hovered() {
            touched.hovered.clone_from(&player.puuid);
        }
    }
    if side == Side::Enemy && !scene.still && scene.since < STINGER {
        // The stinger: the team plate's red, wiping off the rows it was
        // covering, left to right, leading with the slant. Once per lobby,
        // and it is the only entrance on the board.
        let area = Rect::from_min_max(
            pos2(ui.min_rect().left(), top),
            pos2(ui.max_rect().right(), ui.cursor().top()),
        );
        let from = area.left() + area.width() * expo(scene.since / STINGER);
        let cover = Rect::from_min_max(pos2(from, area.top()), area.max);
        if cover.width() > 1.0 {
            ui.painter()
                .add(paint::slant(cover, true, false, colour::ENEMY));
        }
    }
}

/// What to say when the search has hidden everybody.
fn nobody(ui: &mut Ui, filter: &str) {
    ui.add_space(space::XXL);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(format!("Nobody here matches \"{}\"", filter.trim()))
                .color(colour::TEXT_DIM)
                .font(Face::Body.at(size::BODY)),
        );
    });
    ui.add_space(space::XXL);
}
