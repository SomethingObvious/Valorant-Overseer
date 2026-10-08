//! The window, and what it does between frames.
//!
//! Under `COMPACT` the board is alone and sheds columns, above it the detail
//! panel takes the right, and above `WIDE` the panel widens. Nothing here
//! polls. egui repaints only when the bridge, the hotkey, the tray or a
//! running animation asks, so a window nobody touches draws nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::{App, CreationContext, Frame};
use egui::{Key, Rect, Sense, Ui};
use overseer_core::{Board, Bridge, Event, Player, Profile, Status};

use crate::board::{self, Side};
use crate::career::Career;
use crate::header;
use crate::history::History;
use crate::hotkey::{self, Failure, Hotkey};
use crate::lineups::Lineups;
use crate::notes::{self, Notes};
use crate::offline::Offline;
use crate::panel;
use crate::settings::{self, Quality, Settings};
use crate::sort::Sort;
use crate::tray::{self, Action, Tray};
use overseer_ui::{self, colour, motion};

mod foot;
mod frame;
mod screens;
mod waiting;

/// Under this width there is no room for the panel beside the board.
pub(crate) const COMPACT: f32 = 720.0;
/// Above this the panel can afford its full width.
pub(crate) const WIDE: f32 = 1100.0;
/// The panel's width between those two.
const PANEL_NARROW: f32 = 280.0;
/// The panel's width above [`WIDE`], at least.
const PANEL_WIDE: f32 = 340.0;
/// The most the panel takes: two columns at a comfortable width.
const PANEL_MOST: f32 = 680.0;
/// The least a dragged panel leaves the board, which is what the board has
/// beside the narrowest panel in the narrowest window that has one.
const BOARD_LEAST: f32 = COMPACT - PANEL_NARROW;
/// How long a press of the hotkey shows the overlay for, in seconds.
const PEEK: f64 = 15.0;
/// Seconds the pointer rests on a row before its history is asked for, so
/// crossing the board asks nothing.
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
/// Seconds the window takes to fade in when it opens.
const APPEAR: f32 = 0.45;
/// Seconds the window has to keep one size and scale before it fades in.
const STEADY: f64 = 0.15;
/// The longest the fade waits for that, in seconds from the first frame.
const STEADY_AT_MOST: f64 = 1.0;
/// Seconds the waiting plate's words take to fade in after they change.
const RESTATE: f32 = 0.3;

/// How the frames are keeping to their budget, behind the automatic quality
/// drop and the frame trace.
#[derive(Debug, Default)]
struct Budget {
    /// Consecutive frames over budget.
    slow: u32,
    /// Consecutive frames inside budget, for earning the tier back.
    fast: u32,
    /// Set when the window dropped its own quality, so it can say so.
    dropped: bool,
    /// Set by `OVERSEER_TRACE_FRAMES`, to print what each frame cost.
    trace: bool,
}

/// The search row.
#[derive(Debug, Default)]
struct Search {
    /// What has been typed into it.
    filter: String,
    /// Set when a key asked for the box, so the next frame can hand it the
    /// keyboard.
    focus: bool,
    /// Whether the row is open. It only shows while somebody searches.
    open: bool,
}

/// The window's state.
pub(crate) struct Overseer {
    bridge: Bridge,
    root: PathBuf,
    settings: Settings,
    board: Board,
    status: Status,
    /// Why the bridge gave up, if it has. Unlike a disconnect, a bad token
    /// never fixes itself, so the window says so.
    stopped: Option<String>,
    /// Who the panel is about. Kept by account id rather than by position,
    /// because the backend reorders the board between frames.
    selected: Option<String>,
    /// Which screen is showing.
    screen: Screen,
    /// Which screen is drawn, which lags the one above by half a fade.
    screen_showing: Screen,
    /// How many boards have arrived, the cheapest proof that the socket is
    /// alive and the frame on screen isn't stale.
    boards: u64,
    /// How the frames are keeping to their budget, for the automatic
    /// quality drop.
    budget: Budget,
    /// When the window started fading in, once it had settled.
    opened_at: Option<f64>,
    /// The window's size and scale, and when the first frame and the last
    /// change to them were, while it settles.
    settling: Option<((egui::Vec2, f32), f64, f64)>,
    /// Whether the bridge has ever been live since the window opened.
    ever_live: bool,
    /// How the board is ordered, if a heading has been clicked.
    sort: Sort,
    /// The search row and what is typed in it.
    search: Search,
    /// What you have written about the accounts you have met.
    notes: Notes,
    /// The one key combination the whole machine listens for, or why not.
    hotkey: Result<Hotkey, Failure>,
    /// The icon beside the clock, or why there isn't one.
    tray: Result<Tray, String>,
    /// The selected player's history, or where the request for it has got to.
    career: Career,
    /// Your own past games, for the History screen.
    history: History,
    /// Your lineups and the maps they are on, for the Lineups screen.
    lineups: Lineups,
    /// Offline chat's switch in Settings.
    offline: Offline,
    /// The account under the pointer. The panel follows it without a click,
    /// so reading five enemies takes five glances.
    hovered: Option<String>,
    /// When the pointer arrived on it, so a history is only fetched for an
    /// account somebody actually stopped on.
    hovered_at: f64,
    /// The mouse wheel's last notch, for the speed the next one goes at.
    wheel: Wheel,
    /// The enemy accounts on the last board, sorted. The rows land when this
    /// changes, once a match, and not on every board, which comes each second.
    roster: Vec<String>,
    /// When the roster last changed, which is when the rows start landing.
    roster_at: f64,
    /// How tall the overlay's contents actually came out last time they
    /// were drawn, so the window can be exactly that tall.
    overlay_drew: Option<f32>,
    /// What decides whether the overlay is up, apart from the setting that
    /// lets it show itself.
    shown: Shown,
    /// Who the panel is currently faded in on, which lags the pointer by
    /// half the length of the fade.
    panel_showing: Option<String>,
    /// Frames from the bridge this build could not read, and the last reason.
    /// A board that will not parse otherwise looks exactly like an empty lobby.
    unreadable: (u64, Option<String>),
    /// Raised when a second launch asked this window to show itself.
    knocked: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Set when the keyboard last moved the selection, until the pointer
    /// moves. The pointer otherwise wins, and the panel would stay on the row
    /// under it.
    keyboard_owns: bool,
    /// Whether the detail panel was on screen last frame, which decides
    /// whether a history is worth asking for at all.
    panel_visible: bool,
    /// Histories already fetched in this lobby, because each fetch is a match
    /// history call to Riot. Cleared with the lobby, so it holds ten at most.
    histories: HashMap<String, Box<Profile>>,
    /// A few words for the footer and when they were said, for an action
    /// that would otherwise look like it did nothing.
    said: Option<(String, f64)>,
    /// Where the window last was on screen, for a save made minimized.
    place: Place,
    /// What this PC has, which decides whether it counts as slow.
    machine: crate::machine::Machine,
}

/// How long the footer keeps something it was told to say, in seconds.
const SAID: f64 = 2.5;

/// The key eframe keeps the window's place under.
const WINDOW_KEY: &str = "window";

/// What decides whether the overlay is up, apart from the setting that lets
/// it show itself.
#[derive(Debug, Default)]
struct Shown {
    /// Until when a press of the hotkey keeps it up, on egui's clock.
    peek_until: Option<f64>,
    /// Whether it is showing itself, which it does through agent select.
    by_itself: bool,
    /// The match whose agent select it last showed itself for, so hiding
    /// it by hand keeps it hidden until the next one.
    armed_for: Option<String>,
}

impl Shown {
    /// Whether the overlay is up at `now`: asked for in the last `PEEK`
    /// seconds, or, when it may show itself and there are `players` to
    /// show, in agent select.
    fn up(&self, auto: bool, players: bool, now: f64) -> bool {
        let peeking = self.peek_until.is_some_and(|until| now < until);
        peeking || (auto && players && self.by_itself)
    }

    /// Follows the game's `state` in the match `game`: up by itself once
    /// each agent select, and gone when that ends, as the match loads or
    /// somebody dodges.
    fn follow(&mut self, state: Option<&str>, game: Option<&str>) {
        let game = game.unwrap_or_default();
        if state != Some("PREGAME") {
            self.by_itself = false;
        } else if self.armed_for.as_deref() != Some(game) {
            self.armed_for = Some(game.to_owned());
            self.by_itself = true;
        }
    }
}

/// Where the window last was on screen, as eframe saved it.
#[derive(Debug, Default)]
struct Place {
    /// Whether the window was minimized on the last frame.
    minimized: bool,
    /// eframe's last save of the window while it was on screen.
    kept: Option<String>,
}

impl Place {
    /// Puts back where the window last was on screen when eframe has just
    /// saved it minimized. Windows reports a minimized window as nothing at
    /// -32000, and eframe would open the next launch from that as the
    /// smallest window it can make, in the corner of the main screen.
    fn keep(&mut self, storage: &mut dyn eframe::Storage) {
        if !self.minimized {
            self.kept = storage.get_string(WINDOW_KEY);
        } else if let Some(place) = &self.kept {
            storage.set_string(WINDOW_KEY, place.clone());
        }
    }
}

/// How far in from the window's edge a press resizes it. A window without
/// Windows' frame has no grab area outside its edge, so this one is wide. The
/// right edge is where the scroll bars are, and keeps it narrow for them.
const EDGE: f32 = 8.0;

/// See [`EDGE`].
const EDGE_RIGHT: f32 = 5.0;

/// What the footer's hints do, each the same as its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hint {
    /// Jump to the next account worth a look.
    Worth,
    /// Move the selection down a row.
    Pick,
    /// Open the search.
    Find,
    /// Write about whoever is selected.
    Note,
    /// Copy their Riot ID.
    Copy,
    /// Switch the overlay.
    Overlay,
    /// Open or close the settings.
    Settings,
    /// Open or close your match history.
    History,
    /// Open or close your lineups.
    Lineups,
}

/// How wide the detail panel is in a window this wide. Dragged to a width,
/// it keeps it as far as the board can spare. Otherwise, above `WIDE`,
/// whatever the board's full width leaves over goes to the panel, not to
/// margins, and from `panel::TWO_COLUMNS` it lays out in two columns.
pub(crate) fn panel_width(width: f32, chosen: Option<f32>) -> f32 {
    if let Some(chosen) = chosen {
        let widest = (width - BOARD_LEAST).clamp(PANEL_NARROW, PANEL_MOST);
        return chosen.clamp(PANEL_NARROW, widest);
    }
    if width >= WIDE {
        (width - board::full_width()).clamp(PANEL_WIDE, PANEL_MOST)
    } else {
        PANEL_NARROW
    }
}

/// The detail panel's left edge, which drags to make the panel wider or
/// narrower and fits it to the window again on a double click. True when
/// `chosen` changed for good and wants saving. Run after the board, so
/// nothing on the board takes the press first.
pub(crate) fn panel_grip(ui: &Ui, panel: Rect, width: f32, chosen: &mut Option<f32>) -> bool {
    let grip = Rect::from_x_y_ranges(panel.left() - 3.0..=panel.left() + 5.0, panel.y_range());
    let response = ui
        .interact(grip, egui::Id::new("panel-grip"), Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if response.dragged()
        && let Some(at) = response.interact_pointer_pos()
    {
        *chosen = Some(panel_width(width, Some(panel.right() - at.x)));
    }
    if response.hovered() || response.dragged() {
        ui.painter()
            .vline(panel.left() + 0.5, panel.y_range(), (2.0, colour::TEXT_DIM));
    }
    if response.double_clicked() {
        *chosen = None;
        return true;
    }
    response.drag_stopped()
}

/// Whether the board just went from agent select into the match.
fn match_began(was: Option<&str>, now: Option<&str>) -> bool {
    was == Some("PREGAME") && now == Some("INGAME")
}

/// Whether the window fills the screen.
fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

/// The edge or corner under the pointer, where a press resizes the window.
/// The window draws its own frame, so Windows gives it no border to drag.
fn edge(ctx: &egui::Context) -> Option<egui::ResizeDirection> {
    use egui::ResizeDirection as To;
    if maximized(ctx) {
        return None;
    }
    let (rect, at) = ctx.input(|i| (i.viewport_rect(), i.pointer.hover_pos()));
    let at = at?;
    let (west, east) = (at.x < rect.left() + EDGE, at.x > rect.right() - EDGE_RIGHT);
    let (north, south) = (at.y < rect.top() + EDGE, at.y > rect.bottom() - EDGE);
    Some(match (north, south, west, east) {
        (true, _, true, _) => To::NorthWest,
        (true, _, _, true) => To::NorthEast,
        (_, true, true, _) => To::SouthWest,
        (_, true, _, true) => To::SouthEast,
        (true, ..) => To::North,
        (_, true, ..) => To::South,
        (_, _, true, _) => To::West,
        (_, _, _, true) => To::East,
        _ => return None,
    })
}

/// The pointer that says which way an edge resizes.
const fn resize_cursor(to: egui::ResizeDirection) -> egui::CursorIcon {
    use egui::{CursorIcon, ResizeDirection as To};
    match to {
        To::North | To::South => CursorIcon::ResizeVertical,
        To::East | To::West => CursorIcon::ResizeHorizontal,
        To::NorthWest | To::SouthEast => CursorIcon::ResizeNwSe,
        To::NorthEast | To::SouthWest => CursorIcon::ResizeNeSw,
    }
}

/// What the middle of the window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    /// The board.
    Board,
    /// Every switch there is.
    Settings,
    /// Your past games and their scoreboards.
    History,
    /// Your lineups, map by map.
    Lineups,
}

/// How far the mouse wheel goes, notch by notch.
#[derive(Debug, Default)]
struct Wheel {
    /// When the last notch came, in egui's seconds.
    at: f64,
    /// What quick notches in a row have built up to.
    boost: f32,
}

impl Wheel {
    /// Times egui's forty points a notch.
    const BASE: f32 = 1.6;
    /// Notches closer than this are one flick of the wheel.
    const FLICK: f64 = 0.12;
    /// What each notch in a flick adds.
    const STEP: f32 = 0.15;
    /// The most a flick builds up to, so a quick spin goes only a bit further
    /// than a careful notch.
    const MOST: f32 = 1.6;

    /// The multiplier for a notch arriving at `now`.
    fn notch(&mut self, now: f64) -> f32 {
        self.boost = if now - self.at < Self::FLICK {
            (self.boost + Self::STEP).min(Self::MOST)
        } else {
            1.0
        };
        self.at = now;
        Self::BASE * self.boost
    }
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
    pub(crate) fn new(
        cc: &CreationContext<'_>,
        root: &Path,
        listener: Option<std::net::TcpListener>,
    ) -> Self {
        overseer_ui::install_fonts(&cc.egui_ctx);
        overseer_ui::art::warm(&cc.egui_ctx);
        // Lineup clips play in a child of this window, through Windows' engine.
        if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(cc)
            && let raw_window_handle::RawWindowHandle::Win32(window) = handle.as_raw()
        {
            overseer_native::set_window(window.hwnd.get());
        }
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx
            .set_style_of(egui::Theme::Dark, overseer_ui::style());
        // The adapter the request really got decides whether the game keeps
        // the discrete GPU, and this is the only place it can be known.
        if let Some(state) = cc.wgpu_render_state.as_ref() {
            println!("{}", crate::probe::adapter_line(&state.adapter.get_info()));
        }
        let ctx = cc.egui_ctx.clone();
        let bridge = Bridge::start(root, move || ctx.request_repaint());
        let knocked = std::sync::Arc::<std::sync::atomic::AtomicBool>::default();
        if let Some(listener) = listener {
            crate::instance::listen(
                listener,
                std::sync::Arc::clone(&knocked),
                waker(&cc.egui_ctx),
            );
        }
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
            budget: Budget {
                trace: std::env::var_os("OVERSEER_TRACE_FRAMES").is_some(),
                ..Budget::default()
            },
            opened_at: None,
            settling: None,
            ever_live: false,
            sort: Sort::default(),
            search: Search::default(),
            notes: notes::load(root),
            // Both own a hidden window whose messages go to the thread that
            // made it, and this is the message loop's thread.
            hotkey: report("hotkey", hotkey::start(waker(&cc.egui_ctx))),
            tray: report("tray", tray::start(root, waker(&cc.egui_ctx))),
            career: Career::default(),
            history: History::default(),
            lineups: Lineups::default(),
            offline: Offline::default(),
            hovered: None,
            hovered_at: 0.0,
            wheel: Wheel::default(),
            roster: Vec::new(),
            roster_at: 0.0,
            overlay_drew: None,
            shown: Shown::default(),
            panel_showing: None,
            unreadable: (0, None),
            knocked,
            keyboard_owns: false,
            panel_visible: false,
            histories: HashMap::new(),
            said: None,
            machine: crate::machine::Machine::detect(),
            place: Place {
                minimized: false,
                kept: cc.storage.and_then(|s| s.get_string(WINDOW_KEY)),
            },
        }
    }

    /// Hands an answer to whichever screen asked.
    fn answer(&mut self, id: u64, result: Result<serde_json::Value, String>) {
        if self.history.waiting_on(id) {
            self.history.answered(result);
        } else if self.lineups.waiting_on(id) {
            self.lineups.answered(id, result);
        } else if self.offline.waiting_on(id) {
            self.offline.answered(result);
        } else {
            self.career.answered(id, result, &mut self.histories);
        }
    }

    /// Takes everything the hotkey, the tray, a second launch and the bridge
    /// queued since the last frame.
    fn pump(&mut self, ctx: &egui::Context) {
        // Pressed mid-round, so it only switches the overlay. Showing or
        // focusing the window would take the focus from the game, and the
        // tray is how the window comes back.
        let pressed = self.hotkey.as_ref().map_or(0, |k| k.presses().count());
        if pressed % 2 == 1 {
            self.flip_overlay(ctx.input(|i| i.time));
        }
        self.reach(ctx);
        for event in self.bridge.drain() {
            match event {
                Event::Status(status) => {
                    // A socket that has just come back cannot answer a
                    // question asked down the one before it.
                    if status == Status::Live {
                        self.career.reconnected();
                        self.history.reconnected();
                        self.lineups.reconnected(&self.bridge);
                        self.offline.reconnected();
                        self.ever_live = true;
                    }
                    // Before the first connection, a refused socket is the
                    // backend still starting, often behind the bridge.json the
                    // last one left, and not a connection that was lost.
                    self.status = match status {
                        Status::Lost(_) if !self.ever_live => Status::Connecting(String::new()),
                        other => other,
                    };
                }
                Event::Answer { id, result } => self.answer(id, result),
                Event::Unreadable(why) => {
                    self.unreadable.0 = self.unreadable.0.saturating_add(1);
                    self.unreadable.1 = Some(why);
                }
                Event::Board(board) => {
                    let was = self.board.state.take();
                    let was_on = self.board.map.take();
                    self.board = *board;
                    // A match that starts with Lineups open moves it to the
                    // match's map. Only on a change, so a map picked by hand
                    // stays picked.
                    if let Some(map) = self.board.map.as_deref()
                        && was_on.as_deref() != Some(map)
                    {
                        self.lineups.follow(map);
                    }
                    self.shown
                        .follow(self.board.state.as_deref(), self.board.match_id.as_deref());
                    let began = match_began(was.as_deref(), self.board.state.as_deref());
                    // Only when it isn't in use. Somebody reading it as the
                    // match loads wants it where it is.
                    if self.settings.minimize_in_matches
                        && began
                        && !ctx.input(|i| i.viewport().focused.unwrap_or(false))
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    // A face for every script in the lobby's names, loaded
                    // the first time one turns up.
                    let names: String = self
                        .board
                        .players
                        .iter()
                        .filter_map(|p| p.name.as_deref())
                        .collect();
                    overseer_ui::cover(ctx, &names);
                    self.boards = self.boards.saturating_add(1);
                    self.keep_selection();
                    // Sorted, so a reorder is not a new lobby, and enemies
                    // only, so the rows land once when the other five are
                    // revealed and not again when the game starts.
                    let mut roster: Vec<String> = self
                        .board
                        .players
                        .iter()
                        .filter(|p| board::side_of(&self.board, p) == Side::Enemy)
                        .filter_map(|p| p.puuid.clone())
                        .collect();
                    roster.sort_unstable();
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

    /// Moves the selection to you, or else the first player, when the
    /// selected account has left the board.
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

    /// Who the panel is about: whoever is under the pointer, or else the
    /// selection.
    fn current(&self) -> Option<&Player> {
        let id = self.hovered.as_ref().or(self.selected.as_ref())?;
        self.board
            .players
            .iter()
            .find(|p| p.puuid.as_ref() == Some(id))
    }

    /// Selects the next flagged account in screen order, wrapping round. With
    /// a `team` it stays on that side, which is what the team band's chip does.
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
        let Some(next) = board::next_after(&flagged, self.selected.as_deref()) else {
            return;
        };
        self.selected = Some(next);
        // The pointer otherwise outranks the selection, and the jump would
        // not show until it moved.
        self.hovered = None;
        self.keyboard_owns = true;
    }

    /// Whose history to ask for: the hovered account once the pointer has
    /// rested there, or else the selection. Passing over rows asks nothing.
    fn career_subject(&self, now: f64) -> Option<&str> {
        match self.hovered.as_deref() {
            Some(id) if now - self.hovered_at >= DWELL => Some(id),
            Some(_) => None,
            None => self.selected.as_deref(),
        }
    }

    /// The keyboard shortcuts.
    fn keys(&mut self, ui: &Ui) {
        // Only Escape works while a text box has the keyboard, so typing a
        // name never changes screens.
        let typing = ui.memory(egui::Memory::focused).is_some();
        let note = ui.input(|i| i.key_pressed(Key::N));
        let (up, down, settings, history, lineups, escape, find, overlay, worth, copy) =
            ui.input(|i| {
                (
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::Comma),
                    i.key_pressed(Key::H),
                    i.key_pressed(Key::L),
                    i.key_pressed(Key::Escape),
                    i.key_pressed(Key::Slash) || (i.modifiers.command && i.key_pressed(Key::F)),
                    i.key_pressed(Key::O),
                    i.key_pressed(Key::W),
                    // egui turns Ctrl C into a Copy event and does not report
                    // the key.
                    i.events.iter().any(|e| matches!(e, egui::Event::Copy))
                        || (i.modifiers.command && i.key_pressed(Key::C)),
                )
            });
        // A clip shown full screen takes Escape for itself.
        if escape && self.screen == Screen::Lineups && self.lineups.fullscreen() {
            return;
        }
        if escape {
            // Clears the search first, then leaves the screen.
            if self.search.filter.is_empty() {
                self.search.open = false;
                self.screen = Screen::Board;
            } else {
                self.search.filter.clear();
            }
            ui.memory_mut(egui::Memory::stop_text_input);
            return;
        }
        if typing {
            return;
        }
        let ctx = ui.ctx();
        if find {
            self.perform(ctx, Hint::Find);
            return;
        }
        if overlay {
            self.perform(ctx, Hint::Overlay);
        }
        if note && self.selected.is_some() {
            // The n is still in this frame's input, and the note box about
            // to take the keyboard would type it.
            ctx.input_mut(|i| {
                i.events
                    .retain(|e| !matches!(e, egui::Event::Text(t) if t.eq_ignore_ascii_case("n")));
            });
            self.perform(ctx, Hint::Note);
            return;
        }
        for (pressed, hint) in [
            (worth, Hint::Worth),
            (copy, Hint::Copy),
            (settings, Hint::Settings),
            (history, Hint::History),
            (lineups, Hint::Lineups),
        ] {
            if pressed {
                self.perform(ctx, hint);
            }
        }
        // On Lineups the arrows step through the lineups on the map instead.
        if (up || down) && self.screen != Screen::Lineups {
            self.step(down);
        }
    }

    /// Does what a key or a footer hint asks. Both come here, so a click
    /// on the hint does exactly what its key does.
    fn perform(&mut self, ctx: &egui::Context, hint: Hint) {
        match hint {
            Hint::Worth => {
                if !self.board.players.iter().any(|p| p.smurf) {
                    self.say(ctx, "Nobody Worth a Look in This Lobby");
                }
                self.next_flagged(None);
            }
            Hint::Pick if self.screen == Screen::Lineups => self.lineups.step_down(),
            Hint::Pick => self.step(true),
            Hint::Find => {
                self.search.focus = true;
                self.search.open = true;
                self.screen = Screen::Board;
            }
            Hint::Note => {
                if let Some(id) = self.selected.clone() {
                    self.screen = Screen::Board;
                    panel::write_note(ctx, &id);
                }
            }
            // For looking the name up somewhere else.
            Hint::Copy => {
                if let Some(name) = self.current().and_then(|p| p.name.clone()) {
                    panel::copy(ctx, name);
                }
            }
            Hint::Overlay => self.flip_overlay(ctx.input(|i| i.time)),
            Hint::Settings => {
                self.screen = if self.screen == Screen::Settings {
                    Screen::Board
                } else {
                    Screen::Settings
                };
            }
            Hint::History => {
                if self.screen == Screen::History {
                    self.screen = Screen::Board;
                } else {
                    // Asked each time it opens, so a game that just
                    // finished is there. The ones before it are on disk.
                    self.screen = Screen::History;
                    self.history.ask(
                        &self.bridge,
                        self.settings.history_games,
                        self.status == Status::Live,
                    );
                }
            }
            Hint::Lineups => {
                if self.screen == Screen::Lineups {
                    self.screen = Screen::Board;
                } else {
                    // Opens on the map being played, when there is one.
                    self.screen = Screen::Lineups;
                    // The blacked-out figure is cut from the agent you've
                    // played most lately.
                    let main = self
                        .board
                        .players
                        .iter()
                        .find(|p| p.is_self)
                        .and_then(|p| p.top_agents.first())
                        .and_then(|t| t.agent.as_deref());
                    self.lineups.open(
                        &self.bridge,
                        self.status == Status::Live,
                        self.board.map.as_deref(),
                        main,
                    );
                }
            }
        }
    }

    /// Moves the selection a row down, or up.
    fn step(&mut self, down: bool) {
        // The backend's order would step through rows the search has hidden.
        let order = self.visible_order();
        if let Some(next) = board::step(&order, self.selected.as_deref(), down) {
            self.selected = Some(next);
            self.keyboard_owns = true;
        }
    }

    /// Puts a few words in the footer for a moment, for an action that would
    /// otherwise look like it did nothing.
    fn say(&mut self, ctx: &egui::Context, words: &str) {
        self.said = Some((words.to_owned(), ctx.input(|i| i.time)));
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAID));
    }

    /// Every account on screen, in the order they are drawn. The keys use it
    /// too, so they always agree with the board.
    fn visible_order(&self) -> Vec<String> {
        board::order(
            &self.board,
            &self.sort,
            &self.search.filter,
            self.settings.enemies_first,
        )
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

    /// Why there is no board, when "nothing in progress" would be wrong: the
    /// bridge stopped, or nothing it sent could be read.
    fn reason(&self) -> Option<&str> {
        self.stopped.as_deref().or_else(|| {
            (self.boards == 0)
                .then_some(self.unreadable.1.as_deref())
                .flatten()
        })
    }

    /// What the tray and a second launch asked of the window.
    fn reach(&mut self, ctx: &egui::Context) {
        let actions: Vec<Action> = self
            .tray
            .as_ref()
            .map(|t| t.actions().collect())
            .unwrap_or_default();
        // A second launch: back on screen and in front, even from the tray.
        if self
            .knocked
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            Self::reveal(ctx, true);
        }
        for action in actions {
            match action {
                Action::Window => {
                    self.hide_overlay();
                    Self::reveal(ctx, true);
                }
                Action::Overlay => {
                    self.peek_overlay(ctx.input(|i| i.time));
                    Self::reveal(ctx, false);
                }
                Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    /// Puts the window back on screen, and takes the keyboard only with
    /// `focus`, which a peek at the overlay mid-game must not.
    fn reveal(ctx: &egui::Context, focus: bool) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        if focus {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    /// Saves everything on the way out when the window is closed. The close
    /// goes ahead, and the backend this window started ends with it.
    fn closing(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        self.lineups.leave();
        settings::save(&self.root, &self.settings);
        notes::save(&self.root, &self.notes);
    }

    /// Hides the overlay if it is up, and otherwise shows it for `PEEK`
    /// seconds. The hotkey, O and the footer's hint all come here.
    pub(crate) fn flip_overlay(&mut self, now: f64) {
        if self.overlay_up(now) {
            self.hide_overlay();
        } else {
            self.peek_overlay(now);
        }
    }

    /// Shows the overlay for `PEEK` seconds.
    fn peek_overlay(&mut self, now: f64) {
        self.shown.peek_until = Some(now + PEEK);
        // The overlay draws the window's board, and a half finished search
        // or a settings screen behind it would be a trap.
        self.screen = Screen::Board;
    }

    /// Hides the overlay until it next shows itself or is asked for.
    pub(super) const fn hide_overlay(&mut self) {
        self.shown.peek_until = None;
        self.shown.by_itself = false;
    }

    /// Whether the overlay is up: asked for in the last `PEEK` seconds, or
    /// showing itself in agent select, unless it was hidden by hand since.
    pub(super) fn overlay_up(&self, now: f64) -> bool {
        self.shown.up(
            self.settings.overlay.auto,
            !self.board.players.is_empty(),
            now,
        )
    }

    /// How long an animation is allowed to take, given the tier.
    fn pace(&self, base: f32) -> f32 {
        if self.quality() == Quality::Efficient {
            motion::EFFICIENT
        } else {
            base
        }
    }

    /// Whether to hold back for a slow PC, as set or as detected.
    fn slow_pc(&self) -> bool {
        match self.settings.pc {
            settings::Pc::Auto => !self.machine.slow_because().is_empty(),
            settings::Pc::Fast => false,
            settings::Pc::Slow => true,
        }
    }

    /// The tier in force, once auto has made up its mind.
    const fn quality(&self) -> Quality {
        match self.settings.quality {
            Quality::Auto if self.budget.dropped => Quality::Efficient,
            Quality::Auto => Quality::Rich,
            other => other,
        }
    }

    /// Under auto, drops the rich tier after `SLOW_STREAK` slow frames in a
    /// row, because one slow frame is just a window dragged to another
    /// monitor. `FAST_STREAK` good frames earn it back.
    fn watch_frames(&mut self, ctx: &egui::Context, frame: &Frame) {
        if self.budget.trace
            && let Some(dt) = frame.info().cpu_usage
        {
            println!(
                "frame {:.2} ms at {:.3} size {:?} scale {}",
                dt * 1000.0,
                ctx.input(|i| i.time),
                ctx.content_rect().size(),
                ctx.pixels_per_point()
            );
        }
        if self.settings.quality != Quality::Auto {
            return;
        }
        // Startup builds a font atlas, a shader and a swapchain, and misses
        // every budget there is. The measure is the time the last frame took
        // to build, not the gap since the one before, because the window
        // sleeps between boards.
        let since = ctx.input(|i| i.time);
        let Some(dt) = frame.info().cpu_usage else {
            return;
        };
        if since < WARMUP {
            return;
        }
        if dt > SLOW_FRAME {
            self.budget.slow = self.budget.slow.saturating_add(1);
            self.budget.fast = 0;
            if self.budget.slow >= SLOW_STREAK {
                self.budget.dropped = true;
            }
            return;
        }
        self.budget.slow = 0;
        // A machine that was busy for a moment is not a slow machine.
        self.budget.fast = self.budget.fast.saturating_add(1);
        if self.budget.dropped && self.budget.fast >= FAST_STREAK {
            self.budget.dropped = false;
            self.budget.fast = 0;
        }
    }

    /// Fades the window in when it opens, rather than letting it pop in a
    /// piece at a time. The fade waits for the window to settle, since it
    /// opens on the main screen and may move to one at another scale, and
    /// the frames before that are drawn at the wrong size.
    fn appear(&mut self, ui: &mut Ui, now: f64) {
        let appear = self.pace(APPEAR);
        if appear <= 0.0 {
            return;
        }
        let shape = (ui.ctx().content_rect().size(), ui.ctx().pixels_per_point());
        if self.opened_at.is_none() {
            let (seen, first, changed) = self.settling.get_or_insert((shape, now, now));
            if *seen != shape {
                *seen = shape;
                *changed = now;
            }
            if now - *changed < STEADY && now - *first < STEADY_AT_MOST {
                ui.multiply_opacity(0.0);
                ui.ctx().request_repaint();
                return;
            }
        }
        let opened = *self.opened_at.get_or_insert(now);
        let open = (now - opened) as f32;
        if open < appear {
            ui.multiply_opacity(motion::eased(open / appear));
            ui.ctx().request_repaint();
        }
    }

    /// The masthead: the match as a scorebug, the connection, and the
    /// window's own buttons. Says which of those was pressed.
    fn header(&self, ui: &mut Ui, edge: bool) -> Option<header::Chrome> {
        header::draw(
            ui,
            &header::Masthead {
                board: &self.board,
                // Just the dot while all is well. The words come back when
                // there is something to say.
                light: match self.connection() {
                    (radius, tint, _fine)
                        if matches!((&self.stopped, &self.status), (None, Status::Live)) =>
                    {
                        (radius, tint, String::new())
                    }
                    light => light,
                },
                still: self.quality() == Quality::Efficient,
                maximized: maximized(ui.ctx()),
                open: match self.screen {
                    Screen::Settings => Some(header::Chrome::Settings),
                    Screen::History => Some(header::Chrome::History),
                    Screen::Lineups => Some(header::Chrome::Lineups),
                    Screen::Board => None,
                },
                edge,
            },
        )
    }

    /// What a button in the masthead asked for.
    fn chrome(&mut self, ctx: &egui::Context, pressed: header::Chrome) {
        use egui::ViewportCommand;
        match pressed {
            header::Chrome::Settings => self.perform(ctx, Hint::Settings),
            header::Chrome::History => self.perform(ctx, Hint::History),
            header::Chrome::Lineups => self.perform(ctx, Hint::Lineups),
            header::Chrome::Minimize => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
            header::Chrome::Maximize => {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized(ctx)));
            }
            header::Chrome::Close => ctx.send_viewport_cmd(ViewportCommand::Close),
        }
    }

    /// The connection light: a radius, a colour, and the reason behind it.
    fn connection(&self) -> (f32, egui::Color32, String) {
        match (&self.stopped, &self.status) {
            (Some(why), _) | (None, Status::Lost(why)) => (3.0, colour::ENEMY, why.clone()),
            (None, Status::Live) => (3.0, colour::ALLY, "Connected".to_owned()),
            (None, Status::Connecting(detail)) => (2.0, colour::WARN, connecting_text(detail)),
        }
    }
}

impl App for Overseer {
    /// Makes each wheel notch go further than egui's forty points. A trackpad
    /// sends points, not lines, and is left alone.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let now = raw_input.time.unwrap_or(0.0);
        for event in &mut raw_input.events {
            if let egui::Event::MouseWheel { unit, delta, .. } = event
                && *unit == egui::MouseWheelUnit::Line
            {
                *delta *= self.wheel.notch(now);
            }
        }
    }

    /// Called by eframe on the way out and every so often before. Notes save
    /// when their box loses the keyboard, and this catches a window closed
    /// with the caret still in one.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        notes::save(&self.root, &self.notes);
        self.place.keep(storage);
    }

    /// Everything that has to run while the window is minimised, when eframe
    /// skips `ui`: the bridge, the hotkey, the tray and the close button.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame) {
        self.place.minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        self.pump(ctx);
        self.closing(ctx);
        self.watch_frames(ctx, frame);
        let now = ctx.input(|i| i.time);
        // Here and not in `ui`, which eframe skips while the window is
        // minimized, and Minimize in Matches minimizes it just as the
        // match loads.
        self.overlay(ctx, now);
        // A history is only worth fetching for a panel somebody can see.
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
        motion::set_still(ui.ctx(), self.quality() == Quality::Efficient);
        // A press on the window's edge resizes it, before anything under the
        // pointer takes the press.
        let edge = edge(ui.ctx());
        if let Some(to) = edge
            && ui.input(|i| i.pointer.primary_pressed())
        {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(to));
        }
        self.window(ui, edge.is_some());
        // Set last, so no widget under the edge can put its own pointer back.
        if let Some(to) = edge {
            ui.ctx().set_cursor_icon(resize_cursor(to));
        }
        overseer_ui::chrome::outline(ui.ctx());
    }
}

/// Prints whether the hotkey or the tray started, and hands the result back.
/// Another program can take either from us without a sign on screen.
fn report<T, E: std::fmt::Display>(what: &str, outcome: Result<T, E>) -> Result<T, E> {
    match outcome.as_ref() {
        Ok(_) => println!("{what} ok"),
        Err(why) => println!("{what} unavailable: {why}"),
    }
    outcome
}

/// A closure that asks egui for a frame, for whatever wakes the window from
/// outside it.
fn waker(ctx: &egui::Context) -> impl Fn() + Send + Sync + 'static + use<> {
    let ctx = ctx.clone();
    move || ctx.request_repaint()
}

/// The header's connection text, which has no detail on the first attempt.
fn connecting_text(detail: &str) -> String {
    if detail.is_empty() {
        "Connecting".to_owned()
    } else {
        format!("Connecting, {detail}")
    }
}

/// The empty state, drawn for the snapshot test.
#[cfg(test)]
pub(crate) fn snapshot_empty(ui: &mut Ui) {
    let board = Board {
        waiting: Some("game".to_owned()),
        ..Board::default()
    };
    // Still, the way the efficient tier draws it, so the picture holds.
    waiting::empty(ui, (&Status::Live, &board), None, motion::EFFICIENT);
}

/// The header and the footer, drawn for the snapshot test without a window
/// behind them.
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
        budget: Budget::default(),
        opened_at: None,
        settling: None,
        ever_live: true,
        sort: Sort::default(),
        search: Search::default(),
        notes: Notes::default(),
        // Neither in a test: one would take a key combination off the
        // machine and the other would put an icon beside the clock.
        hotkey: Err(Failure::Taken),
        tray: Err(String::new()),
        career: Career::default(),
        history: History::default(),
        lineups: Lineups::default(),
        offline: Offline::default(),
        hovered: None,
        hovered_at: 0.0,
        wheel: Wheel::default(),
        roster: Vec::new(),
        roster_at: 0.0,
        overlay_drew: None,
        shown: Shown::default(),
        panel_showing: None,
        unreadable: (0, None),
        knocked: std::sync::Arc::default(),
        keyboard_owns: false,
        panel_visible: false,
        histories: HashMap::new(),
        said: None,
        place: Place::default(),
        machine: crate::machine::Machine {
            memory: None,
            threads: 8,
            card: true,
        },
    };
    let _pressed = egui::Panel::top("header")
        .exact_size(header::HEIGHT)
        .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
        .show(ui, |ui| shown.header(ui, false));
    let _clicked = egui::Panel::bottom("footer")
        .exact_size(overseer_ui::space::XL + overseer_ui::space::SM)
        .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
        .show(ui, |ui| shown.footer(ui));
}

#[cfg(test)]
mod tests {
    use super::{Place, Shown, Wheel, connecting_text, match_began, panel_grip, panel_width};

    /// Up by itself through agent select, once a match and gone when it
    /// loads, never with nobody to show, and for 15 seconds after the
    /// hotkey either way.
    #[test]
    fn the_overlay_comes_up_in_agent_select_and_for_the_hotkey() {
        let mut shown = Shown::default();
        assert!(!shown.up(true, true, 0.0), "nothing set it off");
        shown.follow(Some("PREGAME"), Some("first"));
        assert!(shown.up(true, true, 100.0), "agent select");
        assert!(!shown.up(false, true, 100.0), "the setting is off");
        assert!(!shown.up(true, false, 100.0), "nobody to show yet");
        shown.follow(Some("INGAME"), Some("first"));
        assert!(!shown.up(true, true, 100.0), "the match loaded");
        shown.follow(Some("PREGAME"), Some("second"));
        shown.by_itself = false;
        shown.follow(Some("PREGAME"), Some("second"));
        assert!(
            !shown.up(true, true, 100.0),
            "hidden by hand, it stays hidden"
        );
        shown.follow(Some("MENUS"), None);
        assert!(!shown.up(true, true, 100.0), "a dodge");
        shown.follow(Some("PREGAME"), Some("third"));
        assert!(shown.up(true, true, 100.0), "the next agent select");
        shown.follow(Some("INGAME"), Some("third"));
        shown.peek_until = Some(146.0);
        assert!(shown.up(false, false, 140.0), "the hotkey shows it anyway");
        assert!(!shown.up(true, true, 147.0), "and only for a while");
    }

    /// A store in memory, standing in for eframe's file.
    #[derive(Debug, Default)]
    struct Disk(std::collections::HashMap<String, String>);

    impl eframe::Storage for Disk {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    /// A save while minimized puts back the place the window last had on
    /// screen, and a save on screen is the new place to keep.
    #[test]
    fn a_minimized_window_keeps_its_last_place_on_screen() {
        use eframe::Storage as _;
        let mut disk = Disk::default();
        let mut place = Place::default();
        disk.set_string("window", "maximized".to_owned());
        place.keep(&mut disk);
        place.minimized = true;
        disk.set_string("window", "nothing at -32000".to_owned());
        place.keep(&mut disk);
        assert_eq!(disk.get_string("window").as_deref(), Some("maximized"));
        place.minimized = false;
        disk.set_string("window", "moved".to_owned());
        place.keep(&mut disk);
        assert_eq!(place.kept.as_deref(), Some("moved"));
    }

    /// A careful notch goes the base distance, a quick run of them builds
    /// up to a cap, and a pause starts over.
    #[test]
    fn the_wheel_speeds_up_a_little_and_no_more() {
        let mut wheel = Wheel::default();
        let first = wheel.notch(10.0);
        assert!((first - Wheel::BASE).abs() < 1e-6);
        let mut last = first;
        for i in 1..20 {
            last = wheel.notch(f64::from(i).mul_add(0.05, 10.0));
        }
        assert!(Wheel::BASE.mul_add(-Wheel::MOST, last).abs() < 1e-6);
        assert!((wheel.notch(20.0) - Wheel::BASE).abs() < 1e-6);
    }

    #[test]
    fn the_first_attempt_has_no_detail_to_show() {
        assert_eq!(connecting_text(""), "Connecting");
        assert_eq!(connecting_text("retry 2"), "Connecting, retry 2");
    }

    /// A dragged panel keeps its width while the board can spare it, and
    /// never goes under the narrowest panel or over the widest.
    #[test]
    fn a_dragged_panel_keeps_its_width_and_leaves_the_board_room() {
        let same = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(same(panel_width(1600.0, Some(500.0)), 500.0));
        // 860 wide leaves the board 440, so the panel stops at 420.
        assert!(same(panel_width(860.0, Some(500.0)), 420.0));
        assert!(same(panel_width(1600.0, Some(100.0)), 280.0));
        assert!(same(panel_width(3000.0, Some(2000.0)), 680.0));
        assert!(same(panel_width(1000.0, None), 280.0));
    }

    /// Only the step from agent select into the match minimizes the window,
    /// not opening it mid-match or going back to the menus.
    #[test]
    fn the_window_steps_aside_only_as_the_match_begins() {
        assert!(match_began(Some("PREGAME"), Some("INGAME")));
        assert!(!match_began(None, Some("INGAME")));
        assert!(!match_began(Some("INGAME"), Some("INGAME")));
        assert!(!match_began(Some("INGAME"), Some("MENUS")));
    }

    /// A real drag on the panel's edge, in a 1200 wide window whose panel
    /// starts 340 wide: 100 points left makes it 440, and a double click
    /// gives the width back to the window.
    #[test]
    fn the_panel_edge_drags() {
        let panel = egui::Rect::from_min_max(egui::pos2(860.0, 0.0), egui::pos2(1200.0, 600.0));
        // A frame a sixtieth of a second, so two clicks fit in a double click.
        let mut shot = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 600.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, state: &mut (Option<f32>, bool)| {
                    let at = state.0.map_or(panel, |w| {
                        egui::Rect::from_min_max(egui::pos2(1200.0 - w, 0.0), panel.max)
                    });
                    state.1 |= panel_grip(ui, at, 1200.0, &mut state.0);
                },
                (None, false),
            );
        let y = 300.0;
        shot.hover_at(egui::pos2(861.0, y));
        shot.run();
        shot.drag_at(egui::pos2(861.0, y));
        shot.run();
        for x in [840.0, 800.0, 761.0] {
            shot.hover_at(egui::pos2(x, y));
            shot.run();
        }
        shot.drop_at(egui::pos2(761.0, y));
        shot.run();
        let (chosen, saved) = *shot.state();
        assert!(
            chosen.is_some_and(|w| (w - 439.0).abs() < 0.5),
            "the panel is {chosen:?} wide"
        );
        assert!(saved, "letting go didn't ask for a save");

        let edge = egui::pos2(762.0, y);
        shot.hover_at(edge);
        shot.run();
        for pressed in [true, false, true, false] {
            shot.event(egui::Event::PointerButton {
                pos: edge,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            shot.step();
        }
        assert_eq!(
            shot.state().0,
            None,
            "a double click kept the dragged width"
        );
    }
}
