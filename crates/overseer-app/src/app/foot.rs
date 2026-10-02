//! The search row along the top of the board and the status line and
//! hints along the bottom of the window.

use egui::{Align2, Rect, RichText, Sense, Ui, pos2, vec2};

use crate::board::{self, GUTTER};
use crate::panel;
use crate::settings::Quality;
use overseer_ui::{self, Face, caps_text, colour, size, space};

use super::{Hint, Overseer, SAID};

impl Overseer {
    /// The search row, and what the board is sorted by. The box is a real
    /// text field, because a painted one gets the caret and selection wrong.
    pub(super) fn search(&mut self, ui: &mut Ui) {
        let bar = ui.max_rect();
        let middle = bar.center().y;
        let painter = ui.painter().clone();
        let label = caps_text(
            &painter,
            pos2(bar.left() + GUTTER, middle),
            Align2::LEFT_CENTER,
            "Find",
            Face::Display.at(size::LABEL),
            colour::TEXT_DIM,
        );
        // A slot in the board's own slant, not egui's outlined field, which
        // looks like a web form.
        let width = (bar.width() * 0.3).clamp(160.0, 320.0);
        let field = Rect::from_min_size(
            pos2(label.right() + space::MD, middle - 12.0),
            vec2(width, 24.0),
        );
        let focused = ui.memory(|m| m.has_focus(egui::Id::new("search-field")));
        painter.add(board::paint::slant(field, false, true, colour::BG_INSET));
        if focused {
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(field.left(), field.bottom() - 2.0),
                    vec2(width - 6.0, 2.0),
                ),
                0,
                colour::TEXT_STRONG,
            );
        }
        let response = ui.put(
            field.shrink2(vec2(space::MD, 2.0)),
            egui::TextEdit::singleline(&mut self.search.filter)
                .id(egui::Id::new("search-field"))
                .frame(egui::Frame::NONE)
                .vertical_align(egui::Align::Center)
                .font(Face::Body.at(size::BODY))
                .hint_text(
                    RichText::new("A name or an agent")
                        .color(colour::HINT)
                        .font(Face::Body.at(size::BODY)),
                ),
        );
        if self.search.focus {
            response.request_focus();
            self.search.focus = false;
        }
        self.found(
            &painter,
            pos2(field.right() + space::LG, middle),
            response.changed(),
        );
        if let Some(column) = self.sort.column.as_deref() {
            let _sorted = caps_text(
                &painter,
                pos2(bar.right() - GUTTER, middle),
                Align2::RIGHT_CENTER,
                &format!("Sorted by {column}"),
                Face::Display.at(size::MICRO),
                colour::TEXT_FAINT,
            );
        }
    }

    /// How many the search found. A change to the text selects the first
    /// match, unless the selection is already one of them.
    fn found(&mut self, painter: &egui::Painter, at: egui::Pos2, changed: bool) {
        if self.search.filter.trim().is_empty() {
            return;
        }
        let found = self.visible_order();
        if changed
            && let Some(first) = found.first()
            && !found.iter().any(|id| Some(id) == self.selected.as_ref())
        {
            self.selected = Some(first.clone());
            self.keyboard_owns = true;
        }
        let count = match found.len() {
            0 => "Nobody".to_owned(),
            1 => "1 match".to_owned(),
            n => format!("{n} matches"),
        };
        let drawn = caps_text(
            painter,
            at,
            Align2::LEFT_CENTER,
            &count,
            Face::Display.at(size::LABEL),
            if found.is_empty() {
                colour::WARN
            } else {
                colour::TEXT
            },
        );
        let cap = overseer_ui::keycap(painter, pos2(drawn.right() + space::XL, at.y), "esc");
        let _clear = caps_text(
            painter,
            pos2(cap.right() + space::SM, at.y),
            Align2::LEFT_CENTER,
            "Clear",
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }

    /// The footer's right half: what was just copied, and what the window
    /// is spending. Returns where the hints have to stop.
    fn status(&self, ui: &Ui, painter: &egui::Painter, rect: Rect) -> f32 {
        // Drawn before the hints, which stop where this starts, so a narrow
        // window drops hints instead of printing them over this.
        let (bad, _why) = &self.unreadable;
        let mut right = rect.right() - GUTTER;
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
                    format!("{bad} unreadable")
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
            right = drawn.left() - space::XL;
        }
        right
    }

    /// The footer: the keys as keycaps, which are quicker to recognise than
    /// prose, and what the window is spending. Each hint is also a button
    /// for its key, and this says which was clicked.
    pub(super) fn footer(&self, ui: &mut Ui) -> Option<Hint> {
        let (rect, _response) = ui.allocate_exact_size(
            vec2(ui.available_width(), space::XL + space::SM),
            Sense::hover(),
        );
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

        let right = self.status(ui, &painter, rect);

        let mut x = rect.left() + GUTTER;
        let mut clicked = None;
        for (key, what, hint) in [
            ("w", "Worth a Look", Hint::Worth),
            ("arrows", "Pick", Hint::Pick),
            ("/", "Find", Hint::Find),
            ("n", "Note", Hint::Note),
            ("ctrl c", "Copy Name", Hint::Copy),
            ("o", "Overlay", Hint::Overlay),
            ("h", "History", Hint::History),
            ("l", "Lineups", Hint::Lineups),
            (",", "Settings", Hint::Settings),
        ] {
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
