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
use crate::hotkey::{self, Listener};
use crate::lineups::Lineups;
use crate::notes::{self, About, Notes};
use crate::offline::Offline;
use crate::panel;
use crate::settings::{self, Quality, Settings};
use crate::sort::Sort;
use crate::tray::{self, Action, Tray};
use crate::update::VERSION;
use overseer_ui::{self, colour, motion};

mod foot;
mod frame;
mod screens;
mod waiting;

pub(crate) const COMPACT: f32 = 720.0;
pub(crate) const WIDE: f32 = 1100.0;
const PANEL_NARROW: f32 = 280.0;
const PANEL_WIDE: f32 = 340.0;
const PANEL_MOST: f32 = 680.0;
const BOARD_LEAST: f32 = COMPACT - PANEL_NARROW;
const PEEK: f64 = 15.0;
const BACK_UP: f64 = 8.0;
const KEEP: f64 = 3600.0;
const RESTARTS: usize = 3;
const RESTART_SPAN: f64 = 600.0;
const DWELL: f64 = 0.25;
/// A frame over this many seconds is a frame that missed, at 60Hz with room
/// to spare for the compositor.
const SLOW_FRAME: f32 = 1.0 / 45.0;
const SLOW_STREAK: u32 = 3;
const FAST_STREAK: u32 = 600;
/// How long to let the window settle before believing anything its frame
/// clock says.
const WARMUP: f64 = 1.5;
const APPEAR: f32 = 0.45;
const STEADY: f64 = 0.15;
const STEADY_AT_MOST: f64 = 1.0;
const RESTATE: f32 = 0.3;

/// How the frames are keeping to their budget, behind the automatic quality
/// drop and the frame trace.
#[derive(Debug, Default)]
struct Budget {
    slow: u32,
    fast: u32,
    dropped: bool,
    trace: bool,
    calm: Calm,
}

impl Budget {
    fn starting(software: bool) -> Self {
        Self {
            trace: std::env::var_os("OVERSEER_TRACE_FRAMES").is_some(),
            calm: if software {
                Calm::Software
            } else if overseer_native::animations() {
                Calm::No
            } else {
                Calm::Animations
            },
            ..Self::default()
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Calm {
    #[default]
    No,
    /// Windows' animation effects are off, which auto takes as a wish for no
    /// motion.
    Animations,
    Software,
}

pub(crate) struct Overseer {
    bridge: Bridge,
    root: PathBuf,
    settings: Settings,
    board: Board,
    tags_given: Vec<Vec<overseer_core::AutoTag>>,
    status: Status,
    /// Why the bridge gave up, if it has. Unlike a disconnect, a bad token
    /// never fixes itself, so the window says so.
    stopped: Option<String>,
    broken: Option<String>,
    selected: Option<String>,
    screen: Screen,
    screen_showing: Screen,
    boards: u64,
    budget: Budget,
    opened_at: Option<f64>,
    settling: Option<((egui::Vec2, f32), f64, f64)>,
    ever_live: bool,
    lost_at: Option<(f64, bool)>,
    /// When a board with players on it was last read live, in seconds
    /// since 1970, for how old a board held through a gap is.
    read_at: f64,
    /// When the window started the backend again lately, in egui's clock.
    restarts: Vec<f64>,
    empty: Empty,
    sort: Sort,
    notes: Notes,
    hotkey: Option<Result<Listener, String>>,
    /// Until when the window's own shortcuts are left alone, after a press
    /// was caught for the overlay's button, since the window sees that
    /// press too and might see it after the catch is over.
    keys_quiet_until: f64,
    /// The icon beside the clock, or why there isn't one.
    tray: Result<Tray, String>,
    career: Career,
    history: History,
    lineups: Lineups,
    offline: Offline,
    update: crate::update::Update,
    art_asks: Vec<u64>,
    hovered: Option<String>,
    hovered_at: f64,
    wheel: Wheel,
    roster: Vec<String>,
    roster_at: f64,
    overlay_drew: Option<f32>,
    shown: Shown,
    panel_showing: Option<String>,
    unreadable: (u64, Option<String>),
    knocked: std::sync::Arc<std::sync::atomic::AtomicBool>,
    keyboard_owns: bool,
    panel_visible: bool,
    /// Histories already fetched in this lobby, because each fetch is a match
    /// history call to Riot. Cleared with the lobby, so it holds ten at most.
    histories: HashMap<String, Box<Profile>>,
    said: Option<(String, f64)>,
    quieting: Option<f64>,
    place: Place,
    machine: crate::machine::Machine,
}

const SAID: f64 = 2.5;

const UNSAVED: &str = "Couldn't save your notes. Check the drive has room";

const WINDOW_KEY: &str = "window";

#[derive(Debug, Default)]
struct Shown {
    /// Until when a press of the hotkey keeps it up, on egui's clock.
    peek_until: Option<f64>,
    by_itself: bool,
    armed_for: Option<String>,
}

impl Shown {
    fn up(&self, auto: bool, players: bool, now: f64) -> bool {
        let peeking = self.peek_until.is_some_and(|until| now < until);
        peeking || (auto && players && self.by_itself)
    }

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

#[derive(Debug, Default)]
struct Place {
    minimized: bool,
    kept: Option<String>,
    maximize: bool,
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

const EDGE_RIGHT: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hint {
    Worth,
    Pick,
    Note,
    /// Copy their Riot ID.
    Copy,
    Overlay,
    /// Open or close the settings.
    Settings,
    /// Open or close your match history.
    History,
    /// Open or close your lineups.
    Lineups,
    Release,
    Quiet,
    Help,
    OverlayKey,
}

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

/// Every character past ASCII in an answer's strings, so the faces for
/// players' names in other scripts load before History or a profile draws
/// them, as they already do for the live board.
fn foreign(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.chars().filter(|c| !c.is_ascii()).collect(),
        serde_json::Value::Array(items) => items.iter().map(foreign).collect(),
        serde_json::Value::Object(fields) => fields.values().map(foreign).collect(),
        _ => String::new(),
    }
}

fn match_began(was: Option<&str>, now: Option<&str>) -> bool {
    was == Some("PREGAME") && now == Some("INGAME")
}

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

const fn resize_cursor(to: egui::ResizeDirection) -> egui::CursorIcon {
    use egui::{CursorIcon, ResizeDirection as To};
    match to {
        To::North | To::South => CursorIcon::ResizeVertical,
        To::East | To::West => CursorIcon::ResizeHorizontal,
        To::NorthWest | To::SouthEast => CursorIcon::ResizeNwSe,
        To::NorthEast | To::SouthWest => CursorIcon::ResizeNeSw,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    Board,
    Settings,
    History,
    Lineups,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Empty {
    /// How far the backend, the Riot client and VALORANT have got.
    Plate,
    LastStats,
}

#[derive(Debug, Default)]
struct Wheel {
    /// When the last notch came, in egui's seconds.
    at: f64,
    boost: f32,
}

impl Wheel {
    /// Times egui's forty points a notch.
    const BASE: f32 = 1.6;
    const FLICK: f64 = 0.12;
    const STEP: f32 = 0.15;
    const MOST: f32 = 1.6;

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
    fn take(&mut self, board: Board) {
        let now = now_secs();
        let board = if board.held_since.is_some_and(|at| now - at >= KEEP) {
            gone(board.you)
        } else {
            board
        };
        if self.holds_through(&board, now) {
            self.board.held_since = Some(self.board.held_since.unwrap_or(self.read_at));
            self.board.reading = None;
            if board.you.is_some() {
                self.board.you = board.you;
            }
            return;
        }
        if !board.players.is_empty() && board.held_since.is_none() {
            self.read_at = now;
        }
        self.tags_given = board.players.iter().map(|p| p.auto_tags.clone()).collect();
        self.board = board;
        self.retag();
    }

    /// Whether `incoming` is a gap in the match on screen rather than its
    /// end: a backend that was started again and is still looking the
    /// match up, or one that couldn't read the game this time. Either says
    /// nothing about the match, so the board stays up for [`KEEP`] seconds
    /// from its last good read. VALORANT or the Riot Client closing, or
    /// another match, is real news and clears it.
    fn holds_through(&self, incoming: &Board, now: f64) -> bool {
        let on = &self.board;
        let gap = incoming.waiting.as_deref() == Some("match")
            || (incoming.state.as_deref() == Some("OFFLINE") && incoming.waiting.is_none());
        incoming.players.is_empty()
            && gap
            && !on.players.is_empty()
            && matches!(on.state.as_deref(), Some("PREGAME" | "INGAME"))
            && incoming
                .match_id
                .as_ref()
                .is_none_or(|id| on.match_id.as_ref() == Some(id))
            && now - on.held_since.unwrap_or(self.read_at) < KEEP
    }

    fn expire(&mut self, ctx: &egui::Context) {
        if self.board.players.is_empty() {
            return;
        }
        let since_lost = self.lost_at.map(|(since, _)| ctx.input(|i| i.time) - since);
        let since_read = self.board.held_since.map(|at| now_secs() - at);
        let Some(up) = since_lost.into_iter().chain(since_read).reduce(f64::max) else {
            return;
        };
        if up < KEEP {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(KEEP - up));
            return;
        }
        self.tags_given.clear();
        self.board = gone(self.board.you.take());
    }

    fn retag(&mut self) {
        for (player, given) in self.board.players.iter_mut().zip(&self.tags_given) {
            player.auto_tags = self.settings.tags.applied(given);
        }
    }

    fn dress(cc: &CreationContext<'_>, root: &Path) -> bool {
        overseer_ui::install_fonts(&cc.egui_ctx);
        overseer_ui::art::warm(&cc.egui_ctx);
        overseer_ui::art::set_fetched_dir(root.join("backend").join("data").join("art"));
        // Lineup clips play in a child of this window, through Windows' engine.
        if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(cc)
            && let raw_window_handle::RawWindowHandle::Win32(window) = handle.as_raw()
        {
            overseer_native::set_window(window.hwnd.get());
        }
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        // Windows' "Text size" setting, as a zoom of the whole window, since
        // every row and column here is measured from its type.
        cc.egui_ctx.set_zoom_factor(overseer_native::text_scale());
        cc.egui_ctx
            .set_style_of(egui::Theme::Dark, overseer_ui::style());
        let software = match cc.wgpu_render_state.as_ref() {
            Some(state) => {
                let adapter = state.adapter.get_info();
                println!("{}", crate::probe::adapter_line(&adapter));
                crate::probe::software(&adapter)
            }
            // Its driver's name is only an unsafe call away. With no GPU
            // listed, though, OpenGL can only be drawing in software.
            None if cc.gl.is_some() => {
                println!("adapter OpenGL through glow");
                !overseer_native::any_gpu()
            }
            None => false,
        };
        if software {
            overseer_native::one_core();
            overseer_native::step_back();
        }
        software
    }

    pub(crate) fn new(
        cc: &CreationContext<'_>,
        root: &Path,
        (listener, maximize, broken): (Option<std::net::TcpListener>, bool, Option<String>),
    ) -> Self {
        let software = Self::dress(cc, root);
        let bridge = Bridge::start(root, board_waker(&cc.egui_ctx));
        let knocked = std::sync::Arc::<std::sync::atomic::AtomicBool>::default();
        if let Some(listener) = listener {
            crate::instance::listen(
                listener,
                std::sync::Arc::clone(&knocked),
                waker(&cc.egui_ctx),
            );
        }
        let settings = settings::load(root);
        let key = settings.overlay.key;
        Self {
            bridge,
            root: root.to_path_buf(),
            settings,
            board: Board {
                you: waiting::kept(root),
                ..Board::default()
            },
            tags_given: Vec::new(),
            status: Status::Connecting(String::new()),
            stopped: broken.clone(),
            broken,
            selected: None,
            screen: Screen::Board,
            screen_showing: Screen::Board,
            boards: 0,
            budget: Budget::starting(software),
            opened_at: None,
            settling: None,
            ever_live: false,
            lost_at: None,
            read_at: 0.0,
            restarts: Vec::new(),
            empty: Empty::Plate,
            sort: Sort::default(),
            notes: notes::load(root, About::Players),
            hotkey: Some(report(
                "overlay key",
                hotkey::start(key, waker(&cc.egui_ctx)),
            )),
            keys_quiet_until: 0.0,
            // It owns a hidden window whose messages go to the thread that
            // made it, and this is the message loop's thread.
            tray: report("tray", tray::start(root, waker(&cc.egui_ctx))),
            career: Career::default(),
            history: History::new(notes::load(root, About::Games)),
            lineups: Lineups::default(),
            offline: Offline::default(),
            update: crate::update::Update::default(),
            art_asks: Vec::new(),
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
            quieting: None,
            machine: crate::machine::Machine::detect(),
            place: Place {
                minimized: false,
                kept: cc.storage.and_then(|s| s.get_string(WINDOW_KEY)),
                maximize,
            },
        }
    }

    fn answer(&mut self, ctx: &egui::Context, id: u64, result: Result<serde_json::Value, String>) {
        if let Ok(value) = &result {
            overseer_ui::cover(ctx, &foreign(value));
        }
        if self.history.waiting_on(id) {
            self.history.answered(result);
        } else if self.lineups.waiting_on(id) {
            self.lineups.answered(id, result);
        } else if self.offline.waiting_on(id) {
            self.offline.answered(result);
        } else if let Some(at) = self.art_asks.iter().position(|asked| *asked == id) {
            self.art_asks.swap_remove(at);
        } else if self.update.waiting_on(id) {
            let now = crate::update::now();
            self.update
                .answered(&result, &mut self.settings.updates, now);
            settings::save(&self.root, &self.settings);
        } else {
            self.career.answered(id, result, &mut self.histories);
        }
    }

    fn pump(&mut self, ctx: &egui::Context) {
        self.overlay_key(ctx.input(|i| i.time));
        self.reach(ctx);
        for event in self.bridge.drain() {
            match event {
                Event::Status(status) => {
                    if status == Status::Live {
                        self.career.reconnected();
                        self.history.reconnected();
                        self.lineups.reconnected(&self.bridge);
                        self.offline.reconnected();
                        self.update.reconnected();
                        self.tell_games_read();
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
                Event::Answer { id, result } => self.answer(ctx, id, result),
                Event::Unreadable(why) => {
                    self.unreadable.0 = self.unreadable.0.saturating_add(1);
                    self.unreadable.1 = Some(why);
                }
                Event::Board(board) => {
                    let was = self.board.state.clone();
                    let was_on = self.board.map.clone();
                    self.take(*board);
                    if let Some(map) = self.board.map.as_deref()
                        && was_on.as_deref() != Some(map)
                    {
                        self.lineups.follow(map);
                    }
                    self.shown
                        .follow(self.board.state.as_deref(), self.board.match_id.as_deref());
                    let began = match_began(was.as_deref(), self.board.state.as_deref());
                    if self.settings.minimize_in_matches
                        && began
                        && !ctx.input(|i| i.viewport().focused.unwrap_or(false))
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    let names: String = self
                        .board
                        .players
                        .iter()
                        .filter_map(|p| p.name.as_deref())
                        .collect();
                    overseer_ui::cover(ctx, &names);
                    self.boards = self.boards.saturating_add(1);
                    self.keep_selection();
                    let mut roster: Vec<String> = self
                        .board
                        .players
                        .iter()
                        .filter(|p| board::side_of(&self.board, p) == Side::Enemy)
                        .filter_map(|p| p.puuid.clone())
                        .collect();
                    roster.sort_unstable();
                    if roster != self.roster {
                        if !roster.is_empty() {
                            self.roster_at = ctx.input(|i| i.time);
                        }
                        self.roster = roster;
                        self.histories.clear();
                        if let Some(first) = self.opening_pick() {
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
        self.mark_lost(ctx);
        self.expire(ctx);
    }

    /// Notes when the connection went. Held through the retries in between
    /// too, so the board stays marked stale and its clock counts from the
    /// first loss, until it's back.
    fn mark_lost(&mut self, ctx: &egui::Context) {
        self.lost_at = match self.status {
            Status::Live => None,
            Status::Lost(_) => Some(
                self.lost_at
                    .unwrap_or_else(|| (ctx.input(|i| i.time), false)),
            ),
            Status::Connecting(_) => self.lost_at,
        };
        let Some((since, false)) = self.lost_at else {
            return;
        };
        if self.stopped.is_some() || self.broken.is_some() {
            return;
        }
        let gone = ctx.input(|i| i.time) - since;
        if gone < BACK_UP {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(BACK_UP - gone));
            return;
        }
        if installing(&self.root) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(BACK_UP));
            return;
        }
        self.lost_at = Some((since, true));
        let now = ctx.input(|i| i.time);
        self.restarts.retain(|at| now - at < RESTART_SPAN);
        if self.restarts.len() >= RESTARTS {
            self.stopped = Some(format!(
                "Overseer's backend stopped {RESTARTS} times in {} minutes, so it isn't being started again. Reconnect Now tries once more, and .overseer\\launcher.log says what went wrong.",
                RESTART_SPAN / 60.0
            ));
            return;
        }
        self.restarts.push(now);
        self.broken = crate::serve(&self.root);
    }

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

    fn current(&self) -> Option<&Player> {
        let id = self.hovered.as_ref().or(self.selected.as_ref())?;
        self.board
            .players
            .iter()
            .find(|p| p.puuid.as_ref() == Some(id))
    }

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
        self.hovered = None;
        self.keyboard_owns = true;
    }

    fn other_team(&mut self) {
        let team_of = |id: &str| {
            self.board
                .players
                .iter()
                .find(|p| p.puuid.as_deref() == Some(id))
                .and_then(|p| p.team.clone())
        };
        let mine = self.selected.as_deref().and_then(team_of);
        let Some(next) = self
            .visible_order()
            .into_iter()
            .find(|id| team_of(id) != mine)
        else {
            return;
        };
        self.selected = Some(next);
        self.hovered = None;
        self.keyboard_owns = true;
    }

    fn career_subject(&self, now: f64) -> Option<&str> {
        if self.showing_kept() {
            return self.board.you.as_ref()?.player.puuid.as_deref();
        }
        match self.hovered.as_deref() {
            Some(id) if now - self.hovered_at >= DWELL => Some(id),
            Some(_) => None,
            None => self.selected.as_deref(),
        }
    }

    /// The keyboard shortcuts.
    fn keys(&mut self, ui: &Ui) {
        let typing = ui.memory(egui::Memory::focused).is_some();
        if hotkey::catching() || ui.input(|i| i.time) < self.keys_quiet_until {
            return;
        }
        let note = ui.input(|i| i.key_pressed(Key::N));
        let (up, down, settings, history, lineups, escape, overlay, worth, copy) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Comma),
                i.key_pressed(Key::H),
                i.key_pressed(Key::L),
                i.key_pressed(Key::Escape),
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
            if self.screen == Screen::Board {
                self.empty = Empty::Plate;
            }
            self.screen = Screen::Board;
            ui.memory_mut(egui::Memory::stop_text_input);
            return;
        }
        if typing {
            return;
        }
        let ctx = ui.ctx();
        if overlay {
            self.perform(ctx, Hint::Overlay);
        }
        if note && self.selected.is_some() {
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
        if (up || down) && self.screen != Screen::Lineups {
            self.screen = Screen::Board;
            self.step(down);
        }
        self.ends(ui);
    }

    fn ends(&mut self, ui: &Ui) {
        let (top, bottom, help, cross) = ui.input(|i| {
            (
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.key_pressed(Key::F1) || i.key_pressed(Key::Questionmark),
                i.key_pressed(Key::T),
            )
        });
        if cross && self.screen == Screen::Board {
            self.other_team();
        }
        if help {
            self.perform(ui.ctx(), Hint::Help);
        }
        if (top || bottom) && self.screen == Screen::Board {
            let order = self.visible_order();
            if let Some(id) = if top { order.first() } else { order.last() } {
                self.selected = Some(id.clone());
                self.keyboard_owns = true;
            }
        }
    }

    fn perform(&mut self, ctx: &egui::Context, hint: Hint) {
        match hint {
            Hint::Worth => {
                self.screen = Screen::Board;
                if !self.board.players.iter().any(|p| p.smurf) {
                    self.say(ctx, "Nobody Worth a Look in This Lobby");
                }
                self.next_flagged(None);
            }
            Hint::Pick if self.screen == Screen::Lineups => self.lineups.step_down(),
            Hint::Pick => {
                self.screen = Screen::Board;
                self.step(true);
            }
            Hint::Note => self.note(ctx),
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
                    self.screen = Screen::History;
                    self.history.ask(&self.bridge, self.status == Status::Live);
                }
            }
            Hint::Lineups => self.flip_lineups(),
            Hint::Release => {
                if let Some(page) = crate::update::newer(&self.settings.updates, VERSION)
                    .and_then(crate::update::page)
                {
                    drop(overseer_native::launch(
                        std::process::Command::new("explorer").arg(page),
                    ));
                }
            }
            Hint::Quiet => self.quiet(ctx),
            Hint::OverlayKey => {
                self.screen = Screen::Settings;
                crate::view::open_at(ctx, "Overlay");
            }
            Hint::Help => {
                if self.screen == Screen::Help {
                    self.screen = Screen::Board;
                } else {
                    if self.screen == Screen::Lineups {
                        crate::view::open_at(ctx, "Keyboard Shortcuts");
                    }
                    self.screen = Screen::Help;
                }
            }
        }
    }

    fn step(&mut self, down: bool) {
        let order = self.visible_order();
        if let Some(next) = board::step(&order, self.selected.as_deref(), down) {
            self.selected = Some(next);
            self.keyboard_owns = true;
        }
    }

    fn flip_lineups(&mut self) {
        if self.screen == Screen::Lineups {
            self.screen = Screen::Board;
            return;
        }
        self.screen = Screen::Lineups;
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

    fn listener(&self) -> Option<&Listener> {
        self.hotkey.as_ref().and_then(|k| k.as_ref().ok())
    }

    fn overlay_key(&mut self, now: f64) {
        let heard = self.listener().map(Listener::heard).unwrap_or_default();
        if heard.presses % 2 == 1 {
            self.flip_overlay(now);
        }
        if heard.done {
            self.keys_quiet_until = now + 0.3;
        }
        if let Some(caught) = heard.caught {
            self.settings.overlay.key = caught;
            settings::save(&self.root, &self.settings);
        }
        if hotkey::catching()
            && self.screen != Screen::Settings
            && let Some(listener) = self.listener()
        {
            listener.stop();
        }
    }

    fn note(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        self.screen = Screen::Board;
        if self.panel_visible {
            panel::write_note(ctx, &id);
        } else {
            self.say(
                ctx,
                "Notes are written in the detail panel. Widen the window, or turn the panel on in Settings",
            );
        }
    }

    fn quiet(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self
            .quieting
            .is_some_and(|at| now - at < crate::lineups::ARMED)
        {
            self.quieting = None;
            self.settings.updates.check_updates = false;
            settings::save(&self.root, &self.settings);
            self.say(ctx, "Update notices off. Settings turns them back on");
        } else {
            self.quieting = Some(now);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(crate::lineups::ARMED));
        }
    }

    fn say(&mut self, ctx: &egui::Context, words: &str) {
        self.said = Some((words.to_owned(), ctx.input(|i| i.time)));
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAID));
    }

    fn visible_order(&self) -> Vec<String> {
        board::order(&self.board, &self.sort, self.settings.enemies_first)
    }

    fn opening_pick(&self) -> Option<String> {
        let ours = self.board.self_team.as_deref();
        let enemy = |id: &&String, flagged: bool| {
            self.board.players.iter().any(|p| {
                p.puuid.as_deref() == Some(id.as_str())
                    && p.team.as_deref() != ours
                    && (p.smurf || !flagged)
            })
        };
        let order = self.visible_order();
        order
            .iter()
            .find(|id| enemy(id, true))
            .or_else(|| order.iter().find(|id| enemy(id, false)))
            .cloned()
    }

    fn reason(&self) -> Option<(&'static str, &str)> {
        if let Some(why) = &self.broken {
            return Some(("Overseer Can't Start", why));
        }
        // The bridge's own thread never started, which a fresh try often
        // gets past. Anything else it stopped on is a build it won't talk to.
        if self
            .stopped
            .as_deref()
            .is_some_and(|why| why.starts_with("no bridge thread"))
        {
            return Some((
                "Overseer Couldn't Connect",
                "Windows didn't let Overseer start its connection. Press Reconnect Now, and if that doesn't help, restart your PC.",
            ));
        }
        self.stopped
            .as_deref()
            .or_else(|| {
                (self.boards == 0)
                    .then_some(self.unreadable.1.as_deref())
                    .flatten()
            })
            .map(|_| {
                (
                    "Overseer Needs a Repair",
                    "Its window and the part that talks to Riot don't match, which happens after an update stops partway. Run install.bat in Overseer's folder to repair it. Your settings and lineups are kept.",
                )
            })
    }

    /// Starts a fresh connection straight away, rather than after the old
    /// one's wait. Dropping the old one stops its thread.
    pub(super) fn reconnect(&mut self, ctx: &egui::Context) {
        self.bridge = Bridge::start(&self.root, board_waker(ctx));
        self.stopped = None;
        // Checked again, since install.bat may have repaired it meanwhile.
        self.broken = self.broken.take().and_then(|_| crate::serve(&self.root));
        self.status = Status::Connecting(String::new());
        self.restarts.clear();
        self.lost_at = self.lost_at.map(|(since, _)| (since, false));
    }

    fn reach(&mut self, ctx: &egui::Context) {
        let actions: Vec<Action> = self
            .tray
            .as_ref()
            .map(|t| t.actions().collect())
            .unwrap_or_default();
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

    fn reveal(ctx: &egui::Context, focus: bool) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        if focus {
            // Windows ignores Focus on a minimized window.
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn closing(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        self.lineups.leave();
        settings::save(&self.root, &self.settings);
        notes::save(&self.root, About::Players, &self.notes);
        notes::save(&self.root, About::Games, &self.history.notes);
    }

    pub(crate) fn flip_overlay(&mut self, now: f64) {
        if self.overlay_up(now) {
            self.hide_overlay();
        } else {
            self.peek_overlay(now);
        }
    }

    fn peek_overlay(&mut self, now: f64) {
        self.shown.peek_until = Some(now + PEEK);
        // The overlay draws the window's board, and a settings screen behind
        // it would be a trap.
        self.screen = Screen::Board;
    }

    pub(super) const fn hide_overlay(&mut self) {
        self.shown.peek_until = None;
        self.shown.by_itself = false;
    }

    pub(super) fn overlay_up(&self, now: f64) -> bool {
        self.shown.up(
            self.settings.overlay.auto,
            !self.board.players.is_empty(),
            now,
        )
    }

    fn pace(&self, base: f32) -> f32 {
        if self.quality() == Quality::Efficient {
            motion::EFFICIENT
        } else {
            base
        }
    }

    fn slow_pc(&self) -> bool {
        match self.settings.pc {
            settings::Pc::Auto => !self.machine.slow_because().is_empty(),
            settings::Pc::Fast => false,
            settings::Pc::Slow => true,
        }
    }

    const fn quality(&self) -> Quality {
        match self.settings.quality {
            _ if matches!(self.budget.calm, Calm::Software) => Quality::Efficient,
            Quality::Auto
                if self.budget.dropped || matches!(self.budget.calm, Calm::Animations) =>
            {
                Quality::Efficient
            }
            Quality::Auto => Quality::Rich,
            other => other,
        }
    }

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
        self.budget.fast = self.budget.fast.saturating_add(1);
        if self.budget.dropped && self.budget.fast >= FAST_STREAK {
            self.budget.dropped = false;
            self.budget.fast = 0;
        }
    }

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
    fn header(&self, ui: &mut Ui, edge: bool) -> (Option<header::Chrome>, bool) {
        header::draw(
            ui,
            &header::Masthead {
                board: &self.board,
                light: self.connection(ui.input(|i| i.time)),
                still: self.quality() == Quality::Efficient,
                maximized: maximized(ui.ctx()),
                open: match self.screen {
                    Screen::Settings => Some(header::Chrome::Settings),
                    Screen::History => Some(header::Chrome::History),
                    Screen::Lineups => Some(header::Chrome::Lineups),
                    Screen::Board | Screen::Help => None,
                },
                edge,
            },
        )
    }

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

    /// The connection light. The masthead says it in a word, or nothing
    /// while all is well, and the exact reason is on its hover. It is the
    /// only sign that a board isn't live, so the board itself reads as it
    /// was. `now` is egui's clock.
    fn connection(&self, now: f64) -> header::Light {
        let (radius, tint, word, why) = match (&self.stopped, &self.status) {
            (Some(why), _) => (3.0, colour::ENEMY, "Stopped", why.clone()),
            (None, Status::Lost(why)) => {
                let gone = self.lost_at.map_or_else(
                    || "just now".to_owned(),
                    |(since, _)| format!("{} ago", waiting::since_text(now - since, true)),
                );
                let kept = if self.board.players.is_empty() {
                    String::new()
                } else {
                    " The board is from then, and it stays up for an hour or until Overseer is back.".to_owned()
                };
                (
                    3.0,
                    colour::ENEMY,
                    "Reconnecting",
                    format!("Lost the connection {gone}, and Overseer is reconnecting by itself.{kept} {why}"),
                )
            }
            (None, Status::Live) if !self.board.players.is_empty() && self.board.held_since.is_some() => (
                3.0,
                colour::WARN,
                "Not Live",
                format!(
                    "Can't reach VALORANT right now, so this board is from {}. It stays up until the match is seen to end, for an hour at most.",
                    crate::history::kept_ago((now_secs() * 1000.0) as i64, self.board.held_since)
                ),
            ),
            (None, Status::Live) if self.board.state.as_deref() == Some("OFFLINE") => (
                3.0,
                colour::TEXT_FAINT,
                "",
                "VALORANT isn't running. History and Lineups still work from what's saved. Click to look for it again now.".to_owned(),
            ),
            (None, Status::Live) => (
                3.0,
                colour::ALLY,
                "",
                "Connected to VALORANT. The board follows the game as it changes. Click to read it again now.".to_owned(),
            ),
            (None, Status::Connecting(detail)) => (2.0, colour::WARN, "Connecting", detail.clone()),
        };
        header::Light {
            radius,
            tint,
            word,
            why,
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

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        notes::save(&self.root, About::Players, &self.notes);
        notes::save(&self.root, About::Games, &self.history.notes);
        self.place.keep(storage);
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame) {
        survive(|| self.tick(ctx, frame));
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        survive(|| self.draw(ui));
    }
}

/// Runs a frame's work so a panic in it costs that frame and not the window.
/// The panic is still written down by the hook `main` sets, and egui's locks
/// don't poison, so the next frame starts clean.
fn survive(work: impl FnOnce()) {
    drop(std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)));
}

impl Overseer {
    /// Everything that has to run while the window is minimised, when eframe
    /// skips `ui`: the bridge, the hotkey, the tray and the close button.
    fn tick(&mut self, ctx: &egui::Context, frame: &Frame) {
        self.place.minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if self.place.maximize {
            if ctx.cumulative_pass_nr() > 0 {
                self.place.maximize = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            } else {
                ctx.request_repaint();
            }
        }
        self.pump(ctx);
        if self.status == Status::Live {
            self.update
                .ask(&self.bridge, &self.settings.updates, crate::update::now());
            for (kind, name) in overseer_ui::art::wanted() {
                let asked = serde_json::json!({ "kind": kind.label(), "name": name });
                self.art_asks.push(self.bridge.ask("art", asked));
            }
        }
        self.closing(ctx);
        self.watch_frames(ctx, frame);
        let now = ctx.input(|i| i.time);
        self.overlay(ctx, now);
        if !self.panel_visible {
            return;
        }
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

    fn draw(&mut self, ui: &mut Ui) {
        motion::set_still(ui.ctx(), self.quality() == Quality::Efficient);
        let edge = edge(ui.ctx());
        if let Some(to) = edge
            && ui.input(|i| i.pointer.primary_pressed())
        {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(to));
        }
        self.window(ui, edge.is_some());
        if let Some(to) = edge {
            ui.ctx().set_cursor_icon(resize_cursor(to));
        }
        overseer_ui::chrome::outline(ui.ctx());
        overseer_ui::chrome::focus(ui.ctx());
    }
}

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

fn installing(root: &Path) -> bool {
    std::fs::File::open(root.join(".overseer").join("install.lock"))
        .is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound)
}

/// Like `waker`, but for a new board, which needs one frame where egui gives
/// a plain repaint two. On Windows' software renderer each frame costs most
/// of a second.
fn board_waker(ctx: &egui::Context) -> impl Fn() + Send + Sync + 'static + use<> {
    let ctx = ctx.clone();
    move || ctx.request_repaint_after(std::time::Duration::from_millis(1))
}

/// The time now, in seconds since 1970.
fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64())
}

fn gone(you: Option<overseer_core::Saved>) -> Board {
    Board {
        state: Some("OFFLINE".to_owned()),
        you,
        ..Board::default()
    }
}

#[cfg(test)]
pub(crate) fn snapshot_empty(ui: &mut Ui, shown: &Board) {
    let board = Board {
        waiting: Some("game".to_owned()),
        you: shown.you.clone(),
        ..Board::default()
    };
    let _pressed = waiting::empty(
        ui,
        (&Status::Live, &board),
        None,
        motion::EFFICIENT,
        Some(false),
    );
}

#[cfg(test)]
pub(crate) fn snapshot_kept(you: &overseer_core::Saved) -> Board {
    waiting::kept_board(you)
}

#[cfg(test)]
pub(crate) fn testing(board: &Board, boards: u64, settings: &Settings) -> Overseer {
    Overseer {
        bridge: Bridge::start(Path::new("."), || {}),
        root: PathBuf::new(),
        settings: settings.clone(),
        board: board.clone(),
        tags_given: Vec::new(),
        status: Status::Live,
        stopped: None,
        broken: None,
        selected: None,
        screen: Screen::Board,
        screen_showing: Screen::Board,
        boards,
        budget: Budget::default(),
        opened_at: None,
        settling: None,
        ever_live: true,
        lost_at: None,
        read_at: 0.0,
        restarts: Vec::new(),
        empty: Empty::Plate,
        sort: Sort::default(),
        notes: Notes::default(),
        // Neither in a test: one would read the keyboard and the other
        // would put an icon beside the clock.
        hotkey: None,
        keys_quiet_until: 0.0,
        tray: Err(String::new()),
        career: Career::default(),
        history: History::default(),
        lineups: Lineups::default(),
        offline: Offline::default(),
        update: crate::update::Update::default(),
        art_asks: Vec::new(),
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
        quieting: None,
        place: Place::default(),
        machine: crate::machine::Machine {
            memory: None,
            threads: 8,
            card: true,
        },
    }
}

#[cfg(test)]
pub(crate) fn snapshot_chrome(ui: &mut Ui, board: &Board, boards: u64, settings: &Settings) {
    let shown = testing(board, boards, settings);
    let _pressed = egui::Panel::top("header")
        .exact_size(header::HEIGHT)
        .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
        .show(ui, |ui| shown.header(ui, false));
    let _clicked = egui::Panel::bottom("footer")
        .exact_size(overseer_ui::space::XXL)
        .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
        .show(ui, |ui| shown.footer(ui));
}

#[cfg(test)]
mod tests {
    use super::{BACK_UP, Place, Shown, panel_grip};
    use overseer_core::Status;

    #[test]
    fn a_backend_that_went_is_started_again_once() {
        let settings = crate::settings::Settings::default();
        let mut app = super::testing(&overseer_core::Board::default(), 0, &settings);
        let ctx = egui::Context::default();
        let lost = || Status::Lost("gone".to_owned());

        let long_ago = -BACK_UP - 1.0;
        app.status = lost();
        app.mark_lost(&ctx);
        assert_eq!(
            app.lost_at,
            Some((0.0, false)),
            "a dropped socket gets time"
        );
        app.lost_at = Some((long_ago, false));
        app.mark_lost(&ctx);
        assert_eq!(app.lost_at, Some((long_ago, true)));
        assert!(app.broken.is_none());

        app.status = Status::Live;
        app.mark_lost(&ctx);
        assert!(app.lost_at.is_none(), "the next time gets its own");

        app.root = std::env::temp_dir().join("overseer-revive-test");
        let held = app.root.join(".overseer");
        std::fs::create_dir_all(&held).unwrap();
        let lock = std::os::windows::fs::OpenOptionsExt::share_mode(
            std::fs::OpenOptions::new().write(true).create(true),
            0,
        )
        .open(held.join("install.lock"))
        .unwrap();
        app.status = lost();
        app.lost_at = Some((long_ago, false));
        app.mark_lost(&ctx);
        assert_eq!(
            app.lost_at,
            Some((long_ago, false)),
            "an install is running"
        );
        drop(lock);
        app.mark_lost(&ctx);
        assert_eq!(app.lost_at, Some((long_ago, true)));
        assert!(
            app.broken
                .as_deref()
                .is_some_and(|why| why.contains("Run the setup again")),
            "an install with no backend says so"
        );
        drop(std::fs::remove_dir_all(&app.root));
        app.broken = None;

        app.status = lost();
        app.lost_at = Some((long_ago, false));
        app.stopped = Some("a build it won't talk to".to_owned());
        app.mark_lost(&ctx);
        assert_eq!(
            app.lost_at,
            Some((long_ago, false)),
            "starting it again fixes nothing"
        );
    }

    #[test]
    fn a_backend_that_keeps_dying_is_left_alone() {
        let settings = crate::settings::Settings::default();
        let mut app = super::testing(&overseer_core::Board::default(), 0, &settings);
        let ctx = egui::Context::default();
        for n in 0..=super::RESTARTS {
            app.status = Status::Lost("gone".to_owned());
            app.lost_at = Some((-BACK_UP - 1.0, false));
            app.mark_lost(&ctx);
            assert_eq!(app.stopped.is_some(), n == super::RESTARTS, "after {n}");
        }
        assert_eq!(app.restarts.len(), super::RESTARTS);
        assert!(app.connection(0.0).why.contains("launcher.log"));

        // Run after install.bat repaired a missing Python, it checks again.
        app.broken = Some("Its Python is gone".to_owned());
        app.reconnect(&ctx);
        assert!(app.stopped.is_none() && app.broken.is_none() && app.restarts.is_empty());
        app.status = Status::Lost("gone".to_owned());
        app.mark_lost(&ctx);
        assert_eq!(app.restarts.len(), 1, "started again straight away");
    }

    fn in_match() -> super::Overseer {
        let mut board = crate::shot::sample();
        board.state = Some("INGAME".to_owned());
        board.match_id = Some("m1".to_owned());
        let mut app = super::testing(
            &overseer_core::Board::default(),
            0,
            &crate::settings::Settings::default(),
        );
        app.take(board);
        app
    }

    #[test]
    fn a_match_stays_up_through_a_gap() {
        use overseer_core::Board;
        let looking = Board {
            state: Some("MENUS".to_owned()),
            waiting: Some("match".to_owned()),
            ..Board::default()
        };
        let unread = Board {
            state: Some("OFFLINE".to_owned()),
            ..Board::default()
        };
        for gap in [looking.clone(), unread] {
            let mut app = in_match();
            app.take(gap);
            assert_eq!(app.board.players.len(), 10, "the match stays up");
            assert_eq!(app.board.state.as_deref(), Some("INGAME"));
            assert!(app.board.held_since.is_some(), "and the light can say so");
        }

        let closed = Board {
            state: Some("OFFLINE".to_owned()),
            waiting: Some("riot".to_owned()),
            ..Board::default()
        };
        let next = Board {
            match_id: Some("m2".to_owned()),
            ..looking.clone()
        };
        for news in [closed, next] {
            let mut app = in_match();
            app.take(news);
            assert!(app.board.players.is_empty(), "real news clears it");
        }

        let mut app = in_match();
        app.read_at -= super::KEEP;
        app.take(looking);
        assert!(app.board.players.is_empty(), "not past an hour");
    }

    #[test]
    fn a_board_comes_off_after_an_hour() {
        let ctx = egui::Context::default();
        let you = overseer_core::Saved::default();

        let mut app = in_match();
        app.board.you = Some(you);
        app.status = Status::Lost("gone".to_owned());
        app.lost_at = Some((1.0 - super::KEEP, true));
        app.expire(&ctx);
        assert_eq!(app.board.players.len(), 10, "not an hour yet");
        app.lost_at = Some((-super::KEEP, true));
        app.expire(&ctx);
        assert!(app.board.players.is_empty());
        assert!(app.board.you.is_some(), "your row stays");

        let mut app = in_match();
        app.board.held_since = Some(super::now_secs() - super::KEEP);
        app.expire(&ctx);
        assert!(app.board.players.is_empty(), "held by the backend an hour");

        let mut stale = crate::shot::sample();
        stale.held_since = Some(super::now_secs() - super::KEEP);
        app.take(stale);
        assert!(app.board.players.is_empty());
    }

    #[test]
    fn the_light_says_a_board_is_not_live() {
        let mut app = in_match();
        assert_eq!(app.connection(0.0).word, "");
        app.board.held_since = Some(super::now_secs() - 120.0);
        let held = app.connection(0.0);
        assert_eq!(held.word, "Not Live");
        assert!(held.why.contains("2 minutes ago"), "{}", held.why);
        app.status = Status::Lost("refused".to_owned());
        app.lost_at = Some((0.0, true));
        let lost = app.connection(90.0);
        assert_eq!(lost.word, "Reconnecting");
        assert!(lost.why.contains("1 min ago"), "{}", lost.why);
        assert!(lost.why.contains("stays up for an hour"), "{}", lost.why);
    }

    fn rests_on(name: &str, at: egui::Pos2) -> bool {
        moves_off(name, at, None)
    }

    fn moves_off(name: &str, at: egui::Pos2, off: Option<(egui::Pos2, usize)>) -> bool {
        let mut board = crate::shot::sample();
        for player in &mut board.players {
            if player.is_self {
                player.auto_tags.push(overseer_core::AutoTag {
                    tag: Some("odin".to_owned()),
                    why: Some("Carried the Odin in 9 of 60 rounds".to_owned()),
                    means: Some("Carried the Odin in at least 15% of their rounds.".to_owned()),
                    kind: Some("Weapon".to_owned()),
                    order: None,
                });
            }
        }
        let settings = crate::settings::Settings {
            welcome: crate::settings::Welcome::Seen,
            ..crate::settings::Settings::default()
        };
        let mut app = super::testing(&board, 12, &settings);
        app.selected = board
            .players
            .iter()
            .find(|p| p.name.as_deref() == Some(name))
            .and_then(|p| p.puuid.clone());
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 900.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut Option<super::Overseer>| {
                    if let Some(app) = app {
                        app.window(ui, false);
                    }
                },
                None,
            );
        overseer_ui::install_fonts(&harness.ctx);
        harness
            .ctx
            .set_style_of(egui::Theme::Dark, overseer_ui::style());
        harness.run();
        *harness.state_mut() = Some(app);
        for _frame in 0..30 {
            harness.step();
        }
        // Straight away, not after egui's half second of stillness.
        harness.hover_at(at);
        harness.step();
        harness.step();
        if let Some((spot, frames)) = off {
            harness.hover_at(spot);
            for _frame in 0..frames {
                harness.step();
            }
        }
        if let Ok(out) = std::env::var("TAG_OUT") {
            drop(harness.render().map(|i| i.save(out)));
        }
        harness.ctx.memory(|m| {
            m.areas()
                .visible_layer_ids()
                .iter()
                .any(|l| l.order == egui::Order::Tooltip)
        })
    }

    #[test]
    fn a_tag_explains_itself_wherever_it_is() {
        let spots = std::env::var("TAG_AT").ok().and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))
        });
        if let Some(at) = spots {
            assert!(rests_on("Day#9932", at), "resting at {at:?} opened nothing");
            return;
        }
        let spots = [
            ("NeonLock#VAL", (200.0, 186.0), "an enemy's row"),
            ("Day#9932", (191.0, 601.0), "your own row"),
            ("Day#9932", (954.0, 257.0), "the panel's reads"),
            ("Day#9932", (880.0, 505.0), "a stat"),
        ];
        for (name, (x, y), what) in spots {
            assert!(rests_on(name, egui::pos2(x, y)), "{what} opened nothing");
        }
        assert!(
            !rests_on("Day#9932", egui::pos2(700.0, 860.0)),
            "a card opened over nothing"
        );
        let odin = egui::pos2(191.0, 601.0);
        let beside = egui::pos2(260.0, 601.0);
        assert!(
            moves_off("Day#9932", odin, Some((beside, 1))),
            "it dropped at once"
        );
        assert!(
            !moves_off("Day#9932", odin, Some((beside, 20))),
            "it never went"
        );
    }

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

    /// A real drag on the panel's edge, in a 1200 wide window whose panel
    /// starts 340 wide: 100 points left makes it 440, and a double click
    /// gives the width back to the window.
    #[test]
    fn the_panel_edge_drags() {
        let panel = egui::Rect::from_min_max(egui::pos2(860.0, 0.0), egui::pos2(1200.0, 600.0));
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
