//! The broadcast's vocabulary, drawn once: slabs, the slant, faces cut to
//! it, emblems with their light, numerals that line up, and results as pips.
//! Everything on the board is built from these, which keeps it one graphic
//! rather than a page of widgets.

use egui::epaint::Vertex;
use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect, Shape, pos2, vec2};
use overseer_core::Player;
use overseer_ui::{Face, art, colour, hex, rank, shape, size, space};

use super::Side;

/// How far the slant leans, as the run of the cut per point of its height.
/// About twelve degrees, and every slanted thing on the board uses it.
pub(crate) const LEAN: f32 = 0.21;

/// A quadrilateral whose left and right edges lean by [`LEAN`] where asked.
/// `lead` cuts the left edge and `tail` the right. With neither it is a
/// plain rectangle, which is what most slabs are.
pub(crate) fn slanted(rect: Rect, lead: bool, tail: bool) -> Vec<Pos2> {
    let run = rect.height() * LEAN;
    vec![
        pos2(rect.left() + if lead { run } else { 0.0 }, rect.top()),
        pos2(rect.right(), rect.top()),
        pos2(rect.right() - if tail { run } else { 0.0 }, rect.bottom()),
        pos2(rect.left(), rect.bottom()),
    ]
}

/// A filled slanted shape.
pub(crate) fn slant(rect: Rect, lead: bool, tail: bool, fill: Color32) -> Shape {
    Shape::convex_polygon(slanted(rect, lead, tail), fill, egui::Stroke::NONE)
}

/// A tag the app worked out, as an outline in the board's slant. Your own
/// tags are solid cream, and the difference is who said it.
pub(crate) fn chip_outline(painter: &egui::Painter, chip: Rect, ink: Color32) {
    let mut points = slanted(chip, false, true);
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    painter.add(Shape::line(points, egui::Stroke::new(1.0, ink)));
}

/// One slab, with an offset shadow under it and a light along its top edge.
/// `lift` runs from resting to under the pointer and raises both. `still`
/// drops the shadow, since the efficient tier draws no blur.
pub(crate) fn slab(painter: &egui::Painter, rect: Rect, fill: Color32, lift: f32, still: bool) {
    if !still {
        painter.add(
            egui::epaint::RectShape::filled(
                rect.translate(vec2(0.0, 3.0f32.mul_add(lift, 3.0))),
                0,
                Color32::from_black_alpha(50.0f32.mul_add(lift, 90.0) as u8),
            )
            .with_blur_width(6.0f32.mul_add(lift, 8.0)),
        );
    }
    painter.rect_filled(rect, 0, fill);
    painter.hline(
        rect.x_range(),
        rect.top() + 0.5,
        (
            1.0,
            colour::TEXT_STRONG.gamma_multiply(0.05f32.mul_add(lift, 0.05)),
        ),
    );
}

/// The agent's killfeed crop, with its right edge cut to the slant.
///
/// The texture is mapped straight onto the slanted quad, so the cut goes
/// through the picture. An agent this build has no art for gets a plate in
/// their colour with their initial, since the newest agent's row is the one
/// somebody most wants to read.
pub(crate) fn crop(painter: &egui::Painter, player: &Player, rect: Rect) {
    // Nobody picked yet: a blacked-out figure over the player card the row
    // already has behind it.
    let Some(agent) = player.agent.as_deref() else {
        stand_in(painter, player, rect);
        return;
    };
    let tint = hex(player.agent_color.as_deref()).unwrap_or(colour::BG_INSET);
    let points = slanted(rect, false, true);
    let Some(face) = art::killfeed(painter.ctx(), agent) else {
        painter.add(Shape::convex_polygon(
            points,
            shape::blend(tint, colour::BG, 0.35),
            egui::Stroke::NONE,
        ));
        let initial: String = agent.chars().take(1).collect::<String>().to_uppercase();
        let (initial, ink) = if initial.is_empty() {
            ("?".to_owned(), colour::TEXT_FAINT)
        } else {
            (initial, shape::ink_on(tint))
        };
        painter.text(
            pos2(rect.height().mul_add(0.6, rect.left()), rect.center().y),
            Align2::CENTER_CENTER,
            initial,
            Face::Heavy.at(rect.height() * 0.5),
            ink,
        );
        return;
    };
    // The agent's colour under the art, which shows through the few pixels
    // Riot's crop leaves transparent and ties the face to its agent.
    painter.add(Shape::convex_polygon(
        points.clone(),
        shape::blend(tint, colour::BG, 0.55),
        egui::Stroke::NONE,
    ));
    paint_face(painter, &face, rect, &points, Color32::WHITE);
}

/// The blacked-out figure in the crop's slot, for somebody with no agent
/// yet. It is a square head and shoulders, because the killfeed crop is cut
/// so tight that blacked out it fills the slot and looks like a hole.
fn stand_in(painter: &egui::Painter, player: &Player, rect: Rect) {
    let side = rect.height();
    let square = Rect::from_center_size(
        pos2(rect.width().mul_add(0.55, rect.left()), rect.center().y),
        vec2(side, side),
    );
    figure(painter, player, square, 1.5);
}

/// A player's blacked-out stand-in filling `square`, its rim `rim` points up
/// and to the right.
pub(crate) fn figure(painter: &egui::Painter, player: &Player, square: Rect, rim: f32) {
    let Some(portrait) = art::stand_in_portrait(painter.ctx(), seed(player)) else {
        return;
    };
    for (tint, shift) in [(RIM, vec2(rim, -rim)), (SHADOW, egui::Vec2::ZERO)] {
        let mut mesh = Mesh::with_texture(portrait.id());
        mesh.add_rect_with_uv(
            square.translate(shift),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            tint,
        );
        painter.add(Shape::mesh(mesh));
    }
}

/// A crop drawn into the slanted box.
fn paint_face(
    painter: &egui::Painter,
    face: &egui::TextureHandle,
    rect: Rect,
    points: &[Pos2],
    tint: Color32,
) {
    let mut mesh = Mesh::with_texture(face.id());
    for point in points {
        mesh.vertices.push(Vertex {
            pos: *point,
            uv: pos2(
                (point.x - rect.left()) / rect.width().max(1.0),
                (point.y - rect.top()) / rect.height().max(1.0),
            ),
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// The stand-in's rim: its own shape offset up and to the right, in cream,
/// so it reads as a figure lit from behind rather than a hole in the card.
const RIM: Color32 = Color32::from_rgba_premultiplied(150, 147, 141, 170);

/// The stand-in figure itself.
const SHADOW: Color32 = Color32::from_black_alpha(238);

/// FNV-1a over the account id, so a player gets the same stand-in every
/// frame and every launch.
fn seed(player: &Player) -> u64 {
    player
        .puuid
        .as_deref()
        .unwrap_or("")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

/// Their player card, or Riot's default one when the backend has none.
pub(crate) fn banner(ctx: &egui::Context, player: &Player) -> Option<egui::TextureHandle> {
    player
        .card_art
        .as_deref()
        .and_then(|path| art::file(ctx, path))
        .or_else(|| art::default_card(ctx))
}

/// The player card behind the head of a row, for somebody who has not
/// picked an agent, the way agent select shows it. It fades into the row's
/// `fill` just past the face, so the name sits on the row and not on the card.
pub(crate) fn card_behind(
    painter: &egui::Painter,
    player: &Player,
    line: Rect,
    (fill, portrait): (Color32, f32),
) {
    let Some(texture) = banner(painter.ctx(), player) else {
        return;
    };
    let size = texture.size_vec2();
    let wide = line.height() * size.x / size.y.max(1.0);
    let card = Rect::from_min_size(line.min, vec2(wide, line.height()));
    let mut mesh = Mesh::with_texture(texture.id());
    mesh.add_rect_with_uv(
        card,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::from_gray(190),
    );
    painter.add(Shape::mesh(mesh));
    let from = portrait.mul_add(0.75, card.left()).min(card.right());
    let mut fade = Mesh::default();
    let to = (card.left() + portrait + space::XXL).min(card.right());
    for (x, y, tint) in [
        (from, card.top(), Color32::TRANSPARENT),
        (to, card.top(), fill),
        (to, card.bottom(), fill),
        (from, card.bottom(), Color32::TRANSPARENT),
    ] {
        fade.colored_vertex(pos2(x, y), tint);
    }
    fade.add_triangle(0, 1, 2);
    fade.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(fade));
    painter.rect_filled(Rect::from_min_max(pos2(to, card.top()), card.max), 0, fill);
}

/// A rank's emblem, standing in its own light.
///
/// The light is a bloom of the emblem's own shape in the tier's colour, and
/// it gets stronger up the ladder so a Radiant catches the eye before an
/// Iron. `light` scales it. Nothing is drawn for a tier with no emblem.
pub(crate) fn emblem(painter: &egui::Painter, tier: u32, centre: Pos2, side: f32, light: f32) {
    let Some(art) = art::rank(painter.ctx(), tier) else {
        return;
    };
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if light > 0.0
        && let Some(bloom) = art::rank_glow(painter.ctx(), tier)
    {
        // The bloom's picture is the emblem's with the pad on every side, at
        // the same pixel scale, so it is drawn that much bigger.
        let stored = art.size()[0] as f32;
        let reach = side * 2.0f32.mul_add(art::GLOW_PAD as f32, stored) / stored;
        let climb = (tier as f32 / 27.0).clamp(0.0, 1.0);
        let mut mesh = Mesh::with_texture(bloom.id());
        mesh.add_rect_with_uv(
            Rect::from_center_size(centre, vec2(reach, reach)),
            whole,
            vivid(rank(Some(tier))).gamma_multiply(0.4f32.mul_add(climb, 0.45) * light),
        );
        painter.add(Shape::mesh(mesh));
    }
    let mut mesh = Mesh::with_texture(art.id());
    mesh.add_rect_with_uv(
        Rect::from_center_size(centre, vec2(side, side)),
        whole,
        Color32::WHITE,
    );
    painter.add(Shape::mesh(mesh));
}

/// A disc of colour fading to nothing at its rim.
pub(crate) fn glow(centre: Pos2, radius: f32, tint: Color32, strength: f32) -> Shape {
    const SEGMENTS: u32 = 24;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centre, tint.gamma_multiply(strength));
    for i in 0..=SEGMENTS {
        let angle = std::f32::consts::TAU * i as f32 / SEGMENTS as f32;
        mesh.colored_vertex(
            centre + vec2(angle.cos(), angle.sin()) * radius,
            Color32::TRANSPARENT,
        );
    }
    for i in 1..=SEGMENTS {
        mesh.add_triangle(0, i, i + 1);
    }
    Shape::mesh(mesh)
}

/// A numeral set in fixed cells, right against `at`, and where it landed.
///
/// Barlow's digits are proportional, so a column of K/Ds set as plain text
/// wobbles at the decimal point. Each digit gets the width of a nought and
/// sits centred in it, the way a tabular figure does.
pub(crate) fn numeral(
    painter: &egui::Painter,
    text: &str,
    at: Pos2,
    font: &FontId,
    tint: Color32,
) -> Rect {
    let cell = painter
        .layout_no_wrap("0".to_owned(), font.clone(), tint)
        .size()
        .x;
    let mut x = at.x;
    let mut height: f32 = 0.0;
    for ch in text.chars().rev() {
        let galley = painter.layout_no_wrap(ch.to_string(), font.clone(), tint);
        let glyph = galley.size();
        let width = if ch.is_ascii_digit() { cell } else { glyph.x };
        x -= width;
        height = height.max(glyph.y);
        painter.galley(
            pos2(x + (width - glyph.x) / 2.0, at.y - glyph.y / 2.0),
            galley,
            tint,
        );
    }
    Rect::from_min_max(
        pos2(x, at.y - height / 2.0),
        pos2(at.x, at.y + height / 2.0),
    )
}

/// The last few results, newest first, as slanted pips.
///
/// Coloured by what the result meant for you, not for them: an enemy's win
/// is in the enemy's red, and an ally's win in your green. A loss is a dim
/// pip either way, because a loss is the absence of the thing being counted.
pub(crate) fn pips(painter: &egui::Painter, form: &[String], left: Pos2, pip: f32, win: Color32) {
    let mut x = left.x;
    for result in form.iter().take(5) {
        let tint = match result.chars().next() {
            Some('W' | 'w') => win,
            Some('D' | 'd') => colour::TEXT_FAINT,
            _ => colour::TEXT_FAINT.gamma_multiply(0.45),
        };
        let rect = Rect::from_min_size(pos2(x, pip.mul_add(-0.7, left.y)), vec2(pip, pip * 1.4));
        painter.add(slant(rect, true, true, tint));
        x += pip + 2.0;
    }
}

/// A colour as light: 35% more saturated, at full value. The rank palette
/// is muted for text and the ladder, and a glow in it looks grey.
fn vivid(tint: Color32) -> Color32 {
    let mut light = egui::ecolor::Hsva::from(tint);
    light.s = (light.s * 1.35).min(1.0);
    light.v = 1.0;
    Color32::from(light)
}

/// A party down the gutter: one tab the height of its rows, with its size
/// set sideways in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spine {
    /// The party's colour, from the party palette by the party's number.
    pub(crate) tint: Color32,
    /// How many are in it, Riot's count or the app's guess.
    pub(crate) size: u32,
    /// Whether the size is the app's guess rather than Riot's word. A guess
    /// is drawn as an outline.
    pub(crate) guessed: bool,
    /// The rows it runs down, first and last, by their place in the list.
    pub(crate) rows: (usize, usize),
}

/// Each party on a side and its colour: the side's own pair, in the
/// order of the parties' numbers, so a sort or a search never swaps them.
///
/// `members` is everybody on the side, not only who is showing, or a search
/// that hid the first party would hand its colour to the second.
pub(crate) fn party_tints(members: &[&Player], side: Side) -> Vec<(u32, Color32)> {
    let palette = match side {
        Side::Ally => colour::PARTY_OURS,
        Side::Enemy => colour::PARTY_THEIRS,
    };
    let mut numbers: Vec<u32> = members
        .iter()
        .filter_map(|p| p.party.as_ref().and_then(|party| party.number))
        .collect();
    numbers.sort_unstable();
    numbers
        .chunk_by(|a, b| a == b)
        .filter(|group| group.len() > 1)
        .filter_map(|group| group.first().copied())
        .zip(palette.iter().cycle().copied())
        .collect()
}

/// The party spines for a side, in the order its rows are drawn.
///
/// A party is a run of adjacent rows with one party number, so a sort can
/// split it into pieces that share a colour. The colour comes from
/// [`party_tints`] rather than the backend, so your parties always get your
/// side's pair. Riot numbers a solo player too, and a party of one gets no
/// tab. A guess covers one row, because the backend guesses a size and not
/// who with.
pub(crate) fn spines(players: &[&Player], tints: &[(u32, Color32)]) -> Vec<Spine> {
    let mut out: Vec<Spine> = Vec::new();
    for (i, player) in players.iter().enumerate() {
        if let Some(party) = player.party.as_ref()
            && let Some(number) = party.number
        {
            let tint = tints
                .iter()
                .find(|(n, _)| *n == number)
                .map_or(colour::TEXT_FAINT, |(_, tint)| *tint);
            match out.last_mut() {
                Some(run) if !run.guessed && run.tint == tint && run.rows.1 + 1 == i => {
                    run.rows.1 = i;
                    run.size = run.size.max(party.size.unwrap_or(0));
                }
                _ => out.push(Spine {
                    tint,
                    size: party.size.unwrap_or(0),
                    guessed: false,
                    rows: (i, i),
                }),
            }
        } else if let Some(size) = player.stack_guess.as_ref().and_then(|g| g.size) {
            out.push(Spine {
                tint: colour::TEXT_FAINT,
                size,
                guessed: true,
                rows: (i, i),
            });
        }
    }
    for run in &mut out {
        run.size = run
            .size
            .max(u32::try_from(run.rows.1 - run.rows.0 + 1).unwrap_or(1));
    }
    out.retain(|run| run.size >= 2);
    out
}

/// How wide a party's tab is.
const SPINE: f32 = 13.0;

/// Draws a party's tab down the gutter beside `rows`, the rows it spans.
pub(crate) fn spine(painter: &egui::Painter, spine: &Spine, rows: Rect) {
    let tab = Rect::from_min_max(
        pos2(rows.left() - 4.0 - SPINE, rows.top()),
        pos2(rows.left() - 4.0, rows.bottom()),
    );
    // Its ends cut at the board's lean, so it is one of the board's plates
    // stood on end rather than a bar.
    let run = SPINE * LEAN * 2.0;
    let corners = [
        pos2(tab.left(), tab.top() + run),
        tab.right_top(),
        pos2(tab.right(), tab.bottom() - run),
        tab.left_bottom(),
    ];
    let ink = if spine.guessed {
        let edge = egui::Stroke::new(1.5, spine.tint);
        let mut closed = corners.to_vec();
        closed.push(corners[0]);
        painter.extend(Shape::dashed_line(&closed, edge, 4.0, 3.0));
        spine.tint
    } else {
        painter.add(Shape::convex_polygon(
            corners.to_vec(),
            spine.tint,
            egui::Stroke::NONE,
        ));
        colour::BG
    };
    // The size, sideways, reading up the tab. The long word if it fits,
    // the number if not, nothing if not even that.
    let word = match spine.size {
        2 => "Duo".to_owned(),
        3 => "Trio".to_owned(),
        n => format!("{n} stack"),
    };
    let mark = if spine.guessed { "?" } else { "" };
    let font = Face::Display.at(size::LABEL);
    for text in [format!("{word}{mark}"), format!("{}{mark}", spine.size)] {
        let long = overseer_ui::caps_width(painter, &text, font.clone());
        if 2.0f32.mul_add(run + space::SM, long) <= tab.height() {
            let galley = painter.layout_job(overseer_ui::caps(&text, font, ink));
            let tall = galley.size().y;
            let at = pos2(tab.center().x - tall / 2.0, tab.center().y + long / 2.0);
            painter.add(
                egui::epaint::TextShape::new(at, galley, ink)
                    .with_angle(-std::f32::consts::FRAC_PI_2),
            );
            return;
        }
    }
}

/// A folded corner at the top right of a slab: you wrote something about
/// this account.
pub(crate) fn dog_ear(painter: &egui::Painter, rect: Rect) {
    let fold = 9.0;
    painter.add(Shape::convex_polygon(
        vec![
            pos2(rect.right() - fold, rect.top()),
            pos2(rect.right(), rect.top()),
            pos2(rect.right(), rect.top() + fold),
        ],
        colour::TEXT_STRONG.gamma_multiply(0.8),
        egui::Stroke::NONE,
    ));
}

/// A caps label, the size every column head and chip on the board is set
/// at.
pub(crate) fn label() -> FontId {
    Face::Display.at(size::LABEL)
}

/// A reason split into runs of digits and runs of everything else, the
/// digits flagged, so the numbers in it can be set in the flag's colour.
pub(crate) fn split_numbers(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut numeric = None;
    for (i, ch) in text.char_indices() {
        let is = ch.is_ascii_digit() || (ch == '.' && numeric == Some(true)) || ch == '%';
        if numeric != Some(is) {
            if let Some(kind) = numeric
                && let Some(run) = text.get(start..i)
            {
                out.push((run, kind));
            }
            start = i;
            numeric = Some(is);
        }
    }
    if let Some(kind) = numeric
        && let Some(run) = text.get(start..)
    {
        out.push((run, kind));
    }
    out
}

/// How hot a number is for the side it belongs to.
///
/// `t` runs from nothing to worry about at 0 to as good as it gets at 1. A
/// good enemy number runs to the enemy's red and a good ally number to your
/// green. The rows, the panel and the career all use this, so one number is
/// one colour everywhere.
pub(crate) fn heat(side: Side, t: f32) -> Color32 {
    match side {
        Side::Enemy => colour::threat(t),
        Side::Ally => colour::strength(t),
    }
}

/// What a win looks like for this side: the enemy's red, or your green.
pub(crate) const fn win(side: Side) -> Color32 {
    match side {
        Side::Enemy => colour::ENEMY,
        Side::Ally => colour::ALLY,
    }
}

/// A K/D as heat, on the scale the rows use.
pub(crate) fn kd_heat(side: Side, kd: Option<f64>) -> Color32 {
    kd.map_or(colour::TEXT_FAINT, |v| heat(side, (v as f32 - 0.9) / 0.8))
}

#[cfg(test)]
mod tests {
    use super::{kd_heat, party_tints, split_numbers};
    use crate::board::Side;
    use overseer_core::{Party, Player};
    use overseer_ui::colour::{self, PARTY_OURS, PARTY_THEIRS};

    /// A K/D nobody knows is faint on either side, never the colour of a bad one.
    #[test]
    fn a_missing_kd_is_not_coloured_like_a_bad_one() {
        for side in [Side::Enemy, Side::Ally] {
            assert_eq!(kd_heat(side, None), colour::TEXT_FAINT);
            assert_ne!(kd_heat(side, Some(0.5)), colour::TEXT_FAINT);
        }
    }

    /// Each side colours its parties from its own pair, in the order of the
    /// party numbers, and a player on their own takes no colour.
    #[test]
    fn each_side_colours_its_own_parties() {
        let member = |number: u32| Player {
            party: Some(Party {
                number: Some(number),
                ..Party::default()
            }),
            ..Player::default()
        };
        let side = [member(7), member(3), member(7), member(3), member(9)];
        let side: Vec<&Player> = side.iter().collect();
        let ours = party_tints(&side, Side::Ally);
        assert_eq!(ours, [(3, PARTY_OURS[0]), (7, PARTY_OURS[1])]);
        let theirs = party_tints(&side, Side::Enemy);
        assert_eq!(theirs.first(), Some(&(3, PARTY_THEIRS[0])));
    }

    /// The numbers in a reason are found, decimals and percentages whole.
    #[test]
    fn a_reason_splits_at_its_numbers() {
        let runs = split_numbers("Level 34, K/D 1.92, won 64%");
        let numbers: Vec<&str> = runs.iter().filter(|r| r.1).map(|r| r.0).collect();
        assert_eq!(numbers, ["34", "1.92", "64%"]);
        let whole: String = runs.iter().map(|r| r.0).collect();
        assert_eq!(whole, "Level 34, K/D 1.92, won 64%");
    }
}
