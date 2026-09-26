//! The window: what is on screen, and what the window itself is doing.
//!
//! Three layouts out of one set of parts. Under `COMPACT` the board is alone
//! and sheds columns; above it the detail panel takes the right; above `WIDE`
//! the panel gets room to breathe. There is no separate compact build,
//! because two builds diverge and a person dragging a window edge should not
//! watch the app turn into a different app.
//!
//! The idle cost is the part worth reading carefully. egui repaints when
//! something asks it to, and the things asking here are the bridge thread,
//! which wakes the UI when a board arrives, and an animation while it runs.
//! There is no timer and no polling, so a window nobody is touching settles
//! to no frames at all.

use std::path::{Path, PathBuf};

use eframe::{App, CreationContext, Frame};
use egui::{Align2, CentralPanel, Key, Panel, RichText, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Bridge, Event, Player, Status};

use crate::board::{self, Pace, RowStyle};
use crate::hotkey::{self, Hotkey};
use crate::notes::{self, Notes};
use crate::overlay;
use crate::settings::{self, Quality, Settings};
use crate::sort::{self, Sort};
use crate::tray::{self, Action, Tray};
use crate::{panel, view};
use overseer_ui::{self, Face, colour, label_text, motion, size, space};

/// Under this width there is no room for the panel beside the board.
pub(crate) const COMPACT: f32 = 720.0;
/// Above this the panel can afford its full width.
pub(crate) const WIDE: f32 = 1100.0;
/// The panel's width between those two.
const PANEL_NARROW: f32 = 280.0;
/// The panel's width above [`WIDE`].
const PANEL_WIDE: f32 = 340.0;
/// A frame over this many seconds is a frame that missed, at 60Hz with room
/// to spare for the compositor.
const SLOW_FRAME: f32 = 1.0 / 45.0;
/// This many missed frames in a row and the window stops trying to be rich.
const SLOW_STREAK: u32 = 3;

/// The window's state.
pub(crate) struct Overseer {
    bridge: Bridge,
    root: PathBuf,
    settings: Settings,
    board: Board,
    status: Status,
    /// Set when the bridge has given up, which is a different thing from
    /// being disconnected: a bad token will not fix itself, so the window
    /// says so rather than showing a spinner for ever.
    stopped: Option<String>,
    /// Who the panel is about. Kept by account id rather than by position,
    /// because the backend reorders the board between frames.
    selected: Option<String>,
    /// Which screen is showing.
    screen: Screen,
    /// How many boards have arrived, which is the cheapest proof that the
    /// socket is alive and the frame on screen is not stale.
    boards: u64,
    /// Consecutive frames over budget, for the automatic quality drop.
    slow: u32,
    /// Set when the window dropped its own quality, so it can say so.
    dropped: bool,
    /// How the board is ordered, if a heading has been clicked.
    sort: Sort,
    /// What has been typed into the search box.
    filter: String,
    /// Set when a key asked for the search box, so the next frame can hand
    /// it the keyboard.
    focus_search: bool,
    /// What you have written about the accounts you have met.
    notes: Notes,
    /// The one key combination the whole machine listens for, or why not.
    hotkey: Result<Hotkey, String>,
    /// The icon beside the clock, or why there isn't one.
    tray: Result<Tray, String>,
    /// Set when the window has been closed to the tray rather than quit.
    hidden: bool,
}

/// What the middle of the window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    /// The board.
    Board,
    /// Every switch there is.
    Settings,
}

impl std::fmt::Debug for Overseer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Overseer")
            .field("status", &self.status)
            .field("players", &self.board.players.len())
            .field("boards", &self.boards)
            .field("screen", &self.screen)
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
        overseer_ui::install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx
            .set_style_of(egui::Theme::Dark, overseer_ui::style());
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
            root: root.to_path_buf(),
            settings: settings::load(root),
            board: Board::default(),
            status: Status::Connecting(String::new()),
            stopped: None,
            selected: None,
            screen: Screen::Board,
            boards: 0,
            slow: 0,
            dropped: false,
            sort: Sort::default(),
            filter: String::new(),
            focus_search: false,
            notes: notes::load(root),
            // Both of these own a window of their own, and Windows delivers
            // their messages to the queue of the thread that made them. This
            // runs on the thread with the message loop; nowhere else would
            // ever hear from either of them.
            hotkey: report("hotkey", hotkey::start(waker(&cc.egui_ctx))),
            tray: report("tray", tray::start(root, waker(&cc.egui_ctx))),
            hidden: false,
        }
    }

    /// Takes everything queued since the last frame, from all three of the
    /// things that can wake this window up.
    fn pump(&mut self, ctx: &egui::Context) {
        let pressed = self.hotkey.as_ref().map_or(0, |k| k.presses().count());
        if pressed % 2 == 1 {
            let on = !self.settings.overlay;
            self.set_overlay(on);
            self.reveal(ctx, !on);
        }
        let actions: Vec<Action> = self
            .tray
            .as_ref()
            .map(|t| t.actions().collect())
            .unwrap_or_default();
        for action in actions {
            match action {
                Action::Window => {
                    self.set_overlay(false);
                    self.reveal(ctx, true);
                }
                Action::Overlay => {
                    self.set_overlay(true);
                    self.reveal(ctx, false);
                }
                Action::Quit => {
                    settings::save(&self.root, &self.settings);
                    notes::save(&self.root, &self.notes);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
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

    /// Keys, which are the fastest way through a board and cost nothing.
    fn keys(&mut self, ui: &Ui) {
        // Nothing here fires while the search box has the keyboard: a
        // person typing a name is not asking to change screens.
        let typing = ui.memory(egui::Memory::focused).is_some();
        let (up, down, settings, escape, find, overlay) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Comma),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Slash) || (i.modifiers.command && i.key_pressed(Key::F)),
                i.key_pressed(Key::O),
            )
        });
        if escape {
            // One key, two jobs, in the order somebody expects: clear what
            // you typed, and then leave the screen you are on.
            if self.filter.is_empty() {
                self.screen = Screen::Board;
            } else {
                self.filter.clear();
            }
            ui.memory_mut(egui::Memory::stop_text_input);
            return;
        }
        if find {
            self.focus_search = true;
            self.screen = Screen::Board;
            return;
        }
        if typing {
            return;
        }
        if overlay {
            self.set_overlay(!self.settings.overlay);
        }
        if settings {
            self.screen = if self.screen == Screen::Settings {
                Screen::Board
            } else {
                Screen::Settings
            };
        }
        if !(up || down) || self.board.players.is_empty() {
            return;
        }
        let order: Vec<Option<String>> =
            self.board.players.iter().map(|p| p.puuid.clone()).collect();
        let at = order
            .iter()
            .position(|id| id == &self.selected)
            .unwrap_or(0);
        let next = if down {
            at.saturating_add(1)
        } else {
            at.saturating_sub(1)
        };
        if let Some(id) = order.get(next.min(order.len().saturating_sub(1))) {
            self.selected.clone_from(id);
        }
    }

    /// Puts the window back on screen, and optionally asks for the
    /// keyboard with it.
    ///
    /// The focus is withheld for the overlay: taking the foreground away
    /// from a game mid round is exactly what an overlay is supposed to avoid
    /// doing.
    fn reveal(&mut self, ctx: &egui::Context, focus: bool) {
        self.hidden = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        if focus {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    /// Closing the window puts it in the tray instead of ending the session.
    ///
    /// Only while there is a tray to put it in. Without one this would be a
    /// window that cannot be closed and has nowhere to be reopened from.
    fn closing(&mut self, ctx: &egui::Context) {
        if self.tray.is_err() || !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.hidden = true;
        settings::save(&self.root, &self.settings);
        notes::save(&self.root, &self.notes);
    }

    /// Switches the overlay on or off and remembers the answer.
    ///
    /// Remembered immediately rather than at exit, because the way out of
    /// the overlay is a hotkey pressed in the middle of a game and the way
    /// out of the game is often the power button.
    pub(crate) fn set_overlay(&mut self, on: bool) {
        if self.settings.overlay == on {
            return;
        }
        self.settings.overlay = on;
        // Nothing in the overlay is clickable, so leaving a half finished
        // search or a settings screen behind it would be a trap.
        self.screen = Screen::Board;
        settings::save(&self.root, &self.settings);
    }

    /// How long an animation is allowed to take, given the tier.
    fn pace(&self, base: f32) -> f32 {
        if self.quality() == Quality::Efficient {
            motion::EFFICIENT
        } else {
            base
        }
    }

    /// The tier in force, once auto has made up its mind.
    const fn quality(&self) -> Quality {
        match self.settings.quality {
            Quality::Auto if self.dropped => Quality::Efficient,
            Quality::Auto => Quality::Rich,
            other => other,
        }
    }

    /// Watches the frame clock, and gives up on the rich tier if the machine
    /// cannot keep up.
    ///
    /// Three consecutive frames over budget rather than one, because one slow
    /// frame is a window being dragged onto another monitor. The drop is said
    /// out loud in the settings screen, and choosing a tier by hand ends it.
    fn watch_frames(&mut self, ui: &Ui) {
        if self.settings.quality != Quality::Auto || self.dropped {
            return;
        }
        let dt = ui.input(|i| i.unstable_dt);
        if dt > SLOW_FRAME {
            self.slow = self.slow.saturating_add(1);
            if self.slow >= SLOW_STREAK {
                self.dropped = true;
            }
        } else {
            self.slow = 0;
        }
    }

    /// The title bar: who is playing what, and whether we can see it.
    fn header(&self, ui: &mut Ui) {
        let height = space::XXL + space::MD;
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter().clone();
        let middle = rect.center().y;

        let x = self.header_match(&painter, middle, rect.left() + space::LG);
        self.header_progress(&painter, middle, x);
        self.header_light(&painter, rect, middle);
        painter.hline(rect.x_range(), rect.bottom() - 1.0, (1.0, colour::LINE));
    }

    /// The wordmark, the state, the map, the queue and the side, left to
    /// right in the order somebody reads them.
    fn header_match(&self, painter: &egui::Painter, middle: f32, start: f32) -> f32 {
        let mut x = start;
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
            let after = painter.text(
                pos2(x, middle),
                Align2::LEFT_CENTER,
                label_text(side),
                Face::Display.at(size::LABEL),
                tint,
            );
            x = after.right() + space::LG;
        }
        x
    }

    /// The score, or how far through agent select the lobby is: whichever of
    /// the two is the number that changes while you are watching.
    fn header_progress(&self, painter: &egui::Painter, middle: f32, x: f32) {
        if let Some(score) = self.board.score.as_ref() {
            let (ally, enemy) = (score.ally.unwrap_or(0), score.enemy.unwrap_or(0));
            let after = painter.text(
                pos2(x, middle),
                Align2::LEFT_CENTER,
                format!("{ally}"),
                Face::Number.at(size::TITLE),
                colour::ALLY,
            );
            let after = painter.text(
                pos2(after.right() + space::SM, middle),
                Align2::LEFT_CENTER,
                format!("{enemy}"),
                Face::Number.at(size::TITLE),
                colour::ENEMY,
            );
            if let Some(round) = score.round {
                painter.text(
                    pos2(after.right() + space::MD, middle),
                    Align2::LEFT_CENTER,
                    format!("round {round}"),
                    Face::Body.at(size::MICRO),
                    colour::TEXT_FAINT,
                );
            }
        } else if let Some(lock) = self.board.lock_progress.as_ref() {
            painter.text(
                pos2(x, middle),
                Align2::LEFT_CENTER,
                format!(
                    "{}/{} locked",
                    lock.locked.unwrap_or(0),
                    lock.total.unwrap_or(0)
                ),
                Face::Body.at(size::MICRO),
                colour::WARN,
            );
        }
    }

    /// Whether the bridge is answering: on the right, where a status light
    /// belongs, and quiet enough to ignore while it is green.
    fn header_light(&self, painter: &egui::Painter, rect: egui::Rect, middle: f32) {
        let (radius, tint, text) = self.connection();
        let drawn = painter.text(
            pos2(rect.right() - space::LG, middle),
            Align2::RIGHT_CENTER,
            text,
            Face::Body.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        painter.circle_filled(pos2(drawn.left() - space::MD, middle), radius, tint);
    }

    /// The connection light: a radius, a colour, and the reason behind it.
    fn connection(&self) -> (f32, egui::Color32, String) {
        match (&self.stopped, &self.status) {
            (Some(why), _) | (None, Status::Lost(why)) => (3.0, colour::ENEMY, why.clone()),
            (None, Status::Live) => (3.0, colour::ALLY, "live".to_owned()),
            (None, Status::Connecting(detail)) => (2.0, colour::WARN, connecting_text(detail)),
        }
    }

    /// The overlay's whole contents: the board, or one line saying there
    /// is no board.
    ///
    /// The windowed empty state is three lines of reassurance in the middle
    /// of a large rectangle, which is right for a window somebody opened on
    /// purpose and wrong for something sitting over a game.
    fn overlay_view(&self, ui: &mut Ui) {
        if self.board.players.is_empty() {
            let (rect, _response) =
                ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
            if ui.is_rect_visible(rect) {
                let (_radius, tint, text) = self.connection();
                ui.painter().text(
                    pos2(rect.left() + space::LG, rect.center().y),
                    Align2::LEFT_CENTER,
                    format!("overseer {text}"),
                    Face::Body.at(size::MICRO),
                    tint,
                );
            }
            return;
        }
        drop(self.rows(ui));
    }

    /// The board, both teams, with the headings each side needs.
    fn board_view(&mut self, ui: &mut Ui) {
        let (clicked, heading) = self.rows(ui);
        if clicked.is_some() {
            self.selected = clicked;
        }
        if let Some(head) = heading {
            self.sort.clicked(head);
        }
    }

    /// Draws every row, and reports what was clicked.
    ///
    /// Read only, so that the overlay can call it too: the overlay is its
    /// own window and takes no clicks, and nothing that only draws can be
    /// the reason two windows disagree about what is selected.
    fn rows(&self, ui: &mut Ui) -> (Option<String>, Option<&'static str>) {
        if self.board.players.is_empty() {
            empty(ui, &self.status, self.stopped.as_deref());
            return (None, None);
        }
        let height = if ui.available_width() < COMPACT {
            space::ROW_TIGHT
        } else {
            space::ROW
        };
        let selected = self.selected.clone();
        let hidden = self.settings.hidden_columns.clone();
        let pace = Pace {
            hover: self.pace(motion::INSTANT),
            select: self.pace(motion::QUICK),
        };
        let sort = self.sort.clone();
        let filter = self.filter.clone();
        let mut clicked: Option<String> = None;
        let mut heading: Option<&'static str> = None;
        let mut shown = 0_usize;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(space::MD);
                for (label, tint, team) in board::teams(&self.board) {
                    let mut players = self.board.team(&team);
                    if players.is_empty() {
                        continue;
                    }
                    sort::apply(&mut players, &sort);
                    players.retain(|p| sort::matches(p, &filter));
                    // Its own id scope: both blocks have a column called
                    // K/D, and without this they ask egui for the same
                    // widget id and it says so, loudly, across the board.
                    ui.push_id(&team, |ui| {
                        board::team_heading(ui, label, tint, &self.board, &team);
                        if let Some(head) =
                            board::headings(ui, ui.available_width(), &hidden, &sort)
                        {
                            heading = Some(head);
                        }
                        for player in players {
                            shown += 1;
                            let style = RowStyle {
                                team: tint,
                                selected: player.puuid.is_some() && player.puuid == selected,
                                height,
                                pace,
                                noted: player.puuid.as_deref().is_some_and(|id| self.notes.has(id)),
                            };
                            if board::row(ui, player, &style, &hidden).clicked() {
                                clicked.clone_from(&player.puuid);
                            }
                        }
                    });
                    ui.add_space(space::XL);
                }
                if shown == 0 {
                    nobody(ui, &filter);
                }
            });
        (clicked, heading)
    }
}

impl App for Overseer {
    /// Called by eframe on the way out, and every so often before that.
    ///
    /// The notes are written when a box loses the keyboard, which covers
    /// clicking anywhere else. It does not cover closing the window with the
    /// caret still in the box, and losing a sentence somebody typed about a
    /// person is the one failure in this app that would actually sting.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        notes::save(&self.root, &self.notes);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        self.pump(ui.ctx());
        self.closing(ui.ctx());
        self.keys(ui);
        self.watch_frames(ui);

        if self.settings.overlay {
            // Its own window, drawn before this one so that a frame where
            // the board changed reaches both. Read only: nothing in it takes
            // a click, so nothing in it can change anything.
            let this = &*self;
            overlay::show(
                ui.ctx(),
                this.settings.corner,
                this.board.players.len(),
                |ui| this.overlay_view(ui),
            );
        }

        let chrome = egui::Frame::NONE.fill(colour::BG);
        Panel::top("header")
            .exact_size(space::XXL + space::MD)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| self.header(ui));
        Panel::bottom("footer")
            .exact_size(space::XL + space::SM)
            .frame(chrome)
            .show(ui, |ui| self.footer(ui));

        if self.screen == Screen::Board {
            Panel::top("search")
                .exact_size(space::ROW + space::MD)
                .frame(egui::Frame::NONE.fill(colour::BG))
                .show(ui, |ui| self.search(ui));
        }

        if self.screen == Screen::Settings {
            // Both read out before the screen borrows the settings, because
            // the tier is worked out from the settings it is about to edit.
            let quality = self.quality();
            let dropped = self.dropped;
            let mut changed = false;
            // Copied out before the screen borrows the settings it sits
            // beside. Failing quietly would leave somebody pressing a key
            // that does nothing with no way to find out why.
            let no_hotkey = self.hotkey.as_ref().err().cloned();
            let no_tray = self.tray.as_ref().err().cloned();
            let trouble = view::Trouble {
                hotkey: no_hotkey.as_deref(),
                tray: no_tray.as_deref(),
            };
            CentralPanel::default().frame(chrome).show(ui, |ui| {
                changed = view::settings(ui, &mut self.settings, quality, dropped, trouble);
            });
            if changed {
                settings::save(&self.root, &self.settings);
            }
            return;
        }

        let width = ui.available_width();
        if width >= COMPACT && self.settings.panel {
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
                    // Lent to the panel rather than borrowed from self, because
                    // the panel edits the notes while reading the player, and
                    // both live on this struct.
                    let mut lent = std::mem::take(&mut self.notes);
                    if panel::show(ui, self.current(), &mut lent) {
                        notes::save(&self.root, &lent);
                    }
                    self.notes = lent;
                });
        }
        CentralPanel::default().frame(chrome).show(ui, |ui| {
            self.board_view(ui);
        });
    }
}

impl Overseer {
    /// The search box, and what the board is currently ordered by.
    ///
    /// A real text field rather than something painted: selection, the
    /// caret, backspace and every other keyboard convention are exactly what
    /// hand painting gets wrong, and nobody thanks an app for reinventing
    /// them badly.
    fn search(&mut self, ui: &mut Ui) {
        let full = ui.available_width();
        ui.horizontal(|ui| {
            ui.add_space(space::LG);
            ui.label(
                RichText::new(label_text("find"))
                    .color(colour::TEXT_FAINT)
                    .font(Face::Display.at(size::MICRO)),
            );
            let width = (full * 0.3).clamp(140.0, 320.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .desired_width(width)
                    .font(Face::Body.at(size::BODY))
                    .hint_text(
                        RichText::new("a name or an agent")
                            .color(colour::TEXT_FAINT)
                            .font(Face::Body.at(size::BODY)),
                    ),
            );
            if self.focus_search {
                response.request_focus();
                self.focus_search = false;
            }
            if !self.filter.is_empty() {
                ui.label(
                    RichText::new("escape clears it")
                        .color(colour::TEXT_FAINT)
                        .font(Face::Body.at(size::MICRO)),
                );
            }
            if let Some(column) = self.sort.column.as_deref() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(space::LG);
                    ui.label(
                        RichText::new(format!("sorted by {column}"))
                            .color(colour::TEXT_FAINT)
                            .font(Face::Body.at(size::MICRO)),
                    );
                });
            }
        });
    }

    /// The footer: what to press, and what the window is spending.
    fn footer(&self, ui: &mut Ui) {
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter().clone();
        painter.text(
            pos2(rect.left() + space::LG, rect.center().y),
            Align2::LEFT_CENTER,
            "[/] find   [,] settings   [up] [down] pick   click a heading to sort",
            Face::Body.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        let quality = self.quality().label();
        painter.text(
            pos2(rect.right() - space::LG, rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{} boards   {quality}", self.boards),
            Face::Number.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

/// What to say when the filter has hidden everybody.
fn nobody(ui: &mut Ui, filter: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(space::XXL);
        ui.label(
            RichText::new(format!("Nobody here matches \"{}\"", filter.trim()))
                .color(colour::TEXT_DIM)
                .font(Face::Body.at(size::BODY)),
        );
    });
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

/// Prints whether one of the two machine wide things started, and hands it
/// back untouched.
///
/// Both are registrations with Windows that another program can refuse, and
/// both are invisible when they fail. The settings screen says so too; this
/// is the line you can read without opening anything, beside the one saying
/// which adapter the window got.
fn report<T>(what: &str, outcome: Result<T, String>) -> Result<T, String> {
    match outcome.as_ref() {
        Ok(_) => println!("{what} ok"),
        Err(why) => println!("{what} unavailable: {why}"),
    }
    outcome
}

/// A closure that asks egui for a frame.
///
/// The bridge, the hotkey and the tray all run on their own schedule, and
/// this window is asleep whenever nothing is happening. This is how each of
/// them reaches it.
fn waker(ctx: &egui::Context) -> impl Fn() + Send + Sync + 'static + use<> {
    let ctx = ctx.clone();
    move || ctx.request_repaint()
}

/// The header's connection text, which has no detail on the first attempt.
fn connecting_text(detail: &str) -> String {
    if detail.is_empty() {
        "connecting".to_owned()
    } else {
        format!("connecting, {detail}")
    }
}

/// The header, drawn for the snapshot test without a window behind it.
///
/// The header is the only place the accent colour appears, so leaving it out
/// of the image would mean the loudest thing in the app was the one thing
/// nothing checked.
#[cfg(test)]
pub(crate) fn snapshot_header(ui: &mut Ui, board: &Board) {
    let shown = Overseer {
        bridge: Bridge::start(Path::new("."), || {}),
        root: PathBuf::new(),
        settings: Settings::default(),
        board: board.clone(),
        status: Status::Live,
        stopped: None,
        selected: None,
        screen: Screen::Board,
        boards: 0,
        slow: 0,
        dropped: false,
        sort: Sort::default(),
        filter: String::new(),
        focus_search: false,
        notes: Notes::default(),
        // Neither in a test: one would take a key combination off the
        // machine and the other would put an icon beside the clock.
        hotkey: Err(String::new()),
        tray: Err(String::new()),
        hidden: false,
    };
    Panel::top("header")
        .exact_size(space::XXL + space::MD)
        .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
        .show(ui, |ui| shown.header(ui));
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
