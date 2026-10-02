//! The screens the window can show: the board with its detail panel,
//! settings, history, lineups and the overlay's own board.

use egui::{CentralPanel, Panel, Rect, Ui};
use overseer_core::Status;

use crate::board::{self, Place, Scene, Side};
use crate::notes;
use crate::overlay;
use crate::settings::{self, Quality};
use crate::{panel, view};
use overseer_ui::{self, colour, motion, space};

use super::waiting::empty;
use super::{DWELL, GREETING, Overseer, RESTATE, Screen, panel_width};

impl Overseer {
    /// The detail panel down the right: one surface, lifted off the board.
    /// Returns where it went.
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
                // A dark stroke and a light one, the same cut that separates
                // the rows, so the panel reads as standing beside the board.
                ui.painter().vline(
                    all.left() + 1.0,
                    all.y_range(),
                    (1.0, colour::TEXT_STRONG.gamma_multiply(0.07)),
                );
                // The subject changes whenever the pointer crosses a row, and
                // snapping between people reads as flicker. So the old one
                // fades out before the new one fades in.
                let wanted = self.current().and_then(|p| p.puuid.clone());
                let settled = ui.ctx().animate_value_with_time(
                    egui::Id::new("panel-subject"),
                    f32::from(wanted == self.panel_showing),
                    self.pace(motion::QUICK) / 2.0,
                );
                if settled <= 0.02 && wanted != self.panel_showing {
                    self.panel_showing = wanted;
                }
                ui.multiply_opacity(0.92f32.mul_add(settled, 0.08));
                ui.add_space(space::MD);
                // Taken out of self, because the panel edits the notes while
                // it reads the player from the same struct.
                let mut lent = std::mem::take(&mut self.notes);
                let showing = self.panel_showing.as_ref().and_then(|id| {
                    self.board
                        .players
                        .iter()
                        .find(|p| p.puuid.as_ref() == Some(id))
                });
                let side = showing.map_or(Side::Enemy, |p| board::side_of(&self.board, p));
                let still = self.quality() == Quality::Efficient;
                if panel::show(ui, showing, side, &mut lent, &self.career, still) {
                    notes::save(&self.root, &lent);
                }
                self.notes = lent;
            })
            .response
            .rect
    }

    /// Every switch there is, and what could not be switched on.
    fn settings_screen(&mut self, ui: &mut Ui, chrome: egui::Frame, turning: f32) {
        // Read out before the screen borrows the settings. The screen shows
        // why the hotkey or tray failed, or a dead key would give no clue.
        let quality = self.quality();
        let dropped = self.budget.dropped;
        let no_hotkey = self.hotkey.as_ref().err().cloned();
        let no_tray = self.tray.as_ref().err().cloned();
        let trouble = view::Trouble {
            hotkey: no_hotkey.as_ref(),
            tray: no_tray.as_deref(),
        };
        let mut changed = false;
        CentralPanel::default().frame(chrome).show(ui, |ui| {
            ui.multiply_opacity(0.90f32.mul_add(turning, 0.10));
            changed = view::settings(
                ui,
                &mut self.settings,
                quality,
                dropped,
                (trouble, &self.root, &mut self.offline),
            );
        });
        if changed {
            settings::save(&self.root, &self.settings);
        }
        let now = ui.input(|i| i.time);
        self.offline.ask(&self.bridge, now);
        // Asked again every few seconds, so the switch follows the proxy.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
    }

    /// Draws whichever screen is showing in place of the board. True when
    /// one did, since the board and its panel are then not drawn at all.
    pub(super) fn other_screen(&mut self, ui: &mut Ui, chrome: egui::Frame, turning: f32) -> bool {
        let live = self.status == Status::Live;
        match self.screen_showing {
            Screen::Board => return false,
            Screen::Settings => self.settings_screen(ui, chrome, turning),
            Screen::History => self.history_screen(ui, chrome, turning),
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
                        },
                    ),
                );
            }
        }
        self.panel_visible = false;
        true
    }

    /// Your past games, with the count to show at the top.
    fn history_screen(&mut self, ui: &mut Ui, chrome: egui::Frame, turning: f32) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        let live = self.status == Status::Live;
        let mut picked = None;
        CentralPanel::default().frame(chrome).show(ui, |ui| {
            ui.multiply_opacity(0.90f32.mul_add(turning, 0.10));
            picked = self
                .history
                .show(ui, self.settings.history_games, now_ms, live);
        });
        if let Some(count) = picked {
            self.settings.history_games = count;
            settings::save(&self.root, &self.settings);
            self.history.ask(&self.bridge, count, live);
        }
    }

    /// The overlay's contents and the height they took: the enemy rows, or
    /// one line while there are none. The window's empty state is too big to
    /// sit over a game.
    fn overlay_view(&self, ui: &mut Ui) -> f32 {
        let enemies = self
            .board
            .players
            .iter()
            .any(|p| board::side_of(&self.board, p) == Side::Enemy);
        if !enemies {
            return overlay::greeting(ui);
        }
        // No session foot here. It belongs in the window, and the overlay is
        // exactly as tall as its rows.
        self.rows(ui, Place::Overlay).drew
    }

    /// The overlay's window, when there is something for it to show.
    pub(super) fn overlay(&mut self, ui: &Ui, now: f64) {
        // Only while there is an enemy to show, and for `GREETING` seconds
        // after being switched on so that switching it on is seen to work.
        let enemies = self
            .board
            .players
            .iter()
            .any(|p| board::side_of(&self.board, p) == Side::Enemy);
        if self.overlay_on_at == Some(f64::NEG_INFINITY) {
            self.overlay_on_at = Some(now);
        }
        let greeting = self
            .overlay_on_at
            .map_or(0.0, |at| GREETING - (now - at))
            .max(0.0);
        if greeting > 0.0 && !enemies {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(greeting));
        }
        if !(enemies || greeting > 0.0) {
            self.overlay_drew = None;
        }
        if self.settings.overlay.on && (enemies || greeting > 0.0) {
            // Drawn before this window, so a frame where the board changed
            // reaches both. Nothing in it takes a click.
            let last = self.overlay_drew.unwrap_or(overlay::DESIGNED);
            let mut drew = last;
            {
                let this = &*self;
                overlay::show(ui.ctx(), this.settings.overlay.corner, last, |ui| {
                    drew = this.overlay_view(ui);
                });
            }
            // Sized to what the board really drew, which cannot go stale the
            // way a height kept by hand can.
            let drew = drew.min(overlay::CEILING);
            if (drew - last).abs() > 0.5 {
                self.overlay_drew = Some(drew);
                ui.ctx().request_repaint();
            }
        }
    }

    /// The board, both teams, with the headings each side needs.
    pub(super) fn board_view(&mut self, ui: &mut Ui) {
        let touched = self.rows(ui, Place::Window);
        let typing = ui.memory(egui::Memory::focused).is_some();
        // Read a frame late, which nobody can see: the panel is drawn before
        // the board, so what the pointer was on last frame is what the panel
        // shows this one.
        let now = ui.input(|i| i.time);
        if !self.keyboard_owns && !typing && touched.hovered != self.hovered {
            self.hovered = touched.hovered;
            self.hovered_at = now;
        }
        // A row the pointer rests on becomes the selection, so the panel
        // keeps showing it while the pointer crosses over to read it.
        if let Some(id) = self.hovered.as_ref()
            && now - self.hovered_at >= DWELL
            && self.selected.as_ref() != Some(id)
        {
            self.selected = Some(id.clone());
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

    /// Draws the board, or what to say instead. Read only, so the overlay can
    /// call it too without the two windows disagreeing about the selection.
    fn rows(&self, ui: &mut Ui, place: Place) -> board::Touched {
        if self.board.players.is_empty() {
            empty(ui, &self.status, self.reason(), self.pace(RESTATE));
            return board::Touched::default();
        }
        let now = ui.input(|i| i.time);
        let scene = Scene {
            board: &self.board,
            sort: &self.sort,
            // The overlay has no search box, so the window's filter would
            // only make it look broken.
            filter: if place == Place::Window {
                &self.search.filter
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
