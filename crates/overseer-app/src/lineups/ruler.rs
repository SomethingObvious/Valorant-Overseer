//! A ruler over the map, for how far apart two places are. It keeps nothing,
//! so what it measures never ends up in a lineup or a drawing.

use egui::{Rect, Response, Stroke, Ui, vec2};
use overseer_ui::{Face, colour, size, space};

use super::shapes::{self, Square};
use crate::controls;

pub(super) const LABEL: &str = "Ruler (M)";

/// How much of the map's bottom edge the switch takes, kept clear of the
/// prompt.
pub(super) const ROOM: f32 = 100.0;

#[derive(Debug, Default)]
pub(super) struct Ruler {
    pub(super) on: bool,
    line: Option<[[f32; 2]; 2]>,
}

impl Ruler {
    #[cfg(test)]
    pub(super) const fn line(&self) -> Option<[[f32; 2]; 2]> {
        self.line
    }

    pub(super) const fn toggle(&mut self) {
        self.on = !self.on;
        self.line = None;
    }

    /// Draws the ruler and its switch, and takes the map's drags while it
    /// is on, which is when this is true. A map with no scale has none.
    pub(super) fn over(
        &mut self,
        ui: &mut Ui,
        (painter, square): (&egui::Painter, Square),
        response: &Response,
        scale: Option<f32>,
    ) -> bool {
        let Some(scale) = scale else {
            return false;
        };
        self.paint(painter, square, scale);
        self.switch(ui, square);
        if self.on {
            self.follow(square, response);
        }
        self.on
    }

    /// The switch, in the map's bottom right corner.
    fn switch(&mut self, ui: &mut Ui, square: Square) {
        let wide = controls::chip_width(ui, LABEL);
        let high = space::ROW;
        let rect = Rect::from_min_size(
            square.right_bottom() - vec2(wide + space::MD, high + space::MD),
            vec2(wide, high),
        );
        let clicked = ui
            .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                controls::chip(ui, LABEL, self.on).clicked()
            })
            .inner;
        if clicked {
            self.toggle();
        }
    }

    /// While on, a drag across the map lays the ruler down and a click
    /// takes it off.
    fn follow(&mut self, square: Square, response: &Response) {
        let (origin, at) = response
            .ctx
            .input(|i| (i.pointer.press_origin(), i.pointer.interact_pos()));
        if response.drag_started()
            && let (Some(origin), Some(at)) = (origin, at)
        {
            self.line = Some([shapes::spot(square, origin), shapes::spot(square, at)]);
        } else if response.dragged()
            && let (Some(line), Some(at)) = (self.line.as_mut(), at)
        {
            line[1] = shapes::spot(square, at);
        } else if response.clicked() {
            self.line = None;
        }
    }

    fn paint(&self, painter: &egui::Painter, square: Square, scale: f32) {
        let Some([a, b]) = self.line else {
            return;
        };
        let (from, to) = (shapes::at(square, a), shapes::at(square, b));
        painter.line_segment([from, to], Stroke::new(4.0, colour::VOID));
        painter.line_segment([from, to], Stroke::new(2.0, colour::TEXT_STRONG));
        for end in [from, to] {
            painter.circle_filled(end, 4.0, colour::TEXT_STRONG);
            painter.circle_stroke(end, 4.0, Stroke::new(1.5, colour::VOID));
        }
        let galley = painter.layout_no_wrap(
            said(metres(scale, a, b)),
            Face::Heavy.at(size::BODY + 1.0),
            colour::TEXT_STRONG,
        );
        let plate = Rect::from_center_size(
            from.lerp(to, 0.5),
            galley.size() + vec2(space::LG, space::SM),
        );
        painter.rect_filled(plate, 0, colour::VOID.gamma_multiply(0.9));
        painter.galley(
            plate.min + vec2(space::LG, space::SM) / 2.0,
            galley,
            colour::TEXT_STRONG,
        );
    }
}

/// How many metres apart two places on the map are. `scale` is how much of
/// the map's width one game unit is, and a game unit is a centimetre.
fn metres(scale: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    (b[0] - a[0]).hypot(b[1] - a[1]) / scale.max(f32::EPSILON) / 100.0
}

fn said(metres: f32) -> String {
    if metres < 10.0 {
        format!("{metres:.1} m")
    } else {
        format!("{metres:.0} m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_width_of_game_units_reads_in_metres() {
        // Ascent's xMultiplier from valorant-api, so its minimap is 143 m across.
        let scale = 0.000_07;
        assert_eq!(said(metres(scale, [0.0, 0.0], [1.0, 0.0])), "143 m");
        assert_eq!(said(metres(scale, [0.2, 0.2], [0.2, 0.25])), "7.1 m");
    }
}
