//! Shapes drawn on the map to show a range or an area: rectangles, circles,
//! ovals, cones and triangles, in a dozen colours, labels of a few words,
//! boxes of text and brush strokes. Each is the two points of the drag that
//! drew it, on the minimap, so it scales with the map. A label is the one
//! point it starts at, and a stroke keeps every point it went through.

use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use overseer_core::Drawing;
use overseer_ui::{Face, caps_text, colour};

/// The shapes there are, as kept and as named, in the order the panel shows.
pub(super) const KINDS: [(&str, &str); 9] = [
    ("rect", "Rectangle"),
    ("circle", "Circle"),
    ("oval", "Oval"),
    ("cone", "Cone"),
    ("triangle", "Triangle"),
    ("brush", "Brush"),
    ("text", "Text"),
    ("textbox", "Text Box"),
    // Not a shape. It takes shapes off.
    ("erase", "Eraser"),
];

/// The colours a shape can be, as kept, bright enough to read on the
/// darkened minimap.
pub(crate) const COLOURS: [&str; 12] = [
    "#FF4655", "#FF8A3D", "#FFC845", "#B5E853", "#18E5A7", "#3FD0FF", "#4A7BFF", "#A86CFF",
    "#FF6FCF", "#F2EFE8", "#8B929C", "#7A4A2A",
];

/// A kept colour as one to paint with.
pub(crate) fn ink(hex: &str) -> Color32 {
    overseer_ui::hex(Some(hex)).unwrap_or(colour::WARN)
}

/// How tall a label's letters are, in step with the map.
fn letters(square: Square) -> f32 {
    (square.width() * 0.024).clamp(11.0, 24.0)
}

/// The plate behind a label that starts at `start`, measured by its letters
/// so a right click needs no painter to find it.
fn plate(square: Square, start: Pos2, words: &str) -> Rect {
    let tall = letters(square);
    let wide = words.chars().count() as f32 * tall * 0.55;
    Rect::from_min_size(
        start + vec2(8.0, -tall * 0.75),
        vec2(wide + tall, tall * 1.5),
    )
}

/// The map on screen: the square it fills, and how many quarter turns
/// clockwise the minimap is turned in it. Everything kept on the map stays
/// in the minimap's own frame, so turning it never moves a lineup.
#[derive(Debug, Clone, Copy)]
pub(super) struct Square {
    /// Where it is on screen.
    pub(super) rect: Rect,
    /// Quarter turns clockwise, 0 to 3.
    pub(super) turn: u8,
}

impl std::ops::Deref for Square {
    type Target = Rect;

    fn deref(&self) -> &Rect {
        &self.rect
    }
}

impl From<Rect> for Square {
    fn from(rect: Rect) -> Self {
        Self { rect, turn: 0 }
    }
}

/// A point on a square from 0 to 1 each way, turned `turn` quarter turns
/// clockwise about its middle.
pub(super) const fn turned([x, y]: [f32; 2], turn: u8) -> [f32; 2] {
    match turn % 4 {
        1 => [1.0 - y, x],
        2 => [1.0 - x, 1.0 - y],
        3 => [y, 1.0 - x],
        _ => [x, y],
    }
}

/// Where a point on the minimap is on screen, turned with the map.
pub(super) fn at(square: Square, spot: [f32; 2]) -> Pos2 {
    let [x, y] = turned(spot, square.turn);
    square.min + vec2(x, y) * square.width()
}

/// Draws one shape, faint inside and solid round its edge, both as solid as
/// its opacity.
pub(super) fn paint(painter: &egui::Painter, square: Square, drawing: &Drawing) {
    let tint = ink(&drawing.colour).gamma_multiply(drawing.opacity.clamp(0.1, 1.0));
    let fill = tint.gamma_multiply(0.24);
    let edge = Stroke::new(2.0, tint);
    let (a, b) = (at(square, drawing.a), at(square, drawing.b));
    match drawing.kind.as_str() {
        "rect" => {
            painter.rect(
                Rect::from_two_pos(a, b),
                0,
                fill,
                edge,
                egui::StrokeKind::Middle,
            );
        }
        "circle" => {
            painter.circle(a, a.distance(b), fill, edge);
        }
        "oval" => {
            let bounds = Rect::from_two_pos(a, b);
            let radius = bounds.size() / 2.0;
            painter.add(Shape::ellipse_filled(bounds.center(), radius, fill));
            painter.add(Shape::ellipse_stroke(bounds.center(), radius, edge));
        }
        "cone" => {
            painter.add(Shape::convex_polygon(
                cone(a, b, drawing.spread),
                fill,
                edge,
            ));
        }
        "triangle" => {
            painter.add(Shape::convex_polygon(triangle(a, b).to_vec(), fill, edge));
        }
        "brush" => {
            let thick = (drawing.width * square.width()).max(1.0);
            let line: Vec<Pos2> = drawing.points.iter().map(|&p| at(square, p)).collect();
            painter.add(Shape::line(line, Stroke::new(thick, tint)));
        }
        "textbox" => {
            let bounds = Rect::from_two_pos(a, b);
            let shade = drawing.opacity.clamp(0.1, 1.0) * 0.8;
            painter.rect(
                bounds,
                0,
                colour::VOID.gamma_multiply(shade),
                Stroke::new(1.5, tint),
                egui::StrokeKind::Inside,
            );
            let pad = letters(square) * 0.4;
            let galley = painter.layout(
                drawing.text.clone(),
                Face::Body.at((letters(square) * 0.75).max(11.0)),
                tint,
                (bounds.width() - pad * 2.0).max(1.0),
            );
            // Words past the bottom are cut off, and dragging a corner shows
            // them again.
            painter.with_clip_rect(bounds.shrink(1.0)).galley(
                bounds.min + vec2(pad, pad),
                galley,
                tint,
            );
        }
        "text" => {
            let back = plate(square, a, &drawing.text);
            let shade = drawing.opacity.clamp(0.1, 1.0) * 0.8;
            painter.rect_filled(back, 0, colour::VOID.gamma_multiply(shade));
            let _label = caps_text(
                painter,
                pos2(back.left() + letters(square) / 2.0, back.center().y),
                egui::Align2::LEFT_CENTER,
                &drawing.text,
                Face::Display.at(letters(square)),
                tint,
            );
        }
        _ => {}
    }
}

/// A cone's outline: its tip, then an arc as far away as `end`, opening
/// `spread` degrees either side of the line to it.
fn cone(tip: Pos2, end: Pos2, spread: f32) -> Vec<Pos2> {
    let reach = tip.distance(end);
    let facing = (end - tip).angle();
    let half = spread.clamp(5.0, 180.0).to_radians() / 2.0;
    let steps = 24;
    std::iter::once(tip)
        .chain((0..=steps).map(|i| {
            let turn = facing - half + 2.0 * half * i as f32 / steps as f32;
            tip + reach * Vec2::angled(turn)
        }))
        .collect()
}

/// An even triangle round `centre` with one corner at `corner`, so the drag
/// that draws it also turns it.
fn triangle(centre: Pos2, corner: Pos2) -> [Pos2; 3] {
    let arm = corner - centre;
    let turn = |degrees: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        centre
            + vec2(
                arm.y.mul_add(-sin, arm.x * cos),
                arm.y.mul_add(cos, arm.x * sin),
            )
    };
    [corner, turn(120.0), turn(240.0)]
}

/// Whether `spot` is on a shape, for the right click that removes it.
pub(super) fn hit(square: Square, drawing: &Drawing, spot: Pos2) -> bool {
    let (a, b) = (at(square, drawing.a), at(square, drawing.b));
    match drawing.kind.as_str() {
        "rect" | "textbox" => Rect::from_two_pos(a, b).expand(3.0).contains(spot),
        "brush" => {
            let reach = (drawing.width * square.width()).max(1.0) / 2.0 + 4.0;
            drawing.points.windows(2).any(|pair| match pair {
                [one, two] => near_segment(spot, at(square, *one), at(square, *two)) <= reach,
                _ => false,
            })
        }
        "circle" => spot.distance(a) <= a.distance(b) + 3.0,
        "oval" => {
            let bounds = Rect::from_two_pos(a, b).expand(3.0);
            let radius = bounds.size() / 2.0;
            let off = spot - bounds.center();
            radius.x > 0.0
                && radius.y > 0.0
                && (off.y / radius.y).mul_add(off.y / radius.y, (off.x / radius.x).powi(2)) <= 1.0
        }
        "cone" => {
            let reach = a.distance(b);
            let off = spot - a;
            let apart = (off.angle() - (b - a).angle() + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            off.length() <= reach + 3.0 && apart.abs() <= drawing.spread.to_radians() / 2.0
        }
        "triangle" => {
            let [one, two, three] = triangle(a, b);
            let side = |from: Pos2, to: Pos2| {
                (to - from)
                    .y
                    .mul_add(-(spot - from).x, (to - from).x * (spot - from).y)
            };
            let turns = [side(one, two), side(two, three), side(three, one)];
            turns.iter().all(|t| *t >= 0.0) || turns.iter().all(|t| *t <= 0.0)
        }
        "text" => plate(square, a, &drawing.text).expand(3.0).contains(spot),
        _ => false,
    }
}

/// How far `spot` is from the segment between `one` and `two`.
fn near_segment(spot: Pos2, one: Pos2, two: Pos2) -> f32 {
    let along = two - one;
    let length = along.length_sq();
    if length <= f32::EPSILON {
        return spot.distance(one);
    }
    let share = ((spot - one).dot(along) / length).clamp(0.0, 1.0);
    spot.distance(one + along * share)
}

/// Where a shape's dot sits, the one that drags it: the middle of a
/// rectangle or an oval, a text box's top left corner so it isn't on the
/// words, and where the drag that drew anything else began.
pub(super) fn handle(square: Square, drawing: &Drawing) -> Pos2 {
    let (a, b) = (at(square, drawing.a), at(square, drawing.b));
    match drawing.kind.as_str() {
        "rect" | "oval" => a.lerp(b, 0.5),
        "textbox" => Rect::from_two_pos(a, b).min,
        _ => a,
    }
}

/// The grips that resize a shape, on the minimap: the four corners of a
/// rectangle or an oval, and for a circle, a cone or a triangle the end the
/// drag that drew it set, which gives its size and which way it faces. A
/// label has none, since its letters go with the map, and a stroke is
/// resized by its brush size instead.
pub(super) fn grips(drawing: &Drawing) -> Vec<[f32; 2]> {
    let ([ax, ay], [bx, by]) = (drawing.a, drawing.b);
    match drawing.kind.as_str() {
        "rect" | "oval" | "textbox" => vec![[ax, ay], [bx, ay], [bx, by], [ax, by]],
        "circle" | "cone" | "triangle" => vec![drawing.b],
        _ => Vec::new(),
    }
}

/// `drawing` with its grip `grip` dragged to `to`. A corner of a rectangle
/// or an oval moves while the one across from it stays put.
pub(super) fn resized(drawing: &Drawing, grip: usize, to: [f32; 2]) -> Drawing {
    let corners = grips(drawing);
    match drawing.kind.as_str() {
        "rect" | "oval" | "textbox" => Drawing {
            a: corners.get((grip + 2) % 4).copied().unwrap_or(drawing.a),
            b: to,
            ..drawing.clone()
        },
        _ => Drawing {
            b: to,
            ..drawing.clone()
        },
    }
}

/// Where a grip from [`grips`] is on screen.
pub(super) fn grip_at(square: Square, spot: [f32; 2]) -> Pos2 {
    at(square, spot)
}

/// `drawing` moved by `by` on the minimap, held so no part of it leaves the
/// map rather than squashing against the edge.
pub(super) fn moved(drawing: &Drawing, by: [f32; 2]) -> Drawing {
    let every = || {
        [drawing.a, drawing.b]
            .into_iter()
            .chain(drawing.points.iter().copied())
    };
    let hold = |axis: usize| {
        let along = every().map(|p| p.get(axis).copied().unwrap_or_default());
        let (low, high) = along.fold((1.0_f32, 0.0_f32), |(l, h), v| (l.min(v), h.max(v)));
        let push = by.get(axis).copied().unwrap_or_default();
        push.clamp(-low, 1.0 - high)
    };
    let (dx, dy) = (hold(0), hold(1));
    let shift = |[x, y]: [f32; 2]| [x + dx, y + dy];
    Drawing {
        a: shift(drawing.a),
        b: shift(drawing.b),
        points: drawing.points.iter().map(|&p| shift(p)).collect(),
        ..drawing.clone()
    }
}

/// A point on the minimap from one on screen, turned back to the minimap's
/// own frame.
pub(super) fn spot(square: Square, at: Pos2) -> [f32; 2] {
    let shown = [
        ((at.x - square.left()) / square.width()).clamp(0.0, 1.0),
        ((at.y - square.top()) / square.height()).clamp(0.0, 1.0),
    ];
    turned(shown, 4 - square.turn % 4)
}

/// Whether a drag went far enough to be a shape rather than a slip.
pub(super) fn long_enough(square: Square, start: [f32; 2], end: [f32; 2]) -> bool {
    at(square, start).distance(at(square, end)) >= 6.0
}

#[cfg(test)]
mod tests {
    use super::{COLOURS, Square, grips, handle, hit, ink, moved, resized, spot, triangle};
    use egui::{Rect, pos2, vec2};
    use overseer_core::Drawing;

    fn shape(kind: &str, a: [f32; 2], b: [f32; 2]) -> Drawing {
        Drawing {
            kind: kind.to_owned(),
            colour: "#FF4655".to_owned(),
            a,
            b,
            spread: 60.0,
            opacity: 1.0,
            text: "Stack here".to_owned(),
            ..Drawing::default()
        }
    }

    /// A right click inside each kind of shape finds it, and one outside
    /// doesn't, so removing a shape never takes the wrong one.
    #[test]
    fn every_shape_is_hit_where_it_is_drawn() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)));
        for (kind, a, b, on, off) in [
            (
                "rect",
                [0.1, 0.1],
                [0.3, 0.2],
                pos2(20.0, 15.0),
                pos2(50.0, 50.0),
            ),
            (
                "circle",
                [0.5, 0.5],
                [0.6, 0.5],
                pos2(55.0, 52.0),
                pos2(70.0, 50.0),
            ),
            (
                "oval",
                [0.1, 0.4],
                [0.5, 0.5],
                pos2(30.0, 45.0),
                pos2(10.0, 40.0),
            ),
            (
                "cone",
                [0.5, 0.5],
                [0.9, 0.5],
                pos2(80.0, 55.0),
                pos2(40.0, 50.0),
            ),
            (
                "text",
                [0.2, 0.2],
                [0.2, 0.2],
                pos2(35.0, 20.0),
                pos2(15.0, 20.0),
            ),
            (
                "triangle",
                [0.5, 0.5],
                [0.5, 0.3],
                pos2(50.0, 45.0),
                pos2(80.0, 80.0),
            ),
        ] {
            let drawn = shape(kind, a, b);
            assert!(hit(square, &drawn, on), "{kind} missed where it is");
            assert!(!hit(square, &drawn, off), "{kind} hit where it isn't");
        }
    }

    /// A shape moves whole, and one pushed at the edge stops there with
    /// its size kept.
    #[test]
    fn a_shape_moves_whole_and_stays_on_the_map() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)));
        let rect = shape("rect", [0.1, 0.2], [0.3, 0.4]);
        assert_eq!(handle(square, &rect), pos2(20.0, 30.0));
        assert_eq!(
            handle(square, &shape("cone", [0.5, 0.5], [0.9, 0.5])),
            pos2(50.0, 50.0)
        );

        let there = moved(&rect, [0.25, -0.1]);
        assert!((there.a[0] - 0.35).abs() < 1e-6 && (there.a[1] - 0.1).abs() < 1e-6);
        assert!((there.b[0] - 0.55).abs() < 1e-6 && (there.b[1] - 0.3).abs() < 1e-6);

        let pushed = moved(&rect, [-0.5, 0.9]);
        assert!(pushed.a[0].abs() < 1e-6 && (pushed.b[0] - 0.2).abs() < 1e-6);
        assert!((pushed.a[1] - 0.8).abs() < 1e-6 && (pushed.b[1] - 1.0).abs() < 1e-6);
        assert_eq!((pushed.kind.as_str(), pushed.opacity), ("rect", 1.0));
    }

    #[test]
    fn a_triangle_is_even_and_turns_with_the_drag() {
        let [one, two, three] = triangle(pos2(0.0, 0.0), pos2(0.0, -10.0));
        assert!((one.distance(two) - two.distance(three)).abs() < 1e-3);
        assert!((two.distance(three) - three.distance(one)).abs() < 1e-3);
        assert_eq!(one, pos2(0.0, -10.0));
    }

    #[test]
    fn every_colour_reads_and_points_stay_on_the_map() {
        assert!(COLOURS.iter().all(|c| overseer_ui::hex(Some(c)).is_some()));
        assert_eq!(ink("nonsense"), overseer_ui::colour::WARN);
        let square = Square::from(Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 100.0)));
        let [x, y] = spot(square, pos2(-50.0, 60.0));
        assert!(x.abs() < 1e-6 && (y - 0.5).abs() < 1e-6, "{x} {y}");
    }

    /// A rectangle's corner moves with the one across from it held, and a
    /// circle grows from its middle. A label has nothing to drag.
    #[test]
    fn a_grip_resizes_and_the_far_side_stays() {
        let rect = Drawing {
            kind: "rect".to_owned(),
            a: [0.2, 0.2],
            b: [0.4, 0.5],
            ..Drawing::default()
        };
        assert_eq!(grips(&rect).len(), 4);
        // The top right corner dragged out: the bottom left stays.
        let wider = resized(&rect, 1, [0.6, 0.1]);
        assert_eq!((wider.a, wider.b), ([0.2, 0.5], [0.6, 0.1]));
        let circle = Drawing {
            kind: "circle".to_owned(),
            a: [0.5, 0.5],
            b: [0.55, 0.5],
            ..Drawing::default()
        };
        let bigger = resized(&circle, 0, [0.7, 0.5]);
        assert_eq!((bigger.a, bigger.b), ([0.5, 0.5], [0.7, 0.5]));
        let label = Drawing {
            kind: "text".to_owned(),
            ..Drawing::default()
        };
        assert!(grips(&label).is_empty());
    }

    /// A point goes on screen and comes back where it was at every turn, so
    /// turning the map never moves what is kept on it.
    #[test]
    fn a_turned_point_comes_back_where_it_was() {
        for turn in 0..4 {
            let square = Square {
                rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 200.0)),
                turn,
            };
            let kept = [0.25, 0.75];
            let [x, y] = spot(square, super::at(square, kept));
            assert!(
                (x - 0.25).abs() < 1e-5 && (y - 0.75).abs() < 1e-5,
                "turn {turn}"
            );
        }
    }

    /// A stroke is hit along its line and nowhere else, and moves whole with
    /// every point, stopping at the edge like any other shape. A text box
    /// takes the same four corners as a rectangle.
    #[test]
    fn a_stroke_and_a_text_box_behave_like_shapes() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)));
        let stroke = Drawing {
            kind: "brush".to_owned(),
            a: [0.1, 0.1],
            b: [0.5, 0.1],
            points: vec![[0.1, 0.1], [0.3, 0.1], [0.5, 0.1]],
            width: 0.02,
            ..Drawing::default()
        };
        assert!(hit(square, &stroke, pos2(40.0, 12.0)), "on the line");
        assert!(!hit(square, &stroke, pos2(40.0, 30.0)), "below it");
        let pushed = moved(&stroke, [0.7, 0.0]);
        let ends: Vec<f32> = pushed.points.iter().map(|p| p[0]).collect();
        assert_eq!(ends, [0.6, 0.8, 1.0], "held at the edge");
        assert!(grips(&stroke).is_empty());

        let boxed = Drawing {
            kind: "textbox".to_owned(),
            a: [0.2, 0.2],
            b: [0.4, 0.3],
            text: "Smoke first".to_owned(),
            ..Drawing::default()
        };
        assert_eq!(grips(&boxed).len(), 4);
        assert!(hit(square, &boxed, pos2(30.0, 25.0)));
        assert_eq!(handle(square, &boxed), pos2(20.0, 20.0));
        let wider = resized(&boxed, 2, [0.6, 0.5]);
        assert_eq!((wider.a, wider.b), ([0.2, 0.2], [0.6, 0.5]));
    }
}
