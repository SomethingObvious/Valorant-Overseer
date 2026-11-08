//! The whole lobby on one ladder, enemies above it and allies below. It
//! shows whether the lobby is above or below you without reading ten
//! numbers. The strip is Riot's tier colours in Riot's order, and every mark
//! on it is that person's face.

use egui::epaint::Vertex;
use egui::{Align2, Color32, Mesh, Rect, Sense, Shape, Ui, pos2, vec2};
use overseer_core::{Board, Player};
use overseer_ui::{Face, art, caps_at, caps_text, colour, hex, rank, shape, size, space};

use super::Side;
use super::paint;

/// How tall the ladder is: a heading, a row of faces, the strip, and a row
/// of faces back.
pub(crate) const HEIGHT: f32 = 108.0;

/// The highest tier Riot has. Radiant is one tier, not the first of three.
const TOP: u32 = 27;

/// The side of a face on the ladder.
const FACE: f32 = 22.0;

/// Half the width of the notch under a face, and how far it points.
const NOTCH: f32 = 3.0;

/// Where one account sits, in tiers times a hundred.
///
/// Riot numbers tiers three to a rank from Iron at three, and rating runs
/// nought to a hundred inside one, so this gives one axis where every tier
/// is the same width. Immortal and Radiant rating runs past a hundred, and
/// it is held to its own tier rather than leaking into the next.
fn rung(player: &Player) -> Option<f32> {
    let tier = player.rank_tier.filter(|t| *t >= 3)?;
    let rr = player.rr.unwrap_or(0).clamp(0, 99) as f32;
    Some((tier as f32).mul_add(100.0, rr))
}

/// The tiers the strip runs over: out to whole ranks, and never past
/// Radiant. A strip that started or stopped mid-rank would have a band too
/// narrow to carry its name.
fn span(lowest: u32, highest: u32) -> (u32, u32) {
    let first = lowest.div_euclid(3) * 3;
    let last = (highest.div_euclid(3) * 3 + 2).min(TOP);
    (first, last.max(first))
}

/// Spreads marks along an axis so none overlap, without leaving it.
///
/// A pass forward pushes each mark clear of the one before, then a pass
/// back from the far end pulls in any pushed off it. Order is kept, so a
/// mark never jumps past its neighbour. When there is not room for them all,
/// they are spaced evenly from the near end.
pub(crate) fn spread(wanted: &[f32], gap: f32, low: f32, high: f32) -> Vec<f32> {
    let mut at: Vec<f32> = wanted.iter().map(|w| w.min(high).max(low)).collect();
    for i in 1..at.len() {
        let before = at.get(i - 1).copied().unwrap_or(low);
        if let Some(here) = at.get_mut(i) {
            *here = here.max(before + gap);
        }
    }
    let mut ceiling = high;
    for here in at.iter_mut().rev() {
        *here = here.min(ceiling);
        ceiling = *here - gap;
    }
    if at.first().is_some_and(|first| *first < low) {
        return (0..at.len()).map(|i| gap.mul_add(i as f32, low)).collect();
    }
    at
}

/// The ladder, if at least two accounts in the lobby have a rank to put on
/// it.
pub(crate) fn draw(ui: &mut Ui, board: &Board, teams: &[(Side, String); 2]) {
    let placed: Vec<(&Player, f32)> = board
        .players
        .iter()
        .filter_map(|p| rung(p).map(|at| (p, at)))
        .collect();
    if placed.len() < 2 {
        return;
    }
    let tiers = placed.iter().filter_map(|(p, _)| p.rank_tier);
    let (Some(lowest), Some(highest)) = (tiers.clone().min(), tiers.max()) else {
        return;
    };
    let (first, last) = span(lowest, highest);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let head = rect.top() + space::MD;
    let named = caps_text(
        &painter,
        pos2(rect.left(), head),
        Align2::LEFT_CENTER,
        "Ladder",
        Face::Heavy.at(size::TITLE),
        colour::TEXT,
    );
    let stood = caps_text(
        &painter,
        pos2(rect.right(), head),
        Align2::RIGHT_CENTER,
        &standing(board, &placed),
        paint::label(),
        colour::TEXT_FAINT,
    );
    paint::double_rule(
        &painter,
        named.right() + space::MD..=stood.left() - space::LG,
        head + 1.0,
    );
    let strip = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 56.0),
        pos2(rect.right(), rect.top() + 72.0),
    );
    let step = strip.width() / (last - first + 1) as f32;
    bands(&painter, first, last, strip, step);
    let floor = first as f32 * 100.0;
    let width = (last - first + 1) as f32 * 100.0;
    let x_of = |rung: f32| ((rung - floor) / width).mul_add(strip.width(), strip.left());
    for (side, team) in teams {
        let mine: Vec<(&Player, f32)> = placed
            .iter()
            .filter(|(p, _)| super::on(p, team))
            .copied()
            .collect();
        marks(&painter, mine, &x_of, strip, *side);
    }
}

/// How near two faces' centres may come: closer than a face, so a crowd
/// overlaps like a hand of cards rather than walking off its point.
const OVERLAP: f32 = FACE * 0.78;

/// How far a face in a crowd turns, the other way from its neighbour, so
/// the overlap reads as one card on another.
const TURN: f32 = 0.06;

/// One side's faces along the strip, spread so none hides another, each on
/// a straight stem down to the point it is really about.
fn marks(
    painter: &egui::Painter,
    mut mine: Vec<(&Player, f32)>,
    x_of: &dyn Fn(f32) -> f32,
    strip: Rect,
    side: Side,
) {
    // Enemies above in red and allies below in green, the board's own two
    // colours, so a face says its side before it says who it is.
    let above = side == Side::Enemy;
    let team = if above { colour::ENEMY } else { colour::ALLY };
    mine.sort_by(|a, b| a.1.total_cmp(&b.1));
    let wanted: Vec<f32> = mine.iter().map(|(_, rung)| x_of(*rung)).collect();
    let half = FACE / 2.0;
    let xs = spread(&wanted, OVERLAP, strip.left() + half, strip.right() - half);
    let y = if above {
        strip.top() - 8.0 - half
    } else {
        strip.bottom() + 8.0 + half
    };
    let toward: f32 = if above { 1.0 } else { -1.0 };
    let edge = if above { strip.top() } else { strip.bottom() };
    // You last, so you lie on top of whoever you share a spot with.
    let mut order: Vec<usize> = (0..mine.len()).collect();
    order.sort_by_key(|i| mine.get(*i).is_some_and(|(p, _)| p.is_self));
    for i in order {
        let (Some((player, rung)), Some(&x)) = (mine.get(i), xs.get(i)) else {
            continue;
        };
        let true_x = x_of(*rung).clamp(strip.left(), strip.right());
        let near = |other: Option<&f32>| other.is_some_and(|o| (o - x).abs() < FACE + 2.0);
        let crowded = near(i.checked_sub(1).and_then(|j| xs.get(j))) || near(xs.get(i + 1));
        // A stand-in is cut from a card that does not turn, so it stays level.
        let turns = crowded && !player.is_self && player.agent.is_some();
        let turn = match (turns, i % 2 == 0) {
            (false, _) => 0.0,
            (true, true) => -TURN,
            (true, false) => TURN,
        };
        let (frame, stem) = if player.is_self {
            (colour::YOU, colour::YOU)
        } else {
            (team, team.gamma_multiply(0.55))
        };
        face(painter, player, pos2(x, y), turn, frame);
        // Past the frame, which is wider round your own face.
        let rim = if player.is_self { 4.0 } else { 1.5 };
        let base = toward.mul_add(half + rim, y);
        let tip = toward.mul_add(NOTCH, base);
        // The notch slides along the face to sit over the point, so the stem
        // drops straight to it, but only along the part of the face no
        // neighbour lies over, or the stem would seem to end on them. It
        // leans when a crowd pushes a face further than that.
        let (mut lo, mut hi) = (x - half, x + half);
        if !player.is_self
            && let Some((next, &at)) = mine.get(i + 1).zip(xs.get(i + 1))
        {
            hi = hi.min(at - cover(next.0));
        }
        if !player.is_self
            && let Some((before, &at)) = i.checked_sub(1).and_then(|j| mine.get(j).zip(xs.get(j)))
            && before.0.is_self
        {
            lo = lo.max(at + cover(before.0));
        }
        let notch = true_x.max(lo + NOTCH).min(hi - NOTCH);
        painter.line_segment([pos2(true_x, edge), pos2(notch, tip)], (1.0, stem));
        painter.add(Shape::convex_polygon(
            vec![
                pos2(notch - NOTCH, base),
                pos2(notch + NOTCH, base),
                pos2(notch, tip),
            ],
            frame,
            egui::Stroke::NONE,
        ));
    }
}

/// How far from its centre a face hides what lies under it: its frame and
/// the rim of background round that.
fn cover(player: &Player) -> f32 {
    FACE / 2.0 + if player.is_self { 4.0 } else { 2.5 }
}

/// One account's square portrait, turned by `turn` and framed in `frame`,
/// or in cream if it is you. A rim of the background round it keeps a face
/// laid over another apart from it.
fn face(painter: &egui::Painter, player: &Player, centre: egui::Pos2, turn: f32, frame: Color32) {
    let rect = Rect::from_center_size(centre, vec2(FACE, FACE));
    let corners = |grow: f32| -> Vec<egui::Pos2> {
        let (sin, cos) = turn.sin_cos();
        let h = FACE / 2.0 + grow;
        [(-h, -h), (h, -h), (h, h), (-h, h)]
            .iter()
            .map(|(dx, dy)| {
                pos2(
                    centre.x + dx * cos - dy * sin,
                    centre.y + dx * sin + dy * cos,
                )
            })
            .collect()
    };
    painter.add(Shape::convex_polygon(
        corners(2.5),
        colour::BG,
        egui::Stroke::NONE,
    ));
    let tint = hex(player.agent_color.as_deref()).unwrap_or(colour::BG_INSET);
    painter.add(Shape::convex_polygon(
        corners(0.0),
        shape::blend(tint, colour::BG, 0.4),
        egui::Stroke::NONE,
    ));
    if player.agent.is_none() {
        stand_in(painter, player, rect);
    } else if let Some(portrait) = art::agent(painter.ctx(), player.agent.as_deref().unwrap_or(""))
    {
        let mut mesh = Mesh::with_texture(portrait.id());
        let uvs = [
            pos2(0.0, 0.0),
            pos2(1.0, 0.0),
            pos2(1.0, 1.0),
            pos2(0.0, 1.0),
        ];
        for (pos, uv) in corners(0.0).into_iter().zip(uvs) {
            mesh.vertices.push(Vertex {
                pos,
                uv,
                color: Color32::WHITE,
            });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(mesh));
    }
    if player.is_self {
        painter.rect_stroke(
            rect.expand(2.0),
            0,
            egui::Stroke::new(2.0, colour::YOU),
            egui::StrokeKind::Outside,
        );
    } else {
        painter.add(Shape::closed_line(
            corners(0.75),
            egui::Stroke::new(1.5, frame),
        ));
    }
}

/// Somebody with no agent yet: a square from the middle of their card, and
/// the same blacked-out figure their row has.
fn stand_in(painter: &egui::Painter, player: &Player, rect: Rect) {
    let ctx = painter.ctx();
    if let Some(card) = paint::banner(ctx, player) {
        let size = card.size_vec2();
        let square = (size.y / size.x.max(1.0)).min(1.0);
        let mut mesh = Mesh::with_texture(card.id());
        mesh.add_rect_with_uv(
            rect,
            Rect::from_min_max(pos2(0.5 - square / 2.0, 0.0), pos2(0.5 + square / 2.0, 1.0)),
            Color32::from_gray(190),
        );
        painter.add(Shape::mesh(mesh));
    }
    paint::figure(painter, player, rect, 1.0);
}

/// Dark type on a light plate, cream on a dark one. Both clear 4.5:1.
fn ink_on(fill: Color32) -> Color32 {
    if egui::Rgba::from(fill).intensity() > 0.215 {
        colour::VOID
    } else {
        colour::TEXT_STRONG
    }
}

/// The strip: a plate per rank on the board's lean, brightening from its
/// first tier to its last so where someone sits inside a rank shows too,
/// and the rank named on it wherever it is wide enough.
fn bands(painter: &egui::Painter, first: u32, last: u32, strip: Rect, step: f32) {
    let run = strip.height() * paint::LEAN;
    let left_of = |tier: u32| ((tier - first) as f32).mul_add(step, strip.left());
    let font = Face::Heavy.at(size::LABEL);
    for group in first.div_euclid(3)..=last.div_euclid(3) {
        let from = (group * 3).max(first);
        let to = (group * 3 + 2).min(last);
        let left = left_of(from);
        // A clear break between ranks, so the three tiers of one read as one.
        let right = left_of(to + 1) - 4.0;
        let tint = rank(Some(from));
        let low = shape::blend(tint, colour::BG, 0.42);
        let high = shape::blend(tint, colour::TEXT_STRONG, 0.12);
        let mut mesh = Mesh::default();
        for (x, y, shade) in [
            (left + run, strip.top(), low),
            (right, strip.top(), high),
            (right - run, strip.bottom(), high),
            (left, strip.bottom(), low),
        ] {
            mesh.colored_vertex(pos2(x, y), shade);
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(mesh));
        let name = overseer_ui::rank_group(from);
        let width = overseer_ui::caps_width(painter, name, font.clone());
        let fits = width + space::XL + run <= right - left;
        // The tiers as upright gaps on whole points, which stay crisp where a
        // hairline on the lean would stairstep, and none under the name.
        let clear = if fits {
            left + run + space::SM + width + space::SM
        } else {
            left
        };
        for tier in from + 1..=to {
            let x = left_of(tier).round();
            if x >= clear {
                painter.rect_filled(
                    Rect::from_x_y_ranges(x - 1.0..=x, strip.top()..=strip.bottom()),
                    0,
                    colour::BG,
                );
            }
        }
        if fits {
            // On the darkest end, so cream unless the rank's colour is light
            // enough to stay light there.
            caps_at(
                painter,
                pos2(left + run + space::SM, strip.center().y - 1.0),
                Align2::LEFT_CENTER,
                name,
                font.clone(),
                ink_on(low),
            );
        }
    }
}

/// The note at the ladder's top right: how many of the other side rank
/// above you, and how many have no rank to show.
fn standing(board: &Board, placed: &[(&Player, f32)]) -> String {
    let unranked = board.players.len().saturating_sub(placed.len());
    let tail = if unranked == 0 {
        String::new()
    } else {
        format!("  \u{b7}  {unranked} off the ladder")
    };
    let ours = board.self_team.as_deref();
    let Some((_me, mine)) = placed.iter().find(|(p, _)| p.is_self) else {
        return format!("{} ranked{tail}", placed.len());
    };
    let over = placed
        .iter()
        .filter(|(p, rung)| p.team.as_deref() != ours && rung > mine)
        .count();
    match over {
        0 => format!("None of them above you{tail}"),
        1 => format!("One of them above you{tail}"),
        n => format!("{n} of them above you{tail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::spread;

    /// Marks never overlap, never leave the axis, and keep their order.
    #[test]
    fn marks_spread_without_leaving_the_strip() {
        // Three accounts on the same point at the top of the strip.
        let at = spread(&[500.0, 500.0, 500.0], 24.0, 0.0, 500.0);
        assert_eq!(at, [452.0, 476.0, 500.0]);
        // Already apart: left alone.
        assert_eq!(spread(&[10.0, 100.0], 24.0, 0.0, 500.0), [10.0, 100.0]);
        // No room at all: spaced from the near end.
        let crowded = spread(&[5.0; 5], 24.0, 0.0, 50.0);
        for pair in crowded.windows(2) {
            if let [a, b] = pair {
                assert!((b - a - 24.0).abs() < 0.01);
            }
        }
    }
}
