//! The whole lobby on one ladder, enemies above it and allies below.
//!
//! Every other answer on the board is a number per person, so the question
//! asked before any other, whether this lobby is above you or below you,
//! costs ten readings and some arithmetic. A ladder answers it without being
//! read. The strip is Riot's own tier colours in Riot's own order, and every
//! mark on it is that person's face.

use egui::epaint::Vertex;
use egui::{Align2, Color32, Mesh, Rect, Sense, Shape, Ui, pos2, vec2};
use overseer_core::{Board, Player};
use overseer_ui::{Face, art, caps_at, colour, hex, rank, shape, size, space};

use super::Side;
use super::paint;

/// How tall the ladder is: a heading, a row of faces, the strip, and a row
/// of faces back.
pub(crate) const HEIGHT: f32 = 96.0;

/// The highest tier Riot has. Radiant is one tier, not the first of three.
const TOP: u32 = 27;

/// The side of a face on the ladder.
const FACE: f32 = 22.0;

/// Where one account sits, in tiers times a hundred.
///
/// Riot number tiers three to a rank from Iron at three, and rating runs
/// nought to a hundred inside one, so multiplying out gives one axis on
/// which a tier is the same width everywhere. Immortal and Radiant rating
/// runs past a hundred; it is held to the tier it belongs to rather than
/// leaking into the next one.
fn rung(player: &Player) -> Option<f32> {
    let tier = player.rank_tier.filter(|t| *t >= 3)?;
    let rr = player.rr.unwrap_or(0).clamp(0, 99) as f32;
    Some(tier as f32 * 100.0 + rr)
}

/// The tiers the strip runs over: out to whole ranks, and never past
/// Radiant.
///
/// Whole ranks, because a strip that starts at Bronze 3 has a band one tier
/// wide at each end, too narrow to carry its name.
fn span(lowest: u32, highest: u32) -> (u32, u32) {
    let first = lowest.div_euclid(3) * 3;
    let last = (highest.div_euclid(3) * 3 + 2).min(TOP);
    (first, last.max(first))
}

/// Spreads marks along an axis so none overlap, without leaving it.
///
/// Forward first, pushing each mark clear of the one before; then back from
/// the far end, pulling any that were pushed off it. Order is kept, so a
/// mark never jumps past the player it was beside. If there is simply not
/// room for them all, they are spaced evenly from the near end, which is as
/// honest as a full strip can be.
pub(crate) fn spread(wanted: &[f32], gap: f32, low: f32, high: f32) -> Vec<f32> {
    let mut at: Vec<f32> = wanted.iter().map(|w| w.clamp(low, high)).collect();
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
        return (0..at.len()).map(|i| low + gap * i as f32).collect();
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
    let tiers = placed.iter().filter_map(|(p, _)| p.rank_tier);
    let (Some(lowest), Some(highest)) = (tiers.clone().min(), tiers.max()) else {
        return;
    };
    if placed.len() < 2 {
        return;
    }
    let (first, last) = span(lowest, highest);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let head = rect.top() + space::MD;
    caps_at(
        &painter,
        pos2(rect.left(), head),
        Align2::LEFT_CENTER,
        "ladder",
        Face::Heavy.at(15.0),
        colour::TEXT,
    );
    caps_at(
        &painter,
        pos2(rect.right(), head),
        Align2::RIGHT_CENTER,
        &standing(board, &placed),
        paint::label(),
        colour::TEXT_FAINT,
    );
    let strip = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 50.0),
        pos2(rect.right(), rect.top() + 62.0),
    );
    let step = strip.width() / (last - first + 1) as f32;
    bands(&painter, first, last, strip, step);
    let floor = first as f32 * 100.0;
    let width = (last - first + 1) as f32 * 100.0;
    let x_of = |rung: f32| strip.left() + (rung - floor) / width * strip.width();
    for (side, team) in teams {
        let mine: Vec<(&Player, f32)> = placed
            .iter()
            .filter(|(p, _)| p.team.as_deref() == Some(team.as_str()))
            .copied()
            .collect();
        marks(&painter, mine, &x_of, strip, *side == Side::Enemy);
    }
}

/// One side's faces along the strip, spread so none overlap, each on a stem
/// down to the point it is really about.
fn marks(
    painter: &egui::Painter,
    mut mine: Vec<(&Player, f32)>,
    x_of: &dyn Fn(f32) -> f32,
    strip: Rect,
    above: bool,
) {
    mine.sort_by(|a, b| a.1.total_cmp(&b.1));
    let wanted: Vec<f32> = mine.iter().map(|(_, rung)| x_of(*rung)).collect();
    let half = FACE / 2.0;
    let xs = spread(
        &wanted,
        FACE + 2.0,
        strip.left() + half,
        strip.right() - half,
    );
    let y = if above {
        strip.top() - 8.0 - half
    } else {
        strip.bottom() + 8.0 + half
    };
    for ((player, rung), x) in mine.iter().zip(xs) {
        let true_x = x_of(*rung).clamp(strip.left(), strip.right());
        let (from, to) = if above {
            (y + half, strip.top())
        } else {
            (strip.bottom(), y - half)
        };
        painter.vline(true_x, from..=to, (1.0, colour::TEXT_FAINT));
        if (x - true_x).abs() > 1.0 {
            let shelf = if above { y + half } else { y - half };
            painter.hline(
                true_x.min(x)..=true_x.max(x),
                shelf,
                (1.0, colour::TEXT_FAINT),
            );
        }
        face(
            painter,
            player,
            Rect::from_center_size(pos2(x, y), vec2(FACE, FACE)),
        );
    }
}

/// One account's square portrait, with a cream frame if it is you.
fn face(painter: &egui::Painter, player: &Player, rect: Rect) {
    let tint = hex(player.agent_color.as_deref()).unwrap_or(colour::BG_INSET);
    painter.rect_filled(rect, 0, shape::blend(tint, colour::BG, 0.4));
    if let Some(portrait) = art::agent(painter.ctx(), player.agent.as_deref().unwrap_or("")) {
        let mut mesh = Mesh::with_texture(portrait.id());
        mesh.add_rect_with_uv(
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.add(Shape::mesh(mesh));
    }
    if player.is_self {
        painter.rect_stroke(
            rect.expand(2.0),
            0,
            egui::Stroke::new(2.0, colour::YOU),
            egui::StrokeKind::Outside,
        );
    }
}

/// The strip: one segment per tier in the tier's own colour, and the rank
/// named in its band wherever the band is wide enough to hold the name.
fn bands(painter: &egui::Painter, first: u32, last: u32, strip: Rect, step: f32) {
    let mut mesh = Mesh::default();
    for tier in first..=last {
        let from = strip.left() + (tier - first) as f32 * step;
        let tint = rank(Some(tier));
        let top = tint.gamma_multiply(0.85);
        let bottom = tint.gamma_multiply(0.45);
        let base = mesh.vertices.len() as u32;
        for (x, y, colour) in [
            (from, strip.top(), top),
            (from + step - 1.0, strip.top(), top),
            (from + step - 1.0, strip.bottom(), bottom),
            (from, strip.bottom(), bottom),
        ] {
            mesh.vertices.push(Vertex {
                pos: pos2(x, y),
                uv: egui::epaint::WHITE_UV,
                color: colour,
            });
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    painter.add(Shape::mesh(mesh));
    for group in first.div_euclid(3)..=last.div_euclid(3) {
        let from = (group * 3).max(first);
        let to = (group * 3 + 2).min(last);
        let left = strip.left() + (from - first) as f32 * step;
        let right = strip.left() + (to + 1 - first) as f32 * step;
        let name = overseer_ui::rank_group(from);
        let font = Face::Display.at(size::MICRO);
        if overseer_ui::caps_width(painter, name, font.clone()) + space::XL > right - left {
            continue;
        }
        caps_at(
            painter,
            pos2(f32::midpoint(left, right), strip.center().y + 0.5),
            Align2::CENTER_CENTER,
            name,
            font,
            shape::ink_on(rank(Some(from)).gamma_multiply(0.65)),
        );
    }
}

/// The one sentence the ladder is worth saying out loud.
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
        0 => format!("none of them above you{tail}"),
        1 => format!("one of them above you{tail}"),
        n => format!("{n} of them above you{tail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{TOP, span, spread};

    /// Radiant is the top of the scale, not the first of three tiers.
    #[test]
    fn the_strip_stops_at_radiant() {
        assert_eq!(span(27, 27), (27, 27));
        assert_eq!(span(24, 27), (24, TOP));
        assert_eq!(span(8, 14), (6, 14));
    }

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
