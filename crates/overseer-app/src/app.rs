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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::{App, CreationContext, Frame};
use egui::{Align2, CentralPanel, Key, Panel, Rect, RichText, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Bridge, Event, Player, Profile, Status};

use crate::board::{self, GUTTER, Place, Scene};
use crate::career::Career;
use crate::header;
use crate::hotkey::{self, Hotkey};
use crate::notes::{self, Notes};
use crate::overlay;
use crate::settings::{self, Quality, Settings};
use crate::sort::{self, Sort};
use crate::tray::{self, Action, Tray};
use crate::{panel, view};
use overseer_ui::{self, Face, caps_text, colour, motion, size, space};

/// Under this width there is no room for the panel beside the board.
pub(crate) const COMPACT: f32 = 720.0;
/// Above this the panel can afford its full width.
pub(crate) const WIDE: f32 = 1100.0;
/// The panel's width between those two.
const PANEL_NARROW: f32 = 280.0;
/// The panel's width above [`WIDE`].
const PANEL_WIDE: f32 = 340.0;
/// How long the pointer has to rest on a row before its history is asked
/// for. Short enough to feel immediate, long enough that crossing the board
/// asks nothing.
const DWELL: f64 = 0.25;
/// A frame over this many seconds is a frame that missed, at 60Hz with room
/// to spare for the compositor.
const SLOW_FRAME: f32 = 1.0 / 45.0;
/// This many missed frames in a row and the window stops trying to be rich.
const SLOW_STREAK: u32 = 3;
/// This many frames inside the budget, after a drop, and it tries again.
/// Long enough that a busy moment cannot make the tier flicker.
const FAST_STREAK: u32 = 600;
/// How long to let the window settle before believing anything its frame
/// clock says.
const WARMUP: f64 = 1.5;

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
    /// Which screen is drawn, which lags the one above by half a fade.
    screen_showing: Screen,
    /// How many boards have arrived, which is the cheapest proof that the
    /// socket is alive and the frame on screen is not stale.
    boards: u64,
    /// Consecutive frames over budget, for the automatic quality drop.
    slow: u32,
    /// Set when the window dropped its own quality, so it can say so.
    dropped: bool,
    /// Consecutive frames inside budget, for earning the tier back.
    fast: u32,
    /// How the board is ordered, if a heading has been clicked.
    sort: Sort,
    /// What has been typed into the search box.
    filter: String,
    /// Set when a key asked for the search box, so the next frame can hand
    /// it the keyboard.
    focus_search: bool,
    /// Whether the search row is open. It used to be a permanent row under
    /// the masthead, a full-width empty field on every frame of every match
    /// for the handful of times anybody searches.
    searching: bool,
    /// What you have written about the accounts you have met.
    notes: Notes,
    /// The one key combination the whole machine listens for, or why not.
    hotkey: Result<Hotkey, String>,
    /// The icon beside the clock, or why there isn't one.
    tray: Result<Tray, String>,
    /// Set when the window has been closed to the tray rather than quit.
    hidden: bool,
    /// The selected player's history, or where the request for it has got to.
    career: Career,
    /// The account the pointer is over, which the panel follows without a
    /// click. Scanning five enemies should cost five glances, not five
    /// clicks and five ways back.
    hovered: Option<String>,
    /// When the pointer arrived on it, so a history is only fetched for an
    /// account somebody actually stopped on.
    hovered_at: f64,
    /// Who was on the last board, and when that last changed.
    ///
    /// The roster rather than the board: a board arrives every second and
    /// the people on it change once a match. Animating the second would
    /// make every number on screen twitch for ever.
    roster: Vec<String>,
    /// When the roster last changed, which is when the rows start landing.
    roster_at: f64,
    /// How tall the overlay's contents actually came out last time they
    /// were drawn, so the window can be exactly that tall.
    overlay_drew: Option<f32>,
    /// Who the panel is currently faded in on, which lags the pointer by
    /// half the length of the fade.
    panel_showing: Option<String>,
    /// Frames the bridge sent that this build could not read, and the last
    /// reason. Counted rather than ignored: a board that will not parse
    /// looks exactly like no match in progress, and that cost an evening
    /// once already.
    unreadable: (u64, Option<String>),
    /// Set by the tray's Quit, so the close it sends is not mistaken for the
    /// window's own close button and put back in the tray.
    quitting: bool,
    /// Set when the last move through the board came from the keyboard.
    ///
    /// The pointer wins over the selection everywhere else, and without this
    /// a key press with the pointer resting on a row did nothing visible: the
    /// selection moved and the panel stayed on the row under the pointer.
    /// Cleared the moment the pointer moves again.
    keyboard_owns: bool,
    /// Whether the detail panel was on screen last frame, which decides
    /// whether a history is worth asking for at all.
    panel_visible: bool,
    /// Every history already fetched for somebody in this lobby.
    ///
    /// Moving the pointer off a row and back is not a reason to ask the
    /// backend again, and each ask is a match history call to Riot. Cleared
    /// when the lobby changes, so it never holds more than ten.
    histories: HashMap<String, Box<Profile>>,
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
            screen_showing: Screen::Board,
            boards: 0,
            slow: 0,
            dropped: false,
            fast: 0,
            sort: Sort::default(),
            filter: String::new(),
            focus_search: false,
            searching: false,
            notes: notes::load(root),
            // Both of these own a window of their own, and Windows delivers
            // their messages to the queue of the thread that made them. This
            // runs on the thread with the message loop; nowhere else would
            // ever hear from either of them.
            hotkey: report("hotkey", hotkey::start(waker(&cc.egui_ctx))),
            tray: report("tray", tray::start(root, waker(&cc.egui_ctx))),
            hidden: false,
            career: Career::default(),
            hovered: None,
            hovered_at: 0.0,
            roster: Vec::new(),
            roster_at: 0.0,
            overlay_drew: None,
            panel_showing: None,
            unreadable: (0, None),
            quitting: false,
            keyboard_owns: false,
            panel_visible: false,
            histories: HashMap::new(),
        }
    }

    /// Takes everything queued since the last frame, from all three of the
    /// things that can wake this window up.
    fn pump(&mut self, ctx: &egui::Context) {
        // The hotkey is pressed in the middle of a round. It switches the
        // overlay and does nothing else: it used to show the board window as
        // well, and give it the keyboard when the overlay went off, which is
        // this app taking the focus away from the game mid-fight. Getting the
        // window back is the tray's job.
        let pressed = self.hotkey.as_ref().map_or(0, |k| k.presses().count());
        if pressed % 2 == 1 {
            let on = !self.settings.overlay;
            self.set_overlay(on);
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
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        for event in self.bridge.drain() {
            match event {
                Event::Status(status) => {
                    // A socket that has just come back cannot answer a
                    // question asked down the one before it.
                    if status == Status::Live {
                        self.career.reconnected();
                    }
                    self.status = status;
                }
                Event::Answer { id, result } => {
                    self.career.answered(id, result, &mut self.histories);
                }
                Event::Unreadable(why) => {
                    self.unreadable.0 = self.unreadable.0.saturating_add(1);
                    self.unreadable.1 = Some(why);
                }
                Event::Board(board) => {
                    self.board = *board;
                    self.boards = self.boards.saturating_add(1);
                    self.keep_selection();
                    let roster: Vec<String> = self
                        .board
                        .players
                        .iter()
                        .filter_map(|p| p.puuid.clone())
                        .collect();
                    if roster != self.roster {
                        self.roster = roster;
                        self.roster_at = ctx.input(|i| i.time);
                        self.histories.clear();
                        // A new lobby opens on the account most worth
                        // reading, so the common case costs no input at all.
                        if let Some(first) = self.first_flagged_enemy() {
                            self.selected = Some(first);
                        }
                    }
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

    /// Who the panel is about: whoever the pointer is over, or failing that
    /// whoever was last clicked.
    ///
    /// Hover wins, and that is the whole point. The board answers "who is
    /// here" and the panel answers "what about them", and needing a click
    /// between the two questions is what makes reading five enemies in
    /// agent select take longer than agent select lasts.
    fn current(&self) -> Option<&Player> {
        let id = self.hovered.as_ref().or(self.selected.as_ref())?;
        self.board
            .players
            .iter()
            .find(|p| p.puuid.as_ref() == Some(id))
    }

    /// Moves to the next account worth a look, wrapping round.
    ///
    /// The whole app exists to answer "which of these five should I worry
    /// about", and this is that question as a single keystroke. Given a
    /// side it stays on that side, which is what the chip in the team band
    /// does when it is clicked.
    fn next_flagged(&mut self, team: Option<&str>) {
        let flagged: Vec<String> = self
            .visible_order()
            .into_iter()
            .filter(|id| {
                self.board.players.iter().any(|p| {
                    p.puuid.as_deref() == Some(id.as_str())
                        && p.smurf
                        && team.is_none_or(|t| p.team.as_deref() == Some(t))
                })
            })
            .collect();
        let Some(first) = flagged.first() else { return };
        let at = self
            .selected
            .as_deref()
            .and_then(|id| flagged.iter().position(|f| f == id));
        let next = at.map_or(first, |i| flagged.get(i + 1).unwrap_or(first));
        self.selected = Some(next.clone());
        // The pointer wins over the selection everywhere else in this app,
        // so a jump has to take the pointer's job away or nothing appears
        // to happen, and keep it away until the pointer moves.
        self.hovered = None;
        self.keyboard_owns = true;
    }

    /// Whose history to ask for.
    ///
    /// The panel follows the pointer instantly because everything on it came
    /// with the board and costs nothing. A history is a request, so it
    /// follows only once the pointer has settled: dragging across ten rows
    /// on the way to the eleventh should not ask the backend eleven
    /// questions.
    fn career_subject(&self, now: f64) -> Option<&str> {
        match self.hovered.as_deref() {
            Some(id) if now - self.hovered_at >= DWELL => Some(id),
            Some(_) => None,
            None => self.selected.as_deref(),
        }
    }

    /// Keys, which are the fastest way through a board and cost nothing.
    fn keys(&mut self, ui: &Ui) {
        // Nothing here fires while the search box has the keyboard: a
        // person typing a name is not asking to change screens.
        let typing = ui.memory(egui::Memory::focused).is_some();
        let (up, down, settings, escape, find, overlay, worth, copy) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Comma),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Slash) || (i.modifiers.command && i.key_pressed(Key::F)),
                i.key_pressed(Key::O),
                i.key_pressed(Key::W),
                i.modifiers.command && i.key_pressed(Key::C),
            )
        });
        if escape {
            // One key, two jobs, in the order somebody expects: clear what
            // you typed, and then leave the screen you are on.
            if self.filter.is_empty() {
                self.searching = false;
                self.screen = Screen::Board;
            } else {
                self.filter.clear();
            }
            ui.memory_mut(egui::Memory::stop_text_input);
            return;
        }
        if typing {
            return;
        }
        if find {
            self.focus_search = true;
            self.searching = true;
            self.screen = Screen::Board;
            return;
        }
        if overlay {
            self.set_overlay(!self.settings.overlay);
        }
        if worth {
            self.next_flagged(None);
            self.keyboard_owns = true;
        }
        // The one thing anybody wants to do with a name in this window that
        // this window cannot do: look it up somewhere else.
        if copy && let Some(player) = self.current() {
            let name = player.display_name().to_owned();
            ui.ctx().copy_text(name);
        }
        if settings {
            self.screen = if self.screen == Screen::Settings {
                Screen::Board
            } else {
                Screen::Settings
            };
        }
        if !(up || down) {
            return;
        }
        // The order on screen, not the order the backend sent: enemies
        // first, sorted by whatever heading was clicked, and without anybody
        // the search has hidden. Walking the backend's order stepped through
        // people who were not visible and could not get from one block to
        // the other.
        let order = self.visible_order();
        if order.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|id| order.iter().position(|o| o == id));
        let next = match (at, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(order.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.selected = order.get(next).cloned();
        self.keyboard_owns = true;
    }

    /// Every account on screen, in the order they are drawn.
    ///
    /// One definition, so the keys and the board cannot disagree about what
    /// comes after what.
    fn visible_order(&self) -> Vec<String> {
        let mut order = Vec::with_capacity(self.board.players.len());
        for (_side, team) in board::teams(&self.board, self.settings.enemies_first) {
            let mut players = self.board.team(&team);
            sort::apply(&mut players, &self.sort);
            players.retain(|p| sort::matches(p, &self.filter));
            order.extend(players.iter().filter_map(|p| p.puuid.clone()));
        }
        order
    }

    /// The first enemy worth a look, in screen order.
    fn first_flagged_enemy(&self) -> Option<String> {
        let ours = self.board.self_team.as_deref();
        self.visible_order().into_iter().find(|id| {
            self.board.players.iter().any(|p| {
                p.puuid.as_deref() == Some(id.as_str()) && p.smurf && p.team.as_deref() != ours
            })
        })
    }

    /// Why there is no board, when the plain answer would be misleading.
    ///
    /// A build that cannot read what the backend is sending is connected,
    /// signed in, and showing nothing. Saying "nothing in progress" there is
    /// the wrong sentence, and it is the sentence that hid this for months.
    fn reason(&self) -> Option<&str> {
        self.stopped.as_deref().or_else(|| {
            (self.boards == 0)
                .then_some(self.unreadable.1.as_deref())
                .flatten()
        })
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
        if self.quitting || self.tray.is_err() || !ctx.input(|i| i.viewport().close_requested()) {
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
    fn watch_frames(&mut self, ctx: &egui::Context, frame: &Frame) {
        if self.settings.quality != Quality::Auto {
            return;
        }
        // The first second is not evidence. Starting up means building a
        // font atlas, compiling a shader and creating a swapchain, and the
        // frames that do all that miss every budget there is.
        //
        // And the measure is how long the last frame took to build, not the
        // time since the one before it. This window sleeps between boards,
        // so the gap between two frames is usually a second of nothing, and
        // judging on that called every idle second a slow frame.
        let since = ctx.input(|i| i.time);
        let Some(dt) = frame.info().cpu_usage else {
            return;
        };
        if since < WARMUP {
            return;
        }
        if dt > SLOW_FRAME {
            self.slow = self.slow.saturating_add(1);
            self.fast = 0;
            if self.slow >= SLOW_STREAK {
                self.dropped = true;
            }
            return;
        }
        self.slow = 0;
        // And a machine that was busy for a moment is not a slow machine.
        // A long run of frames inside the budget earns the tier back, and
        // the run is long enough that nothing can oscillate.
        self.fast = self.fast.saturating_add(1);
        if self.dropped && self.fast >= FAST_STREAK {
            self.dropped = false;
            self.fast = 0;
        }
    }

    /// The masthead: the match as a scorebug, and the connection.
    fn header(&self, ui: &mut Ui) {
        header::draw(
            ui,
            &header::Masthead {
                board: &self.board,
                light: self.connection(),
                still: self.quality() == Quality::Efficient,
            },
        );
    }

    /// The connection light: a radius, a colour, and the reason behind it.
    fn connection(&self) -> (f32, egui::Color32, String) {
        match (&self.stopped, &self.status) {
            (Some(why), _) | (None, Status::Lost(why)) => (3.0, colour::ENEMY, why.clone()),
            (None, Status::Live) => (3.0, colour::ALLY, "live".to_owned()),
            (None, Status::Connecting(detail)) => (2.0, colour::WARN, connecting_text(detail)),
        }
    }

    /// The detail panel down the right: one surface, lifted off the board.
    fn detail(&mut self, ui: &mut Ui, width: f32) {
        let panel_width = if width >= WIDE {
            PANEL_WIDE
        } else {
            PANEL_NARROW
        };
        Panel::right("detail")
            .exact_size(panel_width)
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let all = ui.max_rect();
                ui.painter().add(egui::Shape::gradient_rect(
                    all,
                    egui::Direction::TopDown,
                    [colour::BG_RAISED, colour::BG],
                ));
                ui.painter()
                    .vline(all.left(), all.y_range(), (1.0, colour::VOID));
                // The panel is a column standing beside the board rather
                // than a region of the same sheet, so its near edge is lit:
                // one dark stroke and one light one, the same cut the rows
                // are separated by, turned on its side.
                ui.painter().vline(
                    all.left() + 1.0,
                    all.y_range(),
                    (1.0, colour::TEXT_STRONG.gamma_multiply(0.07)),
                );
                // The panel changes subject whenever the pointer crosses a
                // row, which on the way down a roster is five times in a
                // second. Snapping through five people reads as flicker.
                // This fades the one on screen out, swaps, and fades the
                // next one in, which reads as turning a page.
                let wanted = self.current().and_then(|p| p.puuid.clone());
                let settled = ui.ctx().animate_value_with_time(
                    egui::Id::new("panel-subject"),
                    f32::from(wanted == self.panel_showing),
                    self.pace(motion::QUICK) / 2.0,
                );
                if settled <= 0.02 && wanted != self.panel_showing {
                    self.panel_showing = wanted;
                }
                ui.multiply_opacity(0.08 + 0.92 * settled);
                ui.add_space(space::MD);
                // Lent to the panel rather than borrowed from self, because
                // the panel edits the notes while reading the player, and
                // both live on this struct.
                let mut lent = std::mem::take(&mut self.notes);
                let showing = self.panel_showing.as_ref().and_then(|id| {
                    self.board
                        .players
                        .iter()
                        .find(|p| p.puuid.as_ref() == Some(id))
                });
                if panel::show(ui, showing, &mut lent, &self.career) {
                    notes::save(&self.root, &lent);
                }
                self.notes = lent;
            });
    }

    /// Every switch there is, and what could not be switched on.
    fn settings_screen(&mut self, ui: &mut Ui, chrome: egui::Frame, turning: f32) {
        // All of this is read out before the screen borrows the settings it
        // sits beside. Failing quietly would leave somebody pressing a key
        // that does nothing with no way to find out why.
        let quality = self.quality();
        let dropped = self.dropped;
        let no_hotkey = self.hotkey.as_ref().err().cloned();
        let no_tray = self.tray.as_ref().err().cloned();
        let trouble = view::Trouble {
            hotkey: no_hotkey.as_deref(),
            tray: no_tray.as_deref(),
        };
        let mut changed = false;
        CentralPanel::default().frame(chrome).show(ui, |ui| {
            ui.multiply_opacity(0.10 + 0.90 * turning);
            changed = view::settings(ui, &mut self.settings, quality, dropped, trouble);
        });
        if changed {
            settings::save(&self.root, &self.settings);
        }
    }

    /// The overlay's whole contents: the board, or one line saying there
    /// is no board.
    ///
    /// The windowed empty state is three lines of reassurance in the middle
    /// of a large rectangle, which is right for a window somebody opened on
    /// purpose and wrong for something sitting over a game.
    fn overlay_view(&self, ui: &mut Ui) -> f32 {
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
            return space::ROW + space::MD;
        }
        // No foot in the overlay: the session belongs in a window somebody
        // is looking at, and the overlay is exactly as tall as its rows.
        self.rows(ui, Place::Overlay).drew
    }

    /// The board, both teams, with the headings each side needs.
    fn board_view(&mut self, ui: &mut Ui) {
        let touched = self.rows(ui, Place::Window);
        let typing = ui.memory(egui::Memory::focused).is_some();
        // Read a frame late, which nobody can see: the panel is drawn before
        // the board, so what the pointer was on last frame is what the panel
        // shows this one.
        if !self.keyboard_owns && !typing && touched.hovered != self.hovered {
            self.hovered = touched.hovered;
            self.hovered_at = ui.input(|i| i.time);
        }
        if touched.clicked.is_some() {
            self.selected = touched.clicked;
        }
        if let Some(head) = touched.heading {
            self.sort.clicked(head);
        }
        if let Some(team) = touched.worth {
            self.next_flagged(Some(&team));
        }
    }

    /// Draws the board, or what to say instead of one.
    ///
    /// Read only, so the overlay can call it too: the overlay is its own
    /// window and takes no clicks, and nothing that only draws can be the
    /// reason two windows disagree about who is selected.
    fn rows(&self, ui: &mut Ui, place: Place) -> board::Touched {
        if self.board.players.is_empty() {
            empty(ui, &self.status, self.reason());
            return board::Touched::default();
        }
        let now = ui.input(|i| i.time);
        let scene = Scene {
            board: &self.board,
            sort: &self.sort,
            // The overlay has no search box, so it never inherits the
            // window's: a board over a game that says nobody matches is a
            // board that has quietly stopped working.
            filter: if place == Place::Window {
                &self.filter
            } else {
                ""
            },
            selected: self.selected.as_deref(),
            notes: &self.notes,
            hidden: &self.settings.hidden_columns,
            enemies_first: self.settings.enemies_first,
            place,
            still: self.quality() == Quality::Efficient,
            since: (now - self.roster_at) as f32,
        };
        board::draw(ui, &scene)
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

    /// Everything that has to happen whether or not the window is on
    /// screen: the bridge, the hotkey, the tray and the close button.
    ///
    /// eframe skips `ui` entirely while the window is minimised, and this
    /// all used to live there. So a minimised window stopped hearing the
    /// hotkey and the tray, and a board a second queued up behind it until
    /// somebody restored it.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame) {
        self.pump(ctx);
        self.closing(ctx);
        self.watch_frames(ctx, frame);
        let now = ctx.input(|i| i.time);
        // Only for a panel somebody can see. Every account the pointer
        // rested on used to fetch a history, panel or no panel.
        if !self.panel_visible {
            return;
        }
        // Copied out because the follow needs the bridge and the career at
        // once, and the subject is borrowed from the same struct as both.
        let subject = self.career_subject(now).map(ToOwned::to_owned);
        if let Some(subject) = subject {
            self.career.follow(
                &self.bridge,
                Some(&subject),
                self.status == Status::Live,
                &self.histories,
            );
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        self.keys(ui);
        if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            self.keyboard_owns = false;
        }
        let now = ui.input(|i| i.time);
        // While the pointer is settling, ask for a frame at the moment it
        // will have settled. Without this the history waits for the next
        // thing to happen, which on a still board is nothing.
        if self.hovered.is_some() && now - self.hovered_at < DWELL {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(DWELL));
        }
        // A roster landing is the one animation nothing else asks frames
        // for: the rows are painted from a timestamp rather than from
        // egui's own animator, so the window has to keep itself awake until
        // the last of them is home.
        if board::settling(
            (now - self.roster_at) as f32,
            self.quality() == Quality::Efficient,
        ) {
            ui.ctx().request_repaint();
        }

        if self.settings.overlay {
            // Its own window, drawn before this one so that a frame where
            // the board changed reaches both. Read only: nothing in it takes
            // a click, so nothing in it can change anything.
            let last = self.overlay_drew.unwrap_or(overlay::DESIGNED);
            let mut drew = last;
            {
                let this = &*self;
                overlay::show(ui.ctx(), this.settings.corner, last, |ui| {
                    drew = this.overlay_view(ui);
                });
            }
            // What it asked for against what the board needed. The window
            // follows the board rather than a sum kept by hand, which is the
            // only arrangement that cannot go stale.
            let drew = drew.min(overlay::CEILING);
            if (drew - last).abs() > 0.5 {
                self.overlay_drew = Some(drew);
                ui.ctx().request_repaint();
            }
        }

        let chrome = egui::Frame::NONE.fill(colour::BG);
        Panel::top("header")
            .exact_size(header::HEIGHT)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| self.header(ui));
        Panel::bottom("footer")
            .exact_size(space::XL + space::SM)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| self.footer(ui));

        if self.screen == Screen::Board && (self.searching || !self.filter.is_empty()) {
            Panel::top("search")
                .exact_size(space::ROW + space::MD)
                .frame(egui::Frame::NONE.fill(colour::BG))
                .show(ui, |ui| self.search(ui));
        }

        let turning = ui.ctx().animate_value_with_time(
            egui::Id::new("screen"),
            f32::from(self.screen == self.screen_showing),
            self.pace(motion::QUICK) / 2.0,
        );
        if turning <= 0.02 && self.screen != self.screen_showing {
            self.screen_showing = self.screen;
        }
        if self.screen_showing == Screen::Settings {
            self.panel_visible = false;
            self.settings_screen(ui, chrome, turning);
            return;
        }

        let width = ui.available_width();
        self.panel_visible = width >= COMPACT && self.settings.panel;
        if self.panel_visible {
            self.detail(ui, width);
        }
        CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                // The ground under the board falls off towards the bottom.
                // Ten rows never reach the bottom of a window, and a flat
                // rectangle under the empty half is the single largest area
                // of undesigned colour in the app.
                let all = ui.max_rect();
                ui.painter().add(egui::Shape::gradient_rect(
                    all,
                    egui::Direction::TopDown,
                    [colour::BG, colour::VOID],
                ));
                ui.multiply_opacity(0.10 + 0.90 * turning);
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
                RichText::new("FIND")
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
            // A field is a hole in the surface rather than a plate on it, so
            // the light lands on the far side of it: one hairline inside the
            // bottom edge and it reads as cut in rather than drawn on. It
            // goes on after the widget because the widget paints its own
            // ground, and it sits on the edge where no glyph reaches.
            ui.painter().hline(
                response.rect.x_range(),
                response.rect.bottom() - 1.0,
                (1.0, colour::TEXT_STRONG.gamma_multiply(0.07)),
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
    ///
    /// Keys as keycaps rather than as prose. A line of bracketed letters
    /// is something to read; a row of caps is something to recognise, and
    /// the difference matters on a bar nobody should spend time on.
    fn footer(&self, ui: &mut Ui) {
        let (rect, _response) = ui.allocate_exact_size(
            vec2(ui.available_width(), space::XL + space::SM),
            Sense::hover(),
        );
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter().clone();
        painter.add(egui::Shape::gradient_rect(
            rect,
            egui::Direction::TopDown,
            [colour::BG_RAISED, colour::BG_INSET],
        ));
        painter.hline(
            rect.x_range(),
            rect.top(),
            (1.0, colour::VOID.gamma_multiply(0.6)),
        );
        painter.hline(
            rect.x_range(),
            rect.top() + 1.0,
            (1.0, colour::TEXT_STRONG.gamma_multiply(0.08)),
        );

        // What the window is spending is drawn first, because the hints are
        // the half that can be dropped. At four hundred and sixty points the
        // two halves used to meet in the middle and print over each other,
        // which read as a rendering fault rather than as a narrow window.
        let (bad, _why) = &self.unreadable;
        let mut right = rect.right() - GUTTER;
        for (text, tint) in [
            (self.quality().label().to_owned(), colour::TEXT_FAINT),
            (format!("{} boards", self.boards), colour::TEXT_FAINT),
            (
                if *bad == 0 {
                    String::new()
                } else {
                    format!("{bad} unreadable")
                },
                colour::WARN,
            ),
        ] {
            if text.is_empty() {
                continue;
            }
            let drawn = caps_text(
                &painter,
                pos2(right, rect.center().y),
                Align2::RIGHT_CENTER,
                &text,
                Face::Display.at(size::MICRO),
                tint,
            );
            right = drawn.left() - space::XL;
        }

        let mut x = rect.left() + GUTTER;
        for (key, what) in [
            ("/", "find"),
            (",", "settings"),
            ("w", "worth a look"),
            ("o", "overlay"),
            ("\u{2191}\u{2193}", "pick"),
        ] {
            let font = Face::Display.at(size::MICRO);
            let wide = overseer_ui::keycap_width(&painter, key)
                + space::SM
                + overseer_ui::caps_width(&painter, what, font.clone());
            if x + wide > right {
                break;
            }
            x = overseer_ui::keycap(&painter, pos2(x, rect.center().y), key).right() + space::SM;
            let after = caps_text(
                &painter,
                pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                what,
                font,
                colour::TEXT_FAINT,
            );
            x = after.right() + space::XL;
        }
    }
}

/// What to say while there is no board.
///
/// An empty screen is a place to say what is happening and what to do about
/// it, not a place to say nothing. It is also the screen this window spends
/// most of its life on, so nothing on it moves: a pulse here would wake the
/// compositor every frame for as long as somebody is sitting in the game's
/// menus, and costing a player frames while they are not even in a match is
/// the one thing this app has promised not to do.
fn empty(ui: &mut Ui, status: &Status, trouble: Option<&str>) {
    let (title, detail, reached): (&str, &str, usize) = match (trouble, status) {
        (Some(why), _) => ("This build cannot read the bridge", why, 0),
        (None, Status::Live) => (
            "Signed in, nothing in progress",
            "Open VALORANT and this fills in by itself.",
            2,
        ),
        (None, Status::Connecting(_)) => (
            "Looking for the backend",
            "It writes down its port once it is listening.",
            0,
        ),
        (None, Status::Lost(why)) => ("Not connected", why, 0),
    };
    let broken = trouble.is_some() || matches!(status, Status::Lost(_));
    let room = ui.available_rect_before_wrap();
    if room.height() < 210.0 {
        plain(ui, title, detail);
        return;
    }
    let (rect, _response) = ui.allocate_exact_size(room.size(), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let width = 420.0_f32.min(rect.width() - space::XXL * 2.0);
    // Laid out before the plate is drawn, because the plate is as tall as
    // its sentence. A box built to a number that happens to fit today is a
    // box that crops the first message longer than the one it was measured
    // against, and the longest of these is whatever the bridge last refused
    // to do.
    let mut job = egui::text::LayoutJob::simple(
        detail.to_owned(),
        Face::Body.at(size::BODY),
        colour::TEXT_DIM,
        width - space::XL * 2.0,
    );
    job.wrap.max_rows = 3;
    job.wrap.overflow_character = Some('\u{2026}');
    let sentence = painter.layout_job(job);
    let plate = Rect::from_center_size(
        pos2(
            rect.center().x,
            rect.top() + (rect.height() * 0.44).max(120.0),
        ),
        vec2(width, 50.0 + sentence.size().y + space::XL + 48.0),
    );
    board::paint::slab(&painter, plate, colour::BG_RAISED, 0.0, false);
    let accent = if broken { colour::ENEMY } else { colour::ALLY };
    painter.rect_filled(
        Rect::from_min_size(plate.min, vec2(4.0, plate.height())),
        0,
        accent,
    );
    words(&painter, plate, title, sentence);
    chain(&painter, plate, reached, broken);
}

/// The words on the plate: the headline, and the sentence already laid
/// out to decide how tall the plate had to be.
fn words(
    painter: &egui::Painter,
    plate: Rect,
    title: &str,
    sentence: std::sync::Arc<egui::Galley>,
) {
    let _title = caps_text(
        painter,
        pos2(plate.left() + space::XL, plate.top() + 28.0),
        Align2::LEFT_CENTER,
        title,
        Face::Heavy.at(24.0),
        colour::TEXT_STRONG,
    );
    painter.galley(
        pos2(plate.left() + space::XL, plate.top() + 48.0),
        sentence,
        colour::TEXT_DIM,
    );
    painter.hline(
        plate.left() + space::XL..=plate.right() - space::XL,
        plate.bottom() - 48.0,
        (1.0, colour::LINE),
    );
}

/// The three things that have to happen before there is anything to show,
/// and which of them have.
///
/// Every one of them can fail on its own and the failures look identical
/// from here: an empty window. Naming them turns "nothing is happening" into
/// a place in a sequence, which is the difference between waiting and being
/// stuck.
fn chain(painter: &egui::Painter, plate: Rect, reached: usize, broken: bool) {
    let middle = plate.bottom() - 22.0;
    let mut x = plate.left() + space::XL;
    for (step, name) in ["backend", "riot", "match"].into_iter().enumerate() {
        let tint = match (step < reached, broken && step == 0) {
            (_, true) => colour::ENEMY,
            (true, _) => colour::ALLY,
            (false, _) => colour::TEXT_FAINT,
        };
        let pip = Rect::from_center_size(pos2(x + 5.0, middle), vec2(10.0, 14.0));
        painter.add(board::paint::slant(
            pip,
            true,
            true,
            if step < reached {
                tint
            } else {
                tint.gamma_multiply(0.35)
            },
        ));
        let after = caps_text(
            painter,
            pos2(x + space::XL, middle),
            Align2::LEFT_CENTER,
            name,
            Face::Display.at(size::MICRO),
            if step < reached {
                colour::TEXT
            } else {
                colour::TEXT_FAINT
            },
        );
        x = after.right() + space::LG;
        if step < 2 {
            painter.hline(x..=x + space::LG, middle, (1.0, colour::LINE));
            x += space::LG + space::LG;
        }
    }
}

/// The same words with no furniture, for a window too short to hold any.
fn plain(ui: &mut Ui, title: &str, detail: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(space::XXL);
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
pub(crate) fn snapshot_empty(ui: &mut Ui) {
    empty(ui, &Status::Live, None);
}

/// The window's chrome, drawn for the snapshot test without a window behind
/// it: the title bar, the search row and the footer.
#[cfg(test)]
pub(crate) fn snapshot_chrome(ui: &mut Ui, board: &Board, boards: u64) {
    let shown = Overseer {
        bridge: Bridge::start(Path::new("."), || {}),
        root: PathBuf::new(),
        settings: Settings::default(),
        board: board.clone(),
        status: Status::Live,
        stopped: None,
        selected: None,
        screen: Screen::Board,
        screen_showing: Screen::Board,
        boards,
        slow: 0,
        dropped: false,
        fast: 0,
        sort: Sort::default(),
        filter: String::new(),
        focus_search: false,
        searching: false,
        notes: Notes::default(),
        // Neither in a test: one would take a key combination off the
        // machine and the other would put an icon beside the clock.
        hotkey: Err(String::new()),
        tray: Err(String::new()),
        hidden: false,
        career: Career::default(),
        hovered: None,
        hovered_at: 0.0,
        roster: Vec::new(),
        roster_at: 0.0,
        overlay_drew: None,
        panel_showing: None,
        unreadable: (0, None),
        quitting: false,
        keyboard_owns: false,
        panel_visible: false,
        histories: HashMap::new(),
    };
    Panel::top("header")
        .exact_size(header::HEIGHT)
        .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
        .show(ui, |ui| shown.header(ui));
    Panel::bottom("footer")
        .exact_size(space::XL + space::SM)
        .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
        .show(ui, |ui| shown.footer(ui));
    // No search row: the window only has one while somebody is searching,
    // and a picture of a row the window does not draw is a picture of
    // nothing.
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
