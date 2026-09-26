//! The broadcast's vocabulary, drawn once: slabs, the slant, faces cut to
//! it, emblems with their light, numerals that line up, and results as
//! pips.
//!
//! Everything on the board is one of these, which is the whole of what
//! keeps it one graphic rather than a page of widgets.

use egui::epaint::Vertex;
use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect, Shape, pos2, vec2};
use overseer_core::Player;
use overseer_ui::{Face, art, colour, hex, rank, shape, size, space};

use super::Side;

/// How far the slant leans: the run of the cut per point of its height.
///
/// About twelve degrees, the angle a broadcast graphic cuts its plates at.
/// One number, so every slanted thing on the board leans the same way.
pub(crate) const LEAN: f32 = 0.21;

/// A quadrilateral whose left and right edges lean by [`LEAN`] where asked.
///
/// `lead` cuts the left edge, `tail` the right. A slab with neither is a
/// rectangle, which is what most of them are: the slant is punctuation, and
/// punctuation on every word is noise.
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

/// One slab: an object standing on the ground, with its shadow below it and
/// the light along its top edge.
///
/// The shadow is offset, because a shadow with no offset is a glow, and a
/// glow round everything is decoration. `lift` runs from resting to under
/// the pointer: the slab brightens and its shadow drops further, which is
/// what reads as it coming up to meet the pointer. No shadow at all in the
/// efficient tier, which draws no blur anywhere.
pub(crate) fn slab(painter: &egui::Painter, rect: Rect, fill: Color32, lift: f32, still: bool) {
    if !still {
        painter.add(
            egui::epaint::RectShape::filled(
                rect.translate(vec2(0.0, 3.0 + 3.0 * lift)),
                0,
                Color32::from_black_alpha((90.0 + 50.0 * lift) as u8),
            )
            .with_blur_width(8.0 + 6.0 * lift),
        );
    }
    painter.rect_filled(rect, 0, fill);
    painter.hline(
        rect.x_range(),
        rect.top() + 0.5,
        (1.0, colour::TEXT_STRONG.gamma_multiply(0.05 + 0.05 * lift)),
    );
}

/// The agent's killfeed crop, with its right edge cut to the slant.
///
/// The cut goes through the picture rather than round a plate behind it:
/// the texture is mapped straight onto the slanted quad, so the far corner
/// is simply not there. An agent this build has never heard of gets a plate
/// in their colour with their initial, because a blank at the head of the
/// newest agent's row is the row somebody most wants to read.
pub(crate) fn crop(painter: &egui::Painter, player: &Player, rect: Rect) {
    let named = player.agent.as_deref().unwrap_or("");
    let tint = hex(player.agent_color.as_deref()).unwrap_or(colour::BG_INSET);
    let points = slanted(rect, false, true);
    let Some(face) = art::killfeed(painter.ctx(), named) else {
        painter.add(Shape::convex_polygon(
            points,
            shape::blend(tint, colour::BG, 0.35),
            egui::Stroke::NONE,
        ));
        let initial: String = named.chars().take(1).collect::<String>().to_uppercase();
        painter.text(
            pos2(rect.left() + rect.height() * 0.6, rect.center().y),
            Align2::CENTER_CENTER,
            initial,
            Face::Heavy.at(rect.height() * 0.5),
            shape::ink_on(tint),
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
    let mut mesh = Mesh::with_texture(face.id());
    for point in &points {
        mesh.vertices.push(Vertex {
            pos: *point,
            uv: pos2(
                (point.x - rect.left()) / rect.width().max(1.0),
                (point.y - rect.top()) / rect.height().max(1.0),
            ),
            color: Color32::WHITE,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// A rank's emblem, standing in its own light.
///
/// The light is a bloom of the emblem's own shape in the tier's colour, so
/// it hugs the emblem and falls away the way light does. A disc of colour
/// used to sit behind it, and it read as a smudge with an edge. It is
/// stronger up the ladder: a Radiant should catch the eye before an Iron
/// does. `light` is its strength, and nothing at all is drawn for a tier
/// Riot has no emblem for.
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
        let reach = side * (stored + 2.0 * art::GLOW_PAD as f32) / stored;
        let climb = (tier as f32 / 27.0).clamp(0.0, 1.0);
        let mut mesh = Mesh::with_texture(bloom.id());
        mesh.add_rect_with_uv(
            Rect::from_center_size(centre, vec2(reach, reach)),
            whole,
            vivid(rank(Some(tier))).gamma_multiply((0.45 + 0.4 * climb) * light),
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
/// Barlow's digits are proportional: a one is barely half the width of a
/// nought, so a column of 1.11 above 0.88 above 1.92 set as plain text lines
/// up at its right edge and wobbles at its point. Each digit gets the width
/// of the widest and sits in the middle of it, which is what a tabular
/// figure is.
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
        let rect = Rect::from_min_size(pos2(x, left.y - pip * 0.7), vec2(pip, pip * 1.4));
        painter.add(slant(rect, true, true, tint));
        x += pip + 2.0;
    }
}

/// A colour as light: as saturated again and a third, at full value.
///
/// The rank palette is set to sit quietly in text and on the ladder, and
/// a light in those colours was a grey smudge round Platinum and a dull one
/// round Immortal. What a lit emblem gives off is its colour turned up.
fn vivid(tint: Color32) -> Color32 {
    let mut light = egui::ecolor::Hsva::from(tint);
    light.s = (light.s * 1.35).min(1.0);
    light.v = 1.0;
    Color32::from(light)
}

/// A party down the gutter: one tab the height of its rows, with its size
/// set sideways in it.
///
/// The single most information-dense mark on the board: a three stack plays
/// nothing like three strangers. It used to be a thin bracket drawn a row at
/// a time, which broke at every gap between rows and said nothing about
/// how many; a solid tab reads as one thing and says DUO or TRIO on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spine {
    /// The party's colour, from the party palette by the party's number.
    pub(crate) tint: Color32,
    /// How many are in it, Riot's count or the app's guess.
    pub(crate) size: u32,
    /// Whether it is Riot's word or the app's guess. A guess is an outline,
    /// because a guess drawn solid is a lie.
    pub(crate) guessed: bool,
    /// The rows it runs down, first and last, by their place in the list.
    pub(crate) rows: (usize, usize),
}

/// The party spines for a side, in the order its rows are drawn.
///
/// A party is a run of rows next to each other with the same party number;
/// a sort can split one, and then each piece has its own tab in the same
/// colour. The colour comes from the party palette by the party's number
/// rather than from the backend, which assigned the enemy's red to one of
/// your own parties. A party of one is not a party: Riot numbers a solo
/// player too, and a tab on one row alone is an ornament. A guess is a row
/// at a time, because the backend guesses a size and not who with.
pub(crate) fn spines(players: &[&Player]) -> Vec<Spine> {
    let mut out: Vec<Spine> = Vec::new();
    for (i, player) in players.iter().enumerate() {
        if let Some(party) = player.party.as_ref()
            && let Some(number) = party.number
        {
            let tint = *colour::PARTY
                .get(number as usize % colour::PARTY.len())
                .unwrap_or(&colour::INFO);
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
        2 => "duo".to_owned(),
        3 => "trio".to_owned(),
        n => format!("{n} stack"),
    };
    let mark = if spine.guessed { "?" } else { "" };
    let font = Face::Display.at(size::LABEL);
    for text in [format!("{word}{mark}"), format!("{}{mark}", spine.size)] {
        let long = overseer_ui::caps_width(painter, &text, font.clone());
        if long + 2.0 * (run + space::SM) <= tab.height() {
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
/// On the enemy's side a good number is bad news and runs to the enemy's
/// red; on yours it runs to your green. `t` is nothing to worry about at 0
/// and as good as it gets at 1. One rule, used by the rows, the panel and
/// the career alike: the panel used to colour an enemy's K/D green while the
/// row beside it coloured the same number red.
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
