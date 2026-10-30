//! The status line and hints along the bottom of the window.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use crate::board::GUTTER;
use crate::panel;
use crate::settings::Quality;
use overseer_ui::{self, Face, caps_text, colour, size, space};

use super::{Hint, Overseer, SAID, Screen, VERSION};

impl Overseer {
    /// A newer release, at the footer's far right, when there is one: quiet,
    /// since it waits for whenever suits. A click opens its page, and
    /// Never Show Again beside it stops the daily look. Returns where the
    /// rest of the footer has to stop.
    fn newer(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        rect: Rect,
        clicked: &mut Option<Hint>,
    ) -> f32 {
        let mut right = rect.right() - GUTTER;
        let newest = crate::update::newer(&self.settings.updates, VERSION);
        let notice = newest
            .as_ref()
            .map(|n| format!("Version {n} Is Out"))
            .unwrap_or_default();
        let now = ui.input(|i| i.time);
        let armed = self
            .quieting
            .is_some_and(|at| now - at < crate::lineups::ARMED);
        // Said here as well as in Settings, since without the key the overlay
        // can't be brought up in game at all.
        let clash = matches!(self.hotkey, Some(Err(_))).then_some((
            "Overlay Key Unavailable",
            Hint::OverlayKey,
            colour::WARN,
            "Windows wouldn't give Overseer the overlay's key, often because another program has it. Click to pick another.",
        ));
        let release = newest.is_some().then_some([
            (
                if armed {
                    "Click Again to Stop Checking"
                } else {
                    "Never Show Again"
                },
                Hint::Quiet,
                colour::TEXT_FAINT,
                "Stops the daily look for updates. Settings turns it back on.",
            ),
            (
                notice.as_str(),
                Hint::Release,
                colour::INFO,
                "Opens its page. Its setup updates this install and keeps your lineups, notes and settings.",
            ),
        ]);
        let items = clash.into_iter().chain(release.into_iter().flatten());
        for (text, hint, tint, about) in items {
            let font = Face::Display.at(size::MICRO);
            let wide = overseer_ui::caps_width(painter, text, font.clone());
            let area = Rect::from_min_max(
                pos2(right - wide - space::SM, rect.top() + 2.0),
                pos2(right + space::SM, rect.bottom()),
            );
            let hit = ui
                .interact(area, ui.id().with(("newer-release", text)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(about);
            hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
            if hit.hovered() {
                painter.rect_filled(area, 0, colour::BG_HOVER);
            }
            if hit.clicked() {
                *clicked = Some(hint);
            }
            let drawn = caps_text(
                painter,
                pos2(right, rect.center().y),
                Align2::RIGHT_CENTER,
                text,
                font,
                if hit.hovered() {
                    colour::TEXT_STRONG
                } else {
                    tint
                },
            );
            right = drawn.left() - space::LG;
        }
        right - space::SM
    }

    /// The footer's right half: what was just copied, and what the window
    /// is spending. Returns where the hints have to stop.
    fn status(&self, ui: &Ui, painter: &egui::Painter, rect: Rect, right: f32) -> f32 {
        // Drawn before the hints, which stop where this starts, so a narrow
        // window drops hints instead of printing them over this.
        let (bad, _why) = &self.unreadable;
        let mut right = right;
        // The tier only when it is efficient, the one state worth telling.
        let tier = if self.quality() == Quality::Efficient {
            "Efficient".to_owned()
        } else {
            String::new()
        };
        let now = ui.input(|i| i.time);
        let copied = panel::copied(ui.ctx())
            .map(|name| format!("Copied {name}"))
            .unwrap_or_default();
        let said = self
            .said
            .as_ref()
            .filter(|(_, at)| now - at < SAID)
            .map(|(words, _)| words.clone())
            .unwrap_or_default();
        for (text, tint) in [
            (said, colour::TEXT),
            (copied, colour::ALLY),
            (tier, colour::TEXT_FAINT),
            (
                if *bad == 0 {
                    String::new()
                } else {
                    "May Need a Repair".to_owned()
                },
                colour::WARN,
            ),
        ] {
            if text.is_empty() {
                continue;
            }
            let drawn = caps_text(
                painter,
                pos2(right, rect.center().y),
                Align2::RIGHT_CENTER,
                &text,
                Face::Display.at(size::MICRO),
                tint,
            );
            if tint == colour::WARN {
                let why = format!(
                    "{bad} updates from the backend couldn't be read. If the board stops changing, run install.bat in Overseer's folder to repair it. {}",
                    self.unreadable.1.as_deref().unwrap_or_default()
                );
                let _why = ui
                    .interact(drawn, ui.id().with("unreadable"), Sense::hover())
                    .on_hover_text(why);
            }
            right = drawn.left() - space::XL;
        }
        right
    }

    /// The keys the footer offers. The ones that act on players only show
    /// on the board with somebody on it, and Help is always there, since it
    /// lists the rest.
    fn hints(&self) -> impl Iterator<Item = (&'static str, &'static str, Hint)> + use<'_> {
        [
            ("?", "Help", Hint::Help),
            ("w", "Worth a Look", Hint::Worth),
            ("arrows", "Pick", Hint::Pick),
            ("n", "Note", Hint::Note),
            ("ctrl c", "Copy Name", Hint::Copy),
            ("o", "Overlay", Hint::Overlay),
            ("h", "History", Hint::History),
            ("l", "Lineups", Hint::Lineups),
            (",", "Settings", Hint::Settings),
        ]
        .into_iter()
        .filter(|(_, _, hint)| {
            (self.screen == Screen::Board && !self.board.players.is_empty())
                || !matches!(hint, Hint::Worth | Hint::Pick | Hint::Note | Hint::Copy)
        })
    }

    /// The footer: the keys as keycaps, which are quicker to recognise than
    /// prose, and what the window is spending. Each hint is also a button
    /// for its key, and this says which was clicked.
    pub(super) fn footer(&self, ui: &mut Ui) -> Option<Hint> {
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::XXL), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return None;
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

        let mut clicked = None;
        let right = self.newer(ui, &painter, rect, &mut clicked);
        let right = self.status(ui, &painter, rect, right);

        let mut x = rect.left() + GUTTER;
        for (key, what, hint) in self.hints() {
            let font = Face::Display.at(size::MICRO);
            let wide = overseer_ui::keycap_width(&painter, key)
                + space::SM
                + overseer_ui::caps_width(&painter, what, font.clone());
            if x + wide > right {
                break;
            }
            let area = Rect::from_min_max(
                pos2(x - space::SM, rect.top() + 2.0),
                pos2(x + wide + space::SM, rect.bottom()),
            );
            let hit = ui
                .interact(area, ui.id().with(("hint", key)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            hit.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{what}, {key}"))
            });
            if hit.hovered() {
                painter.rect_filled(area, 0, colour::BG_HOVER);
            }
            if hit.clicked() {
                clicked = Some(hint);
            }
            x = overseer_ui::keycap(&painter, pos2(x, rect.center().y), key).right() + space::SM;
            let after = caps_text(
                &painter,
                pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                what,
                font,
                if hit.hovered() {
                    colour::TEXT_STRONG
                } else {
                    colour::TEXT_FAINT
                },
            );
            x = after.right() + space::XL;
        }
        clicked
    }
}
