//! The window itself.
//!
//! P0 draws the board and nothing else, because the point of P0 is to prove
//! the expensive claims: that the window opens on the integrated GPU, that it
//! shows live data, and that it costs nothing while sitting still. The look
//! arrives in P1, on top of this.
//!
//! The idle cost is the part worth reading carefully. egui repaints when
//! something asks it to, and the only thing asking here is the bridge thread,
//! which wakes the UI when a board arrives. There is no timer, no polling and
//! no animation clock yet, so a window nobody is touching draws no frames.

use std::path::Path;

use eframe::{App, CreationContext, Frame};
use egui::{
    Align, Align2, CentralPanel, Color32, FontId, Panel, Rect, RichText, ScrollArea, Sense, Theme,
    ThemePreference, Ui, pos2, vec2,
};
use overseer_core::{Board, Bridge, Event, Player, Status};

use crate::theme;

/// One row's height. Enough for a name at the body size with air around it.
const ROW: f32 = 24.0;
/// The body text size. One size for everything in P0; the scale arrives with
/// the rest of the look.
const BODY: f32 = 13.0;

/// A column of a player row: its label, and how much width it gets.
struct Column {
    /// What the header says.
    head: &'static str,
    /// Drawn width, in points, before the next column starts.
    width: f32,
}

/// The board's columns, in order. Painted at fixed offsets rather than laid
/// out: a row is a handful of strings, and going through egui's layout for
/// each of them costs more than the row is worth when ten redraw together.
const COLUMNS: [Column; 6] = [
    Column {
        head: "AGENT",
        width: 78.0,
    },
    Column {
        head: "PLAYER",
        width: 168.0,
    },
    Column {
        head: "RANK",
        width: 104.0,
    },
    Column {
        head: "K/D",
        width: 52.0,
    },
    Column {
        head: "WIN",
        width: 52.0,
    },
    Column {
        head: "LVL",
        width: 48.0,
    },
];

/// The window's state: a connection, the last board it sent, and what to say
/// when there is no board yet.
pub(crate) struct Overseer {
    bridge: Bridge,
    board: Board,
    status: Status,
    /// Set when the bridge has given up, which is a different thing from being
    /// disconnected: a bad token will not fix itself, so the window says so
    /// instead of showing a spinner for ever.
    stopped: Option<String>,
    /// How many boards have arrived. In the footer during P0 because it is the
    /// cheapest proof that the socket is alive and the frame is not stale.
    boards: u64,
}

impl std::fmt::Debug for Overseer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Overseer")
            .field("status", &self.status)
            .field("players", &self.board.players.len())
            .field("boards", &self.boards)
            .finish_non_exhaustive()
    }
}

impl Overseer {
    /// Starts the bridge and takes the style.
    ///
    /// The waker handed to the bridge is a clone of egui's context, which is
    /// the whole reason the idle cost is nothing: the thread asks for a repaint
    /// when it has something, rather than the window asking every frame whether
    /// anything has happened.
    pub(crate) fn new(cc: &CreationContext<'_>, root: &Path) -> Self {
        cc.egui_ctx.set_theme(ThemePreference::Dark);
        cc.egui_ctx.set_style_of(Theme::Dark, theme::style());
        // Printed once, because which adapter the request actually got is the
        // difference between a scoreboard that leaves the game alone and one
        // that does not. There is nowhere else this is knowable.
        if let Some(state) = cc.wgpu_render_state.as_ref() {
            println!("{}", crate::probe::adapter_line(&state.adapter.get_info()));
        }
        let ctx = cc.egui_ctx.clone();
        let bridge = Bridge::start(root, move || ctx.request_repaint());
        Self {
            bridge,
            board: Board::default(),
            status: Status::Connecting(String::new()),
            stopped: None,
            boards: 0,
        }
    }

    /// Takes everything the bridge has queued since the last frame.
    fn pump(&mut self) {
        for event in self.bridge.drain() {
            match event {
                Event::Status(status) => self.status = status,
                Event::Board(board) => {
                    self.board = *board;
                    self.boards = self.boards.saturating_add(1);
                }
                Event::Stopped(why) => {
                    self.stopped = Some(why.clone());
                    self.status = Status::Lost(why);
                }
            }
        }
    }

    /// The state of the game, the map, and whether the bridge is answering.
    fn header(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new("VALORANT")
                    .color(theme::RED)
                    .size(15.0)
                    .strong(),
            );
            ui.label(
                RichText::new("OVERSEER")
                    .color(theme::BONE)
                    .size(15.0)
                    .strong(),
            );
            ui.label(RichText::new("|").color(theme::LINE));
            let label = self
                .board
                .state_label
                .as_deref()
                .or(self.board.state.as_deref())
                .unwrap_or("WAITING");
            ui.label(RichText::new(label).color(theme::BONE).strong());
            if let Some(map) = self.board.map.as_deref() {
                ui.label(RichText::new(map).color(theme::TEXT));
            }
            if let Some(mode) = self.board.mode.as_deref() {
                ui.label(RichText::new(mode).color(theme::DIM));
            }
            if let Some(side) = self.board.side.as_deref() {
                let colour = if side.eq_ignore_ascii_case("attack") {
                    theme::RED
                } else {
                    theme::ALLY
                };
                ui.label(RichText::new(side.to_uppercase()).color(colour).strong());
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                let (mark, colour, text) = self.connection();
                ui.label(RichText::new(format!("{mark} {text}")).color(colour));
            });
        });
    }

    /// The connection light: a mark, a colour and the reason behind it.
    fn connection(&self) -> (&'static str, Color32, String) {
        match (&self.stopped, &self.status) {
            (Some(why), _) | (None, Status::Lost(why)) => ("o", theme::RED, why.clone()),
            (None, Status::Live) => ("*", theme::ALLY, "live".to_owned()),
            (None, Status::Connecting(detail)) => ("o", theme::GOLD, connecting_text(detail)),
        }
    }
}

/// One team's block, or nothing when that side is empty.
fn team(ui: &mut Ui, label: &str, colour: Color32, players: &[&Player]) {
    if players.is_empty() {
        return;
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(RichText::new(label).color(colour).strong());
        let flagged = players.iter().filter(|p| p.smurf).count();
        if flagged > 0 {
            ui.label(RichText::new(format!("! {flagged} flagged")).color(theme::GOLD));
        }
    });
    heading_row(ui);
    for player in players {
        row(ui, player, colour);
    }
}

impl App for Overseer {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        self.pump();

        let chrome = egui::Frame::NONE.fill(theme::INK);
        Panel::top("header")
            .exact_size(30.0)
            .frame(chrome)
            .show(ui, |ui| {
                self.header(ui);
            });
        Panel::bottom("footer")
            .exact_size(20.0)
            .frame(chrome)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("{} boards", self.boards))
                            .color(theme::FAINT)
                            .size(11.0),
                    );
                });
            });
        CentralPanel::default().frame(chrome).show(ui, |ui| {
            if self.board.players.is_empty() {
                empty(ui, &self.status, self.stopped.as_deref());
                return;
            }
            body(ui, &self.board);
        });
    }
}

/// Both team blocks, in the order they are read: ours, then theirs.
fn body(ui: &mut Ui, board: &Board) {
    let ours = board.self_team.as_deref().unwrap_or("Blue");
    let theirs = if ours == "Blue" { "Red" } else { "Blue" };
    ScrollArea::vertical().show(ui, |ui| {
        team(ui, "ALLIES", theme::ALLY, &board.team(ours));
        team(ui, "ENEMIES", theme::RED, &board.team(theirs));
    });
}

/// The same body the window draws, for the snapshot test. An empty board takes
/// the waiting screen, so both states go through one door.
#[cfg(test)]
pub(crate) fn snapshot_body(ui: &mut Ui, board: &Board) {
    if board.players.is_empty() {
        empty(ui, &Status::Connecting(String::new()), None);
    } else {
        body(ui, board);
    }
}

/// What to say while there is no board. An empty screen is a place to say what
/// is happening, not a place to say nothing.
fn empty(ui: &mut Ui, status: &Status, stopped: Option<&str>) {
    let (title, detail): (&str, &str) = match (stopped, status) {
        (Some(why), _) => ("The bridge refused this build", why),
        (None, Status::Live) => (
            "Signed in, nothing in progress",
            "Open VALORANT to see a lobby",
        ),
        (None, Status::Connecting(_)) => (
            "Looking for the backend",
            "It writes its port once it is listening",
        ),
        (None, Status::Lost(why)) => ("Not connected", why),
    };
    ui.vertical_centered(|ui| {
        ui.add_space(72.0);
        ui.label(RichText::new(title).color(theme::BONE).size(16.0));
        ui.add_space(4.0);
        ui.label(RichText::new(detail).color(theme::FAINT));
    });
}

/// The header's connection text, which has no detail on the first attempt.
fn connecting_text(detail: &str) -> String {
    if detail.is_empty() {
        "connecting".to_owned()
    } else {
        format!("connecting, {detail}")
    }
}

/// The column headers, once per team block so a long board stays readable.
fn heading_row(ui: &mut Ui) {
    let (rect, _response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let mut x = rect.left() + 10.0;
    for column in &COLUMNS {
        ui.painter().text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            column.head,
            FontId::proportional(10.0),
            theme::FAINT,
        );
        x += column.width;
    }
}

/// One player, as one row of fixed height.
fn row(ui: &mut Ui, player: &Player, team: Color32) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 0, theme::LINE.gamma_multiply(0.35));
    }
    if player.is_self {
        let mut bar = rect;
        bar.set_width(2.0);
        painter.rect_filled(bar, 0, theme::RED);
    }

    let name_colour = if player.is_self { theme::BONE } else { team };
    let cells: [(String, Color32); 6] = [
        (player.agent.clone().unwrap_or_else(dash), theme::ICE),
        (player.name.clone().unwrap_or_else(dash), name_colour),
        (
            player.rank.clone().unwrap_or_else(dash),
            theme::rank(player.rank_tier),
        ),
        (number(player.kd, 2), theme::kd(player.kd)),
        (percent(player.win_rate), theme::TEXT),
        (
            player.level.map_or_else(dash, |l| l.to_string()),
            theme::FAINT,
        ),
    ];

    let mut x = rect.left() + 10.0;
    for (index, (text, colour)) in cells.iter().enumerate() {
        let width = COLUMNS.get(index).map_or(60.0, |c| c.width);
        // Clipped per cell, so a long name is cut off rather than pushing the
        // numbers out of line. A clipped name is readable; a shifted column is
        // not.
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(width - 6.0, rect.height()));
        painter.with_clip_rect(cell.intersect(rect)).text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(BODY),
            *colour,
        );
        x += width;
    }
    if player.smurf {
        painter.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            "!",
            FontId::proportional(BODY),
            theme::GOLD,
        );
    }
    painter.hline(
        rect.x_range(),
        rect.bottom(),
        (1.0, theme::LINE.gamma_multiply(0.5)),
    );
}

/// What a missing value looks like, in one place.
fn dash() -> String {
    "-".to_owned()
}

/// A number to a fixed number of places, or a dash when it is missing.
fn number(value: Option<f64>, places: usize) -> String {
    value.map_or_else(dash, |v| format!("{v:.places$}"))
}

/// A percentage with no decimal, or a dash.
fn percent(value: Option<f64>) -> String {
    value.map_or_else(dash, |v| format!("{}%", v.round()))
}

#[cfg(test)]
mod tests {
    use super::{COLUMNS, connecting_text, number, percent};

    #[test]
    fn missing_numbers_read_as_missing() {
        assert_eq!(number(None, 2), "-");
        assert_eq!(number(Some(0.914), 2), "0.91");
        assert_eq!(percent(None), "-");
        assert_eq!(percent(Some(52.4)), "52%");
    }

    #[test]
    fn the_first_attempt_has_no_detail_to_show() {
        assert_eq!(connecting_text(""), "connecting");
        assert_eq!(connecting_text("retry 2"), "connecting, retry 2");
    }

    /// The row paints one cell per column at a fixed offset, so the two lists
    /// have to be the same length or a cell is drawn at the wrong width.
    #[test]
    fn every_column_is_wide_enough_for_its_own_heading() {
        assert_eq!(COLUMNS.len(), 6);
        for column in &COLUMNS {
            assert!(
                column.width >= 40.0,
                "{} is too narrow to hold a value",
                column.head
            );
        }
    }
}
