//! The window: what is on screen, and what the window itself is doing.
//!
//! Three layouts out of one set of parts. Under `COMPACT` the board is alone
//! and sheds columns; above it the detail panel takes the right; above `WIDE`
//! the panel gets room to breathe. There is no separate compact build, because
//! two builds diverge and a person dragging a window edge should not watch the
//! app turn into a different app.
//!
//! The idle cost is the part worth reading carefully. egui repaints when
//! something asks it to, and the only thing asking here is the bridge thread,
//! which wakes the UI when a board arrives. There is no timer and no polling,
//! so a window nobody is touching draws no frames at all.

use std::path::Path;

use eframe::{App, CreationContext, Frame};
use egui::{Align2, CentralPanel, Panel, RichText, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Bridge, Event, Player, Status};

use crate::board::{self, RowStyle};
use crate::design::{self, Face, colour, label_text, size, space};
use crate::panel;

/// Under this width there is no room for the panel beside the board.
const COMPACT: f32 = 720.0;
/// Above this the panel can afford its full width.
const WIDE: f32 = 1100.0;
/// The panel's width between those two.
const PANEL_NARROW: f32 = 260.0;
/// The panel's width above [`WIDE`].
const PANEL_WIDE: f32 = 320.0;

/// The window's state.
pub(crate) struct Overseer {
    bridge: Bridge,
    board: Board,
    status: Status,
    /// Set when the bridge has given up, which is a different thing from being
    /// disconnected: a bad token will not fix itself, so the window says so
    /// rather than showing a spinner for ever.
    stopped: Option<String>,
    /// Who the panel is about. Kept by account id rather than by index,
    /// because the backend reorders the board between frames.
    selected: Option<String>,
    /// How many boards have arrived, which is the cheapest proof that the
    /// socket is alive and the frame on screen is not stale.
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
    /// Starts the bridge, installs the fonts and takes the style.
    ///
    /// The waker handed to the bridge is a clone of egui's context, which is
    /// the whole reason the idle cost is nothing: the thread asks for a
    /// repaint when it has something, rather than the window asking every
    /// frame whether anything has happened.
    pub(crate) fn new(cc: &CreationContext<'_>, root: &Path) -> Self {
        design::install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, design::style());
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
            selected: None,
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
                    self.keep_selection();
                }
                Event::Stopped(why) => {
                    self.stopped = Some(why.clone());
                    self.status = Status::Lost(why);
                }
            }
        }
    }

    /// Keeps the panel pointed at somebody real.
    ///
    /// A new board can arrive with the selected player gone, which on a board
    /// keyed by position would quietly show the wrong person's numbers under
    /// the right person's name. Falling back to yourself is the least
    /// surprising thing to be looking at.
    fn keep_selection(&mut self) {
        let still_here = self.selected.as_ref().is_some_and(|id| {
            self.board
                .players
                .iter()
                .any(|p| p.puuid.as_ref() == Some(id))
        });
        if still_here {
            return;
        }
        self.selected = self
            .board
            .players
            .iter()
            .find(|p| p.is_self)
            .or_else(|| self.board.players.first())
            .and_then(|p| p.puuid.clone());
    }

    /// The selected player, if they are still on the board.
    fn current(&self) -> Option<&Player> {
        let id = self.selected.as_ref()?;
        self.board
            .players
            .iter()
            .find(|p| p.puuid.as_ref() == Some(id))
    }

    /// The title bar: who is playing what, and whether we can see it.
    fn header(&self, ui: &mut Ui) {
        let height = space::XXL + space::MD;
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter();
        let middle = rect.center().y;
        let mut x = rect.left() + space::LG;

        // The wordmark, in the one place the accent is allowed to be loud.
        let after = painter.text(
            pos2(x, middle),
            Align2::LEFT_CENTER,
            label_text("valorant"),
            Face::Display.at(size::TITLE),
            colour::ENEMY,
        );
        let after = painter.text(
            pos2(after.right() + space::MD, middle),
            Align2::LEFT_CENTER,
            label_text("overseer"),
            Face::Display.at(size::TITLE),
            colour::TEXT_STRONG,
        );
        x = after.right() + space::XL;

        let state = self
            .board
            .state_label
            .as_deref()
            .or(self.board.state.as_deref())
            .unwrap_or("waiting");
        let after = painter.text(
            pos2(x, middle),
            Align2::LEFT_CENTER,
            label_text(state),
            Face::Display.at(size::LABEL),
            colour::TEXT_DIM,
        );
        x = after.right() + space::LG;

        for (text, tint) in [
            (self.board.map.clone(), colour::TEXT_STRONG),
            (self.board.mode.clone(), colour::TEXT_DIM),
        ] {
            let Some(text) = text else { continue };
            let after = painter.text(
                pos2(x, middle),
                Align2::LEFT_CENTER,
                text,
                Face::Body.at(size::BODY),
                tint,
            );
            x = after.right() + space::MD;
        }
        if let Some(side) = self.board.side.as_deref() {
            let tint = if side.eq_ignore_ascii_case("attack") {
                colour::ENEMY
            } else {
                colour::ALLY
            };
            painter.text(
                pos2(x, middle),
                Align2::LEFT_CENTER,
                label_text(side),
                Face::Display.at(size::LABEL),
                tint,
            );
        }

        let (radius, tint, text) = self.connection();
        let drawn = painter.text(
            pos2(rect.right() - space::LG, middle),
            Align2::RIGHT_CENTER,
            text,
            Face::Body.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        painter.circle_filled(pos2(drawn.left() - space::MD, middle), radius, tint);
        painter.hline(rect.x_range(), rect.bottom() - 1.0, (1.0, colour::LINE));
    }

    /// The connection light: a radius, a colour, and the reason behind it.
    fn connection(&self) -> (f32, egui::Color32, String) {
        match (&self.stopped, &self.status) {
            (Some(why), _) | (None, Status::Lost(why)) => (3.0, colour::ENEMY, why.clone()),
            (None, Status::Live) => (3.0, colour::ALLY, "live".to_owned()),
            (None, Status::Connecting(detail)) => (2.0, colour::WARN, connecting_text(detail)),
        }
    }

    /// The board, both teams, with the headings each side needs.
    fn board_view(&mut self, ui: &mut Ui) {
        if self.board.players.is_empty() {
            empty(ui, &self.status, self.stopped.as_deref());
            return;
        }
        let height = if ui.available_width() < COMPACT {
            space::ROW_TIGHT
        } else {
            space::ROW
        };
        let selected = self.selected.clone();
        let mut clicked: Option<String> = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(space::MD);
                for (label, tint, players) in board::teams(&self.board) {
                    if players.is_empty() {
                        continue;
                    }
                    board::team_heading(ui, label, tint, &players);
                    board::headings(ui, ui.available_width());
                    for player in players {
                        let style = RowStyle {
                            team: tint,
                            selected: player.puuid.is_some() && player.puuid == selected,
                            height,
                        };
                        if board::row(ui, player, &style).clicked() {
                            clicked.clone_from(&player.puuid);
                        }
                    }
                    ui.add_space(space::XL);
                }
            });
        if clicked.is_some() {
            self.selected = clicked;
        }
    }
}

impl App for Overseer {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        self.pump();

        let chrome = egui::Frame::NONE.fill(colour::BG);
        Panel::top("header")
            .exact_size(space::XXL + space::MD)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| self.header(ui));
        Panel::bottom("footer")
            .exact_size(space::XL + space::SM)
            .frame(chrome)
            .show(ui, |ui| footer(ui, self.boards));

        let width = ui.available_width();
        if width >= COMPACT {
            let panel_width = if width >= WIDE {
                PANEL_WIDE
            } else {
                PANEL_NARROW
            };
            Panel::right("detail")
                .exact_size(panel_width)
                .frame(
                    egui::Frame::NONE
                        .fill(colour::BG_RAISED)
                        .stroke(egui::Stroke::new(1.0, colour::LINE)),
                )
                .show(ui, |ui| {
                    ui.add_space(space::MD);
                    panel::show(ui, self.current());
                });
        }
        CentralPanel::default().frame(chrome).show(ui, |ui| {
            self.board_view(ui);
        });
    }
}

/// The footer: the one counter worth keeping while the app is this young.
fn footer(ui: &mut Ui, boards: u64) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().text(
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{boards} boards"),
        Face::Number.at(size::MICRO),
        colour::TEXT_FAINT,
    );
}

/// What to say while there is no board. An empty screen is a place to say
/// what is happening and what to do about it, not a place to say nothing.
fn empty(ui: &mut Ui, status: &Status, stopped: Option<&str>) {
    let (title, detail): (&str, &str) = match (stopped, status) {
        (Some(why), _) => ("The bridge refused this build", why),
        (None, Status::Live) => (
            "Signed in, nothing in progress",
            "Open VALORANT and this fills in by itself.",
        ),
        (None, Status::Connecting(_)) => (
            "Looking for the backend",
            "It writes down its port once it is listening.",
        ),
        (None, Status::Lost(why)) => ("Not connected", why),
    };
    ui.vertical_centered(|ui| {
        ui.add_space(space::XXL * 2.0);
        ui.label(
            RichText::new(title)
                .color(colour::TEXT_STRONG)
                .font(Face::Display.at(size::DISPLAY)),
        );
        ui.add_space(space::SM);
        ui.label(
            RichText::new(detail)
                .color(colour::TEXT_FAINT)
                .font(Face::Body.at(size::BODY)),
        );
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

/// The board and the panel, drawn the way the window draws them, for the
/// snapshot test. One door, so the test cannot drift away from the app.
#[cfg(test)]
pub(crate) fn snapshot_view(ui: &mut Ui, board: &Board, selected: Option<&str>) {
    let width = ui.available_width();
    let header = Overseer {
        bridge: Bridge::start(Path::new("."), || {}),
        board: board.clone(),
        status: Status::Live,
        stopped: None,
        selected: None,
        boards: 0,
    };
    Panel::top("header")
        .exact_size(space::XXL + space::MD)
        .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
        .show(ui, |ui| header.header(ui));
    if width >= COMPACT {
        let panel_width = if width >= WIDE {
            PANEL_WIDE
        } else {
            PANEL_NARROW
        };
        Panel::right("detail")
            .exact_size(panel_width)
            .frame(
                egui::Frame::NONE
                    .fill(colour::BG_RAISED)
                    .stroke(egui::Stroke::new(1.0, colour::LINE)),
            )
            .show(ui, |ui| {
                ui.add_space(space::MD);
                let player = selected
                    .and_then(|id| board.players.iter().find(|p| p.name.as_deref() == Some(id)));
                panel::show(ui, player);
            });
    }
    CentralPanel::default()
        .frame(egui::Frame::NONE.fill(colour::BG))
        .show(ui, |ui| {
            if board.players.is_empty() {
                empty(ui, &Status::Connecting(String::new()), None);
                return;
            }
            ui.add_space(space::MD);
            let height = if width < COMPACT {
                space::ROW_TIGHT
            } else {
                space::ROW
            };
            for (label, tint, players) in board::teams(board) {
                if players.is_empty() {
                    continue;
                }
                board::team_heading(ui, label, tint, &players);
                board::headings(ui, ui.available_width());
                for player in players {
                    let style = RowStyle {
                        team: tint,
                        selected: player.name.as_deref() == selected,
                        height,
                    };
                    board::row(ui, player, &style);
                }
                ui.add_space(space::XL);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::connecting_text;

    #[test]
    fn the_first_attempt_has_no_detail_to_show() {
        assert_eq!(connecting_text(""), "connecting");
        assert_eq!(connecting_text("retry 2"), "connecting, retry 2");
    }
}
