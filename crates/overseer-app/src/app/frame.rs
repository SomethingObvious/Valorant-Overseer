use egui::{CentralPanel, Panel, Ui};
use overseer_core::Status;

use crate::board;
use crate::header;
use crate::settings::{self, Quality};
use overseer_ui::{self, colour, motion, space};

use super::{COMPACT, DWELL, Overseer, Screen, panel_grip};

impl Overseer {
    pub(super) fn window(&mut self, ui: &mut Ui, edge: bool) {
        self.keys(ui);
        if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            self.keyboard_owns = false;
        }
        let now = ui.input(|i| i.time);
        if self.hovered.is_some() && now - self.hovered_at < DWELL {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(DWELL));
        }
        // The rows land from a timestamp, not egui's animator, so nothing
        // else asks for these frames.
        if board::settling(
            (now - self.roster_at) as f32,
            self.quality() == Quality::Efficient,
        ) {
            ui.ctx().request_repaint();
        }

        self.appear(ui, now);

        let chrome = egui::Frame::NONE.fill(colour::BG);
        let (pressed, refresh) = Panel::top("header")
            .exact_size(header::HEIGHT)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| self.header(ui, edge))
            .inner;
        let clicked = Panel::bottom("footer")
            .exact_size(space::XXL)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| self.footer(ui))
            .inner;
        if let Some(pressed) = pressed {
            self.chrome(ui.ctx(), pressed);
        }
        if refresh && self.status == Status::Live {
            let _asked = self.bridge.ask("refresh", serde_json::Value::Null);
        }
        if let Some(hint) = clicked {
            self.perform(ui.ctx(), hint);
        }

        let turning = ui.ctx().animate_value_with_time(
            egui::Id::new("screen"),
            f32::from(self.screen == self.screen_showing),
            self.pace(motion::QUICK) / 2.0,
        );
        if turning <= 0.02 && self.screen != self.screen_showing {
            if self.screen_showing == Screen::Lineups {
                self.lineups.leave();
            }
            self.screen_showing = self.screen;
        }
        if self.other_screen(ui, chrome, turning) {
            return;
        }

        let width = ui.available_width();
        let was = self.panel_visible;
        self.panel_visible = width >= COMPACT
            && self.settings.panel
            && (!self.board.players.is_empty() || self.showing_kept());
        if self.panel_visible && !was {
            ui.ctx().request_repaint();
        }
        let panel = self.panel_visible.then(|| self.detail(ui, width));
        CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let all = ui.max_rect();
                ui.painter().add(egui::Shape::gradient_rect(
                    all,
                    egui::Direction::TopDown,
                    [colour::BG, colour::VOID],
                ));
                ui.multiply_opacity(0.90f32.mul_add(turning, 0.10));
                self.board_view(ui);
            });
        if let Some(panel) = panel
            && panel_grip(ui, panel, width, &mut self.settings.panel_width)
        {
            settings::save(&self.root, &self.settings);
        }
    }
}
