use egui::{CentralPanel, Panel, Rect, Ui};
use overseer_core::Status;

use crate::board::{self, Place, Scene, Side};
use crate::notes;
use crate::overlay;
use crate::settings::{self, Discord, Quality, Welcome};
use crate::{panel, view};
use overseer_ui::{self, colour, motion, space};

use super::waiting::empty;
use super::{Empty, Overseer, RESTATE, Screen, panel_width};
use crate::board::Asked;

impl Overseer {
    pub(super) fn detail(&mut self, ui: &mut Ui, width: f32) -> Rect {
        Panel::right("detail")
            .exact_size(panel_width(width, self.settings.panel_width))
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
                ui.painter().vline(
                    all.left() + 1.0,
                    all.y_range(),
                    (1.0, colour::TEXT_STRONG.gamma_multiply(0.07)),
                );
                let kept = self.kept();
                let board = kept.as_ref().unwrap_or(&self.board);
                let wanted = kept
                    .as_ref()
                    .map_or_else(|| self.current(), |kept| kept.players.first())
                    .and_then(|p| p.puuid.clone());
                let settled = ui.ctx().animate_value_with_time(
                    egui::Id::new("panel-subject"),
                    f32::from(wanted == self.panel_showing),
                    self.pace(motion::QUICK) / 2.0,
                );
                if settled <= 0.02 && wanted != self.panel_showing {
                    self.panel_showing = wanted;
                    ui.ctx().request_repaint();
                }
                ui.multiply_opacity(0.92f32.mul_add(settled, 0.08));
                ui.add_space(space::MD);
                let mut lent = std::mem::take(&mut self.notes);
                let showing = kept.as_ref().map_or_else(
                    || {
                        let id = self.panel_showing.as_ref()?;
                        board.players.iter().find(|p| p.puuid.as_ref() == Some(id))
                    },
                    |kept| kept.players.first(),
                );
                let side = showing.map_or(Side::Enemy, |p| board::side_of(board, p));
                let still = self.quality() == Quality::Efficient;
                if panel::show(ui, showing, side, &mut lent, &self.career, still)
                    && !notes::save(&self.root, notes::About::Players, &lent)
                {
                    self.said = Some((super::UNSAVED.to_owned(), ui.input(|i| i.time)));
                }
                self.notes = lent;
            })
            .response
            .rect
    }

    fn settings_screen(&mut self, ui: &mut Ui, chrome: egui::Frame, turning: f32) {
        let quality = self.quality();
        let why = if self.budget.calm == super::Calm::Software {
            Some(
                "Staying on Efficient because no graphics card is drawing the window, and Full would take most of a second a frame.",
            )
        } else if self.settings.quality != Quality::Auto {
            None
        } else if self.budget.calm == super::Calm::Animations {
            Some(
                "Switched to Efficient because animation effects are off in Windows. Pick Full or Efficient to choose yourself.",
            )
        } else if self.budget.dropped {
            Some(
                "Switched to Efficient because the window was lagging. Pick Full or Efficient to choose yourself.",
            )
        } else {
            None
        };
        let trouble = view::Trouble {
            hotkey: self
                .hotkey
                .as_ref()
                .and_then(|k| k.as_ref().err())
                .map(String::as_str),
            tray: self.tray.as_ref().err().map(String::as_str),
        };
        let mut changed = false;
        self.offline.playing = matches!(self.board.state.as_deref(), Some("PREGAME" | "INGAME"));
        CentralPanel::default().frame(chrome).show(ui, |ui| {
            ui.multiply_opacity(0.90f32.mul_add(turning, 0.10));
            changed = view::settings(
                ui,
                &mut self.settings,
                (quality, why, &self.machine),
                (trouble, &self.root, &mut self.offline),
            );
        });
        let again = egui::Id::new(view::TRAY_AGAIN);
        if ui
            .ctx()
            .data_mut(|d| d.remove_temp::<bool>(again))
            .is_some()
        {
            self.tray = crate::tray::start(&self.root, super::waker(ui.ctx()));
        }
        let catch = egui::Id::new(view::CATCH_KEY);
        if ui
            .ctx()
            .data_mut(|d| d.remove_temp::<bool>(catch))
            .is_some()
            && let Some(listener) = self.listener()
        {
            listener.catch();
        }
        if changed {
            settings::save(&self.root, &self.settings);
            self.retag();
            self.tell_games_read();
        }
        let now = ui.input(|i| i.time);
        self.offline.ask(&self.bridge, now);
        // Asked again every few seconds, so the switch follows the proxy.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(crate::offline::POLL));
    }

    pub(super) fn other_screen(&mut self, ui: &mut Ui, chrome: egui::Frame, turning: f32) -> bool {
        let live = self.status == Status::Live;
        match self.screen_showing {
            Screen::Board => return false,
            Screen::Settings => self.settings_screen(ui, chrome, turning),
            Screen::History => self.history_screen(ui, chrome, turning),
            Screen::Help => {
                CentralPanel::default().frame(chrome).show(ui, |ui| {
                    ui.multiply_opacity(0.90f32.mul_add(turning, 0.10));
                    view::help(ui);
                });
            }
            Screen::Lineups => {
                self.lineups.show(
                    ui,
                    (&self.bridge, &self.root),
                    live,
                    (
                        0.90f32.mul_add(turning, 0.10),
                        self.settings.clip(),
                        crate::lineups::Look {
                            turn: self.settings.lineups.map_turn,
                            names: self.settings.lineups.lineup_names,
                            area: self.settings.lineups.area_colour.as_deref(),
                            agent: &self.settings.lineups.default_agent,
                            slow: self.slow_pc(),
                            redraws: &self.settings.lineups.redraws,
                        },
                    ),
                );
                if let Some(agent) = self.lineups.new_default() {
                    self.settings.lineups.default_agent = agent;
                    settings::save(&self.root, &self.settings);
                }
                if let Some(on) = self.lineups.new_looping() {
                    self.settings.lineups.clip_loop = on;
                    settings::save(&self.root, &self.settings);
                }
                if let Some(redraw) = self.lineups.redraw_seen() {
                    self.settings.lineups.redraws.redraws_seen.push(redraw);
                    settings::save(&self.root, &self.settings);
                }
            }
        }
        self.panel_visible = false;
        true
    }

    fn history_screen(&mut self, ui: &mut Ui, chrome: egui::Frame, turning: f32) {
        let now_ms = now_ms();
        let live = self.status == Status::Live;
        let (mut picked, mut noted) = (None, false);
        CentralPanel::default().frame(chrome).show(ui, |ui| {
            ui.multiply_opacity(0.90f32.mul_add(turning, 0.10));
            (picked, noted) = self
                .history
                .show(ui, self.settings.history_games, now_ms, live);
        });
        if noted && !notes::save(&self.root, notes::About::Games, &self.history.notes) {
            self.said = Some((super::UNSAVED.to_owned(), ui.input(|i| i.time)));
        }
        if let Some(count) = picked {
            self.settings.history_games = count;
            settings::save(&self.root, &self.settings);
            self.history.ask(&self.bridge, count, live);
        }
    }

    fn overlay_view(&self, ui: &mut Ui) -> (f32, bool) {
        if self.board.players.is_empty() {
            return (overlay::greeting(ui), false);
        }
        let corner = ui.max_rect();
        let drew = self.rows(ui, Place::Overlay).drew;
        (drew, overlay::hide_button(ui, corner))
    }

    pub(super) fn overlay(&mut self, ctx: &egui::Context, now: f64) {
        if let Some(until) = self.shown.peek_until.filter(|t| *t > now) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(until - now));
        }
        if !self.overlay_up(now) {
            return;
        }
        let last = self.overlay_drew.unwrap_or(overlay::DESIGNED);
        let mut drew = last;
        let mut hidden = false;
        {
            let this = &*self;
            overlay::show(ctx, this.settings.overlay.corner, last, |ui| {
                (drew, hidden) = this.overlay_view(ui);
            });
        }
        overseer_native::never_activate(overlay::TITLE);
        if hidden {
            self.hide_overlay();
            ctx.request_repaint();
        }
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .map_or(0.0, |m| m.y);
        let drew = drew.min(overlay::tallest(monitor));
        if (drew - last).abs() > 0.5 {
            self.overlay_drew = Some(drew);
            ctx.request_repaint();
        }
    }

    pub(super) fn tell_games_read(&self) {
        let _asked = self.bridge.ask(
            "games_read",
            serde_json::json!({
                "count": self.settings.games_read,
                "days": self.settings.games_within,
                "keep": self.settings.party_kept,
            }),
        );
        let _told = self.bridge.ask(
            "discord",
            serde_json::json!({ "on": self.settings.discord == Discord::On }),
        );
    }

    pub(super) fn board_view(&mut self, ui: &mut Ui) {
        if self.settings.welcome == Welcome::Due
            && !self.board.players.is_empty()
            && view::welcome(ui)
        {
            self.settings.welcome = Welcome::Seen;
            settings::save(&self.root, &self.settings);
        }
        let touched = self.rows(ui, Place::Window);
        let typing = ui.memory(egui::Memory::focused).is_some();
        let now = ui.input(|i| i.time);
        if !self.keyboard_owns && !typing && touched.hovered != self.hovered {
            self.hovered = touched.hovered;
            self.hovered_at = now;
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(super::DWELL));
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
        match touched.waiting {
            Some(Asked::LastStats) => self.empty = Empty::LastStats,
            Some(Asked::Reconnect) => self.reconnect(ui.ctx()),
            None if !self.board.players.is_empty() => self.empty = Empty::Plate,
            None => {}
        }
    }

    pub(super) fn showing_kept(&self) -> bool {
        self.board.players.is_empty() && self.empty == Empty::LastStats && self.board.you.is_some()
    }

    pub(super) fn kept(&self) -> Option<overseer_core::Board> {
        self.board
            .you
            .as_ref()
            .filter(|_| self.showing_kept())
            .map(super::waiting::kept_board)
    }

    fn rows(&self, ui: &mut Ui, place: Place) -> board::Touched {
        let kept;
        let shown = if self.board.players.is_empty() {
            let asked = self.showing_kept() && place == Place::Window;
            let Some(you) = self.board.you.as_ref().filter(|_| asked) else {
                let waiting = empty(
                    ui,
                    (&self.status, &self.board),
                    self.reason(),
                    self.pace(RESTATE),
                    (place == Place::Window)
                        .then_some(self.stopped.is_some() || self.broken.is_some()),
                );
                return board::Touched {
                    waiting,
                    ..board::Touched::default()
                };
            };
            view::note(
                ui,
                &format!(
                    "Your stats from {}. They're from the last match Overseer read, and the board comes back once you're in a lobby. Press Esc to go back.",
                    crate::history::kept_ago(now_ms(), you.at)
                ),
            );
            kept = super::waiting::kept_board(you);
            &kept
        } else {
            &self.board
        };
        let now = ui.input(|i| i.time);
        let scene = Scene {
            board: shown,
            sort: &self.sort,
            selected: self.selected.as_deref(),
            notes: &self.notes,
            hidden: &self.settings.hidden_columns,
            enemies_first: self.settings.enemies_first,
            place,
            still: self.quality() == Quality::Efficient,
            since: (now - self.roster_at) as f32,
            lost: self.lost_at.is_some(),
        };
        board::draw(ui, &scene)
    }
}

/// The time now, in milliseconds since 1970.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}
