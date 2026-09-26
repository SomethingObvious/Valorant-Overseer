//! The board: two teams, one row per player, everything the backend knows.
//!
//! A column is a spec rather than a guess. Width, alignment, face and
//! priority live in one table; the heading row and the data rows both read
//! from it, so a heading cannot drift off its own numbers. That is the single
//! thing that separates a table somebody built from a table somebody
//! assembled.
//!
//! There are a lot of columns, on purpose. Each one answers a question a
//! player actually asks in agent select, and the answer to "is that too much"
//! is the priority list rather than a shorter table: a narrow window sheds
//! from the right and from the bottom of the list, and anybody who wants
//! fewer can switch them off and have the choice remembered.

use egui::text::{LayoutJob, TextWrapping};
use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Player};

use crate::sort::{Direction, Sort};
use overseer_ui::{
    Face, caps, caps_at, caps_text, colour, hex, kd, motion, rank, shape, size, space,
};

/// The margin down both sides of the board.
///
/// Wide enough on the left for a party bracket to sit in it without
/// touching a row, which is the whole reason it is not the default gap: a
/// bracket drawn inside the rows would be a twelfth column, and drawn
/// against the window edge it would look like a rendering fault.
pub(crate) const GUTTER: f32 = 18.0;

/// Which way a column's content sits against its own width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    /// Words: agents, names, ranks.
    Left,
    /// Numbers. Right, always, so the digits line up under each other.
    Right,
}

/// How much a column matters when the window is too narrow for all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Priority {
    /// Never dropped. Without these there is no table.
    Core,
    /// The rest of the first glance: who they locked, how new the account
    /// is, and how it has been going for them.
    High,
    /// Worth a look once those are in.
    Mid,
    /// Interesting, and first to go.
    Low,
}

/// One column of the board.
pub(crate) struct Column {
    /// The key a person switches it off by, and the heading before it is put
    /// into caps and tracked.
    pub(crate) head: &'static str,
    /// Drawn width in points, not counting the gutter after it.
    pub(crate) width: f32,
    /// Which way the content sits.
    pub(crate) align: Align,
    /// Which face draws it. Numbers get the mono face so they cannot drift.
    pub(crate) face: Face,
    /// When it is dropped.
    pub(crate) priority: Priority,
    /// What the column answers, for the settings screen.
    pub(crate) about: &'static str,
}

/// Every column, in reading order: who they are, how they rank, how they
/// play, and what history you have with them.
pub(crate) const COLUMNS: [Column; 12] = [
    Column {
        head: "agent",
        width: 84.0,
        align: Align::Left,
        face: Face::Body,
        priority: Priority::High,
        about: "Who they locked",
    },
    Column {
        head: "player",
        width: 148.0,
        align: Align::Left,
        face: Face::Body,
        priority: Priority::Core,
        about: "Their name and tag",
    },
    Column {
        head: "rank",
        width: 104.0,
        align: Align::Left,
        face: Face::Body,
        priority: Priority::Core,
        about: "Where they are now",
    },
    Column {
        head: "rr",
        width: 64.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::Low,
        about: "Rating, and what the last match did to it",
    },
    Column {
        head: "peak",
        width: 140.0,
        align: Align::Left,
        face: Face::Body,
        priority: Priority::Mid,
        about: "The best they have ever been, and when",
    },
    Column {
        head: "k/d",
        width: 44.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::Core,
        about: "Kills over deaths, last few matches",
    },
    Column {
        head: "hs",
        width: 48.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::Low,
        about: "Headshots, over those same matches",
    },
    Column {
        head: "win",
        width: 44.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::Mid,
        about: "Career win rate",
    },
    Column {
        head: "map",
        width: 72.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::Low,
        about: "How they do on the map being played",
    },
    Column {
        head: "met",
        width: 40.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::Mid,
        about: "Lobbies you have shared with them",
    },
    Column {
        head: "lvl",
        width: 44.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::High,
        about: "Account level, which is the first smurf tell",
    },
    Column {
        head: "last 5",
        width: 62.0,
        align: Align::Left,
        face: Face::Body,
        priority: Priority::High,
        about: "Their recent results, newest first",
    },
];

/// The room kept on the right for the flags: a note mark, a smurf mark and
/// a stack guess, with a gutter before the first of them so the rightmost
/// column never runs into one.
const FLAG_WIDTH: f32 = 52.0;

/// How long the two things a row animates are allowed to take.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pace {
    /// The tint under the cursor.
    pub(crate) hover: f32,
    /// The tint on the row you picked.
    pub(crate) select: f32,
}

/// What one row needs beyond the player themselves.
pub(crate) struct RowStyle {
    /// The team's colour, which the name is drawn in.
    pub(crate) team: Color32,
    /// Whether this row is the selected one.
    pub(crate) selected: bool,
    /// How tall the row is, which the window's width decides.
    pub(crate) height: f32,
    /// How long the hover tint takes to arrive, and how long the selection
    /// takes. Both zero in the efficient tier, which is the whole of what
    /// that tier does to a row.
    pub(crate) pace: Pace,
    /// Whether you have written something about this account.
    pub(crate) noted: bool,
    /// The party bracket in the gutter, if this row is in one: the colour,
    /// and whether it is the top or the bottom of the group.
    pub(crate) bracket: Option<Bracket>,
    /// How far this row has arrived, from nothing to all the way.
    ///
    /// A roster does not appear, it lands: each row fades up and slides the
    /// last few points into place, a little after the one above it. It runs
    /// when the lobby changes and never when the numbers in it do, because
    /// animating a value that updates every second is the one motion
    /// mistake that makes an app unusable.
    pub(crate) arrive: f32,
}

/// One row's share of a party bracket.
///
/// The single most information-dense mark on the board. A three stack plays
/// nothing like three strangers, and a bracket down the gutter says so
/// without a column, a word or a number: you see the shape before you read
/// anything.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bracket {
    /// The party's own colour, as the backend assigned it.
    pub(crate) tint: Color32,
    /// Whether the bracket turns in at the top of this row.
    pub(crate) top: bool,
    /// Whether it turns in at the bottom.
    pub(crate) bottom: bool,
}

/// What a set of columns needs, gutters and margins included.
fn width_of(keep: &[&Column]) -> f32 {
    let columns: f32 = keep.iter().map(|c| c.width + space::MD).sum();
    columns + FLAG_WIDTH + space::LG + GUTTER * 2.0
}

/// Which columns fit in the width available, after the ones switched off.
///
/// Built by adding rather than by shedding. Shedding overshoots: dropping a
/// 128 point column to save 40 leaves 88 points of room that nothing is ever
/// offered, and the test that noticed said the board kept fewer columns
/// after one was switched off, which is the opposite of what switching one
/// off is for.
///
/// The first pass ignores `hidden` entirely, and that is the part that
/// matters. Whatever survives a fill of the whole table survives the fill
/// below it, so hiding one column can never take a different one away with
/// it. Without that step, hiding the agent column freed enough room for
/// peak, which is wider, which pushed win off the end: you switch one thing
/// off and a second thing you never touched disappears.
pub(crate) fn columns_for(width: f32, hidden: &[String]) -> Vec<&'static Column> {
    let shown = fill(width, &[], &[]);
    let kept: Vec<&Column> = shown
        .into_iter()
        .filter(|c| !hidden.iter().any(|h| h == c.head))
        .collect();
    fill(width, &kept, hidden)
}

/// Adds columns in priority order, keeping any that still fit.
///
/// In priority order and then in table order, so a narrow window sheds from
/// the bottom of the priority list rather than from the right of the table,
/// and the board reads the same however many columns are on.
fn fill(width: f32, start: &[&'static Column], hidden: &[String]) -> Vec<&'static Column> {
    let visible = |c: &&Column| !hidden.iter().any(|h| h == c.head);
    let mut keep: Vec<&Column> = start.to_vec();
    // The core columns are not negotiable: without them there is no table,
    // and a window too narrow for them is a window that gets a wide row.
    for column in COLUMNS.iter().filter(|c| c.priority == Priority::Core) {
        if visible(&column) && !keep.iter().any(|c| c.head == column.head) {
            keep.push(column);
        }
    }
    for priority in [Priority::High, Priority::Mid, Priority::Low] {
        for column in COLUMNS
            .iter()
            .filter(|c| c.priority == priority)
            .filter(visible)
        {
            if keep.iter().any(|c| c.head == column.head) {
                continue;
            }
            let mut candidate = keep.clone();
            candidate.push(column);
            candidate.sort_by_key(|c| COLUMNS.iter().position(|o| o.head == c.head).unwrap_or(0));
            if width_of(&candidate) <= width {
                keep = candidate;
            }
        }
    }
    keep.sort_by_key(|c| COLUMNS.iter().position(|o| o.head == c.head).unwrap_or(0));
    keep
}

/// Draws one cell, ending in an ellipsis rather than mid glyph.
///
/// A severed letter reads as a bug; three dots read as "there is more". Both
/// cost one layout, so there is no reason to take the one that looks broken.
fn cell_text(
    painter: &egui::Painter,
    text: &str,
    font: FontId,
    tint: Color32,
    rect: Rect,
    align: Align,
) {
    if text.is_empty() {
        return;
    }
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, tint);
    job.wrap = TextWrapping {
        max_width: rect.width(),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    let y = rect.center().y - galley.size().y / 2.0;
    let x = match align {
        Align::Left => rect.left(),
        Align::Right => rect.right() - galley.size().x,
    };
    painter.galley(pos2(x, y), galley, tint);
}

/// The heading row, drawn from the same spec the data uses.
///
/// Returns the heading that was clicked, because a heading you can click is
/// the shortest way there is to ask "who is the best player here".
pub(crate) fn headings(
    ui: &mut Ui,
    width: f32,
    hidden: &[String],
    sort: &Sort,
) -> Option<&'static str> {
    let (rect, _response) = ui.allocate_exact_size(vec2(width, space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let painter = ui.painter().clone();
    let inner = Rect::from_min_max(
        pos2(rect.left() + GUTTER, rect.top()),
        pos2(rect.right() - GUTTER, rect.bottom()),
    );
    let mut clicked = None;
    let mut x = inner.left() + space::LG;
    for column in columns_for(width, hidden) {
        let hit = Rect::from_min_size(
            pos2(x - space::SM, inner.top()),
            vec2(column.width + space::MD, inner.height()),
        );
        let response = ui.interact(hit, ui.id().with(("head", column.head)), Sense::click());
        if response.clicked() {
            clicked = Some(column.head);
        }
        let sorted = sort.arrow(column.head);
        let tint = if sorted.is_some() {
            colour::TEXT
        } else if response.hovered() {
            colour::TEXT_DIM
        } else {
            colour::TEXT_FAINT
        };
        let (pos, anchor) = match column.align {
            Align::Left => (pos2(x, inner.center().y), Align2::LEFT_CENTER),
            Align::Right => (
                pos2(x + column.width, inner.center().y),
                Align2::RIGHT_CENTER,
            ),
        };
        let drawn = caps_text(
            &painter,
            pos,
            anchor,
            column.head,
            Face::Display.at(size::MICRO),
            tint,
        );
        if let Some(direction) = sorted {
            arrow(&painter, drawn, column.align, direction);
        }
        x += column.width + space::MD;
    }
    painter.hline(
        inner.left() + space::LG..=inner.right(),
        inner.bottom() - 1.0,
        (1.0, colour::LINE),
    );
    clicked
}

/// The mark on the sorted heading: a triangle, painted rather than typed.
///
/// A glyph would be at the mercy of the face having it, and of how wide the
/// world thinks that glyph is. Three points are three points.
fn arrow(painter: &egui::Painter, label: Rect, align: Align, direction: Direction) {
    let x = match align {
        Align::Left => label.right() + space::SM + 3.0,
        Align::Right => label.left() - space::SM - 3.0,
    };
    let y = label.center().y;
    let (base, point) = match direction {
        Direction::Down => (y - 2.0, y + 2.5),
        Direction::Up => (y + 2.0, y - 2.5),
    };
    painter.add(egui::Shape::convex_polygon(
        vec![pos2(x - 3.0, base), pos2(x + 3.0, base), pos2(x, point)],
        colour::INFO,
        egui::Stroke::NONE,
    ));
}

/// One player. Painted rather than assembled out of widgets: a row is a
/// handful of strings at known offsets, and going through egui's layout for
/// each of them costs more than the row is worth when ten redraw together.
pub(crate) fn row(ui: &mut Ui, player: &Player, style: &RowStyle, hidden: &[String]) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, style.height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let mut painter = ui.painter().clone();
    if style.arrive < 1.0 {
        painter.multiply_opacity(style.arrive);
    }

    // Both tints arrive over time rather than at once, which is the
    // difference between a cursor that feels attached to the app and one
    // that snaps. Selection is the slower of the two because it is a
    // decision rather than a movement.
    let hover = motion::eased(ui.ctx().animate_bool_with_time(
        response.id,
        response.hovered(),
        style.pace.hover,
    ));
    let chosen = motion::eased(ui.ctx().animate_bool_with_time(
        response.id.with("selected"),
        style.selected,
        style.pace.select,
    ));
    let slide = (1.0 - style.arrive) * 12.0;
    let inner = Rect::from_min_max(
        pos2(rect.left() + GUTTER + slide, rect.top()),
        pos2(rect.right() - GUTTER + slide, rect.bottom()),
    );
    furniture(&painter, player, style, inner, hover, chosen);
    flags(&painter, player, inner, style.noted);
    painter.hline(
        inner.left() + space::LG..=inner.right(),
        inner.bottom(),
        (1.0, colour::LINE_SOFT),
    );

    // An account the backend could not see at all. A row of dashes across
    // ten columns reads as the app being broken; one quiet word reads as the
    // truth, which is that Riot did not say.
    if player.name.is_none() && player.agent.is_none() && player.rank.is_none() {
        painter.text(
            pos2(inner.left() + space::LG, inner.center().y),
            Align2::LEFT_CENTER,
            "not visible",
            Face::Body.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        return response;
    }

    let name_colour = if player.is_self {
        colour::YOU
    } else {
        colour::TEXT_STRONG
    };
    let mut x = inner.left() + space::LG;
    for column in columns_for(width, hidden) {
        let cell_rect = Rect::from_min_size(pos2(x, rect.top()), vec2(column.width, rect.height()));
        match column.head {
            "rr" => rr_cell(&painter, player, cell_rect),
            "last 5" => form_cell(&painter, player, cell_rect),
            "rank" => rank_cell(&painter, player, cell_rect),
            "agent" => agent_cell(&painter, player, cell_rect),
            _ => {
                let (text, tint) = cell(column.head, player, name_colour);
                cell_text(
                    &painter,
                    &text,
                    column.face.at(size::BODY),
                    tint,
                    cell_rect,
                    column.align,
                );
            }
        }
        x += column.width + space::MD;
    }
    why(ui, player, style.arrive);
    response
}

/// Why a flagged account is flagged, on a line of its own under their row.
///
/// The mark at the end of a row says "look at this one" and nothing else,
/// and until now the answer lived in the panel, one account at a time. That
/// is five hovers to read five reasons, and in the overlay, which has no
/// panel, it was not readable at all. The reasons are short, there are
/// rarely more than two accounts carrying them, and the question the mark
/// raises is answered on the next line down where it was asked.
fn why(ui: &mut Ui, player: &Player, arrive: f32) {
    if !player.smurf || player.smurf_reasons.is_empty() {
        return;
    }
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::LG + space::SM),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let slide = (1.0 - arrive) * 12.0;
    let left = rect.left() + GUTTER + space::LG + slide;
    // The same gutter mark the rows above and below carry, so the line reads
    // as belonging to the row rather than as a second row.
    painter.add(shape::tick(
        pos2(left, rect.top() + 1.0),
        rect.height() - 4.0,
        colour::WARN.gamma_multiply(0.7 * arrive),
    ));
    // As many reasons as the width holds, cut at a word with an ellipsis
    // rather than at a letter: the backend sends four of these and a narrow
    // window has room for one. The panel carries all of them in full, which
    // is what the ellipsis is pointing at.
    let tint = colour::WARN.gamma_multiply(0.85 * arrive);
    let mut job = LayoutJob::simple_singleline(
        player.smurf_reasons.join("  \u{b7}  "),
        Face::Body.at(size::MICRO),
        tint,
    );
    job.wrap = TextWrapping {
        max_width: rect.right() - GUTTER - space::LG - (left + space::MD),
        max_rows: 1,
        break_anywhere: false,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(left + space::MD, rect.center().y - galley.size().y / 2.0),
        galley,
        tint,
    );
}

/// Everything on a row that is not a value: the tints, the rail and the
/// party bracket.
///
/// The rail is the agent's own colour, which is the cheapest texture this
/// board can have and is real information: ten rails tell you the lobby's
/// composition before you have read a word. Your own row overrides it with
/// bone, because in game your row is the light one and a companion app that
/// moves that is worse than one with no colour at all.
fn furniture(
    painter: &egui::Painter,
    player: &Player,
    style: &RowStyle,
    inner: Rect,
    hover: f32,
    chosen: f32,
) {
    // Left to right, because the rail is on the left and the tint should
    // look like it came from it.
    if chosen > 0.0 {
        painter.add(egui::Shape::gradient_rect(
            inner,
            egui::Direction::LeftToRight,
            [
                colour::BG_SELECTED.gamma_multiply(chosen),
                colour::BG_SELECTED.gamma_multiply(chosen * 0.55),
            ],
        ));
    }
    if hover > 0.0 && chosen < 1.0 {
        let fade = hover * (1.0 - chosen);
        painter.add(egui::Shape::gradient_rect(
            inner,
            egui::Direction::LeftToRight,
            [
                colour::BG_HOVER.gamma_multiply(fade),
                colour::BG_HOVER.gamma_multiply(fade * 0.4),
            ],
        ));
    }
    let lift = chosen.max(hover).max(f32::from(player.is_self));
    let rail_tint = if player.is_self {
        colour::YOU
    } else {
        hex(player.agent_color.as_deref()).unwrap_or(style.team)
    };
    painter.rect_filled(
        Rect::from_min_size(inner.min, vec2(3.0 + 2.0 * lift, inner.height())),
        0,
        rail_tint.gamma_multiply(0.45 + 0.55 * lift),
    );
    let Some(bracket) = style.bracket else { return };
    let x = inner.left() - 10.0;
    let (top, bottom) = (inner.top(), inner.bottom());
    painter.vline(x, top..=bottom, (2.0, bracket.tint));
    for (on, y) in [(bracket.top, top + 1.0), (bracket.bottom, bottom - 1.0)] {
        if on {
            painter.hline(x..=x + 6.0, y, (2.0, bracket.tint));
        }
    }
}

/// The agent's own mark: their colour, chamfered, with their initial on it.
///
/// Shared, because the row and the ladder both draw one and the same person
/// has to look like the same person in both. Eighteen points is the size at
/// which a single letter is still a letter rather than a texture.
fn agent_tile(painter: &egui::Painter, player: &Player, at: Pos2) -> Rect {
    let tint = hex(player.agent_color.as_deref()).unwrap_or(colour::TEXT_DIM);
    let tile = Rect::from_center_size(at, vec2(18.0, 18.0));
    painter.add(shape::cut_wash(
        tile,
        4.0,
        shape::blend(tint, colour::TEXT_STRONG, 0.18),
        tint,
    ));
    let initial: String = player
        .agent
        .as_deref()
        .unwrap_or("?")
        .chars()
        .take(1)
        .collect::<String>()
        .to_uppercase();
    painter.text(
        tile.center(),
        Align2::CENTER_CENTER,
        initial,
        Face::Display.at(size::LABEL),
        shape::ink_on(tint),
    );
    tile
}

/// The agent, as a tile with their initial on it.
///
/// A word in a column is a word to read; a coloured tile is a shape to
/// recognise, and after two matches you know the composition of a lobby
/// from the left edge of the board without reading anything. It is the
/// closest this app can get to the portrait every other tracker shows
/// without going to Riot's servers for a picture, which is a thing this app
/// does not do.
fn agent_cell(painter: &egui::Painter, player: &Player, rect: Rect) {
    let Some(agent) = player.agent.as_deref().filter(|a| !a.is_empty()) else {
        painter.text(
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            "-",
            Face::Body.at(size::BODY),
            colour::TEXT_FAINT,
        );
        return;
    };
    let tile = agent_tile(painter, player, pos2(rect.left() + 9.0, rect.center().y));
    cell_text(
        painter,
        agent,
        Face::Body.at(size::BODY),
        colour::TEXT,
        Rect::from_min_max(pos2(tile.right() + space::MD, rect.top()), rect.max),
        Align::Left,
    );
}

/// The rank division, as the game draws it: a stack of chevrons.
///
/// One chevron for the first division of a tier, three for the third, in
/// the tier's own colour. It is the mark a player reads before the word
/// beside it, and the whole reason the colours are Riot's exact ones.
fn chevrons(painter: &egui::Painter, tier: u32, at: Pos2, tint: Color32) {
    // Tier 0 to 2 is unranked and has no divisions; Radiant is one tier of
    // one and gets a single mark rather than a third of one.
    if tier < 3 {
        return;
    }
    let division = if tier >= 27 { 3 } else { tier % 3 + 1 };
    for i in 0..division {
        let y = at.y - 5.0 + i as f32 * 4.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(at.x - 4.0, y + 2.5),
                pos2(at.x, y - 0.5),
                pos2(at.x + 4.0, y + 2.5),
                pos2(at.x, y + 1.0),
            ],
            tint,
            egui::Stroke::NONE,
        ));
    }
}

/// The rank, on a plate in its own colour.
///
/// A plate rather than coloured text because rank is the one value on the
/// row that is a category rather than a measurement, and because eight tiers
/// of coloured text at the same weight is eight colours that cancel out.
fn rank_cell(painter: &egui::Painter, player: &Player, rect: Rect) {
    let Some(name) = player.rank.as_deref().filter(|n| !n.is_empty()) else {
        painter.text(
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            "-",
            Face::Body.at(size::BODY),
            colour::TEXT_FAINT,
        );
        return;
    };
    let tier = player.rank_tier.unwrap_or(0);
    let tint = rank(player.rank_tier);
    let galley = painter.layout_no_wrap(name.to_owned(), Face::Body.at(size::BODY), tint);
    let badge = if tier >= 3 { 14.0 } else { 0.0 };
    let plate = Rect::from_min_size(
        pos2(rect.left(), rect.center().y - 9.0),
        vec2(
            (galley.size().x + space::MD * 2.0 + badge).min(rect.width()),
            18.0,
        ),
    );
    // Unranked is a state rather than a tier, so it gets no plate: a grey
    // plate next to nine coloured ones reads as a tenth rank.
    if tier > 0 {
        painter.add(shape::cut_wash(
            plate,
            4.0,
            tint.gamma_multiply(0.26),
            tint.gamma_multiply(0.10),
        ));
    }
    chevrons(
        painter,
        tier,
        pos2(plate.left() + space::MD + 4.0, plate.center().y),
        tint,
    );
    painter.galley(
        pos2(
            plate.left() + space::MD + badge,
            plate.center().y - galley.size().y / 2.0,
        ),
        galley,
        tint,
    );
}

/// The marks on the right: a note you left, a flag, and a group.
fn flags(painter: &egui::Painter, player: &Player, rect: Rect, noted: bool) {
    let mut x = rect.right() - space::LG;
    if noted {
        // A small square rather than a letter: it means "there is something
        // in the panel", and it must not compete with the flag beside it.
        let mark = Rect::from_min_size(pos2(x - 6.0, rect.center().y - 3.0), vec2(6.0, 6.0));
        painter.rect_filled(mark, 0, colour::INFO);
        x = mark.left() - space::MD;
    }
    if player.smurf {
        // A filled triangle rather than an exclamation mark. The mark that
        // answers "which of these five" has to be found without reading,
        // and a glyph in a column of glyphs is not.
        let (cx, cy) = (x - 6.0, rect.center().y);
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(cx, cy - 6.0),
                pos2(cx + 6.0, cy + 5.0),
                pos2(cx - 6.0, cy + 5.0),
            ],
            colour::WARN,
            egui::Stroke::NONE,
        ));
        painter.text(
            pos2(cx, cy + 1.0),
            Align2::CENTER_CENTER,
            "!",
            Face::Display.at(size::MICRO),
            colour::VOID,
        );
        x = cx - 6.0 - space::MD;
    }
    // A party Riot told us about is drawn as a bracket down the gutter, so
    // there is nothing to say here. A stack the app only inferred cannot be
    // bracketed, because inference does not say who with, and it gets a
    // question mark instead.
    if player.party.is_none() && player.stack_guess.is_some() {
        painter.text(
            pos2(x, rect.center().y),
            Align2::RIGHT_CENTER,
            "?",
            Face::Number.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

/// Rating, and what the last match did to it, in one cell.
fn rr_cell(painter: &egui::Painter, player: &Player, rect: Rect) {
    let Some(rr) = player.rr else {
        cell_text(
            painter,
            "-",
            Face::Number.at(size::BODY),
            colour::TEXT_FAINT,
            rect,
            Align::Right,
        );
        return;
    };
    let delta = player.rr_earned.unwrap_or(0);
    let (text, tint) = match delta.signum() {
        1 => (format!("+{delta}"), colour::GOOD),
        -1 => (delta.to_string(), colour::BAD),
        _ => (String::new(), colour::TEXT_FAINT),
    };
    let delta_rect = Rect::from_min_size(
        pos2(rect.right() - 26.0, rect.top()),
        vec2(26.0, rect.height()),
    );
    cell_text(
        painter,
        &text,
        Face::Number.at(size::MICRO),
        tint,
        delta_rect,
        Align::Right,
    );
    let value_rect = Rect::from_min_size(rect.min, vec2(rect.width() - 28.0, rect.height()));
    cell_text(
        painter,
        &rr.to_string(),
        Face::Number.at(size::BODY),
        colour::TEXT,
        value_rect,
        Align::Right,
    );
}

/// The last few results as pips, newest first.
///
/// Pips rather than letters because the question is "how has it been going",
/// which is a shape rather than a word. Five squares answer it without being
/// read, and the streak count sits on the end when there is one worth saying.
fn form_cell(painter: &egui::Painter, player: &Player, rect: Rect) {
    let mut x = rect.left();
    let pips = player.form.len().min(5) as f32 * (space::PIP + 2.0);
    for result in player.form.iter().take(5) {
        let tint = match result.chars().next() {
            Some('W' | 'w') => colour::ALLY,
            Some('D' | 'd') => colour::WARN,
            _ => colour::ENEMY,
        };
        let pip = Rect::from_min_size(
            pos2(x, rect.center().y - space::PIP / 2.0),
            vec2(space::PIP - 2.0, space::PIP),
        );
        painter.rect_filled(pip, 0, tint.gamma_multiply(0.85));
        x += space::PIP + 2.0;
    }
    // Only if the pips left room for it. On a narrow window this column is
    // the one that gets squeezed, and a streak count printed over the flag
    // beside it is worse than no streak count.
    let room = rect.width() - pips > 18.0;
    if let Some(streak) = player
        .streak
        .as_ref()
        .filter(|s| room && s.count.unwrap_or(0) >= 3)
    {
        let count = streak.count.unwrap_or(0);
        let tint = if streak.kind.as_deref() == Some("W") {
            colour::ALLY
        } else {
            colour::ENEMY
        };
        painter.text(
            pos2(rect.right(), rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{count}"),
            Face::Number.at(size::MICRO),
            tint,
        );
    }
}

/// What one column says about one player, and in what colour.
fn cell(head: &str, player: &Player, name_colour: Color32) -> (String, Color32) {
    match head {
        // The agent's own colour is already on the rail at the left of the
        // row, so the word itself is just a word.
        "agent" => (
            player.agent.clone().unwrap_or_else(dash),
            player
                .agent
                .as_ref()
                .map_or(colour::TEXT_FAINT, |_| colour::TEXT),
        ),
        "player" => (player.display_name().to_owned(), name_colour),
        "rank" => (
            player.rank.clone().unwrap_or_else(dash),
            rank(player.rank_tier),
        ),
        "peak" => (peak_text(player), peak_colour(player)),
        "k/d" => (number(player.kd), kd(player.kd)),
        "hs" => (
            player.hs_pct.map_or_else(dash, |v| format!("{v:.0}%")),
            player.hs_pct.map_or(colour::TEXT_FAINT, |v| {
                if v >= 30.0 {
                    colour::GOOD
                } else {
                    colour::NEUTRAL
                }
            }),
        ),
        "win" => (
            player
                .win_rate
                .map_or_else(dash, |v| format!("{}%", v.round())),
            win_colour(player.win_rate),
        ),
        "map" => map_cell(player),
        "met" => {
            let met = player.met();
            if met == 0 {
                (dash(), colour::TEXT_FAINT)
            } else {
                (format!("{met}x"), colour::WARN)
            }
        }
        "lvl" => (
            player.level.map_or_else(dash, |l| l.to_string()),
            level_colour(player),
        ),
        _ => (dash(), colour::TEXT_FAINT),
    }
}

/// The peak, with the act it was reached in, shortened.
///
/// "V25 Act 3" becomes "V25A3", because a peak with no date reads as a
/// current rank and the long form does not fit beside eleven other columns.
/// The panel has room for Riot's own wording and uses it.
fn peak_text(player: &Player) -> String {
    let Some(peak) = player.peak_rank.as_deref() else {
        return dash();
    };
    player.peak_act.as_deref().map_or_else(
        || peak.to_owned(),
        |act| format!("{peak}  {}", short_act(act)),
    )
}

/// "V25 Act 3" as "V25A3", and anything unexpected left alone but tightened.
fn short_act(act: &str) -> String {
    let mut out = String::with_capacity(act.len());
    let mut parts = act.split_whitespace();
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for part in parts {
        if part.eq_ignore_ascii_case("act") {
            out.push('A');
        } else {
            out.push_str(part);
        }
    }
    out
}

/// Gold when they are well below their own peak, which is the thing worth
/// noticing about a peak at all.
fn peak_colour(player: &Player) -> Color32 {
    let gap = player
        .peak_rank_tier
        .unwrap_or(0)
        .saturating_sub(player.rank_tier.unwrap_or(0));
    // Two whole ranks, not one. Three tiers is a single rank and half a
    // lobby is a rank off its peak at any time, so colouring that made the
    // peak column a wall of orange and the colour meant nothing.
    if gap >= 6 {
        colour::WARN
    } else {
        rank(player.peak_rank_tier)
    }
}

/// How they do on the map being played, which is the only map that matters.
fn map_cell(player: &Player) -> (String, Color32) {
    let Some(map) = player.map_win_rate.as_ref() else {
        return (dash(), colour::TEXT_FAINT);
    };
    let games = map.games.unwrap_or(0);
    if games == 0 {
        return (dash(), colour::TEXT_FAINT);
    }
    let rate = map.win_rate.unwrap_or(0.0);
    (
        format!("{}% {games}g", rate.round()),
        win_colour(Some(rate)),
    )
}

/// Above half is good, below it is not, and nothing is neither.
fn win_colour(rate: Option<f64>) -> Color32 {
    match rate {
        None => colour::TEXT_FAINT,
        // Only the ends. Half of every lobby is within a few points of even,
        // and tinting all of them spends the two colours that were supposed
        // to mean something on the players they mean nothing about.
        Some(v) if v >= 57.0 => colour::GOOD,
        Some(v) if v <= 43.0 => colour::BAD,
        Some(_) => colour::NEUTRAL,
    }
}

/// A low level on a high rank is the first smurf tell, so it is coloured
/// rather than left to be worked out.
const fn level_colour(player: &Player) -> Color32 {
    match player.level {
        Some(l) if l < 60 && !player.level_hidden => colour::WARN,
        _ => colour::TEXT_FAINT,
    }
}

/// Today, under the board: what the session has cost or paid.
///
/// The board is ten rows tall and a window is not, so there is always space
/// under it. Air is the honest thing to put there only if nothing useful
/// fits, and something does: every match you have played since you sat down,
/// which is the one number that decides whether to queue again and the one
/// the game itself will not show you until you go looking.
pub(crate) fn session(ui: &mut Ui, board: &Board) {
    let Some(session) = board.session.as_ref().filter(|s| !s.points.is_empty()) else {
        return;
    };
    ui.add_space(space::XL);
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::ROW + space::MD),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let band = Rect::from_min_max(
        pos2(rect.left() + GUTTER, rect.top()),
        pos2(rect.right() - GUTTER, rect.bottom() - space::SM),
    );
    painter.hline(band.x_range(), band.top(), (1.0, colour::LINE));

    let mut x = band.left() + space::SM;
    let after = caps_text(
        &painter,
        pos2(x, band.center().y),
        Align2::LEFT_CENTER,
        "session",
        Face::Display.at(size::LABEL),
        colour::TEXT_DIM,
    );
    x = after.right() + space::LG;

    let net = session
        .net
        .unwrap_or_else(|| session.points.iter().filter_map(|p| p.delta).sum());
    let tint = match net.signum() {
        1 => colour::GOOD,
        -1 => colour::BAD,
        _ => colour::TEXT_DIM,
    };
    let after = painter.text(
        pos2(x, band.center().y),
        Align2::LEFT_CENTER,
        format!("{net:+}"),
        Face::Number.at(size::TITLE),
        tint,
    );
    let after = caps_text(
        &painter,
        pos2(after.right() + space::SM, band.center().y + 1.0),
        Align2::LEFT_CENTER,
        "rr",
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    x = after.right() + space::XL;

    results(&painter, session, band, x);

    let won = session
        .points
        .iter()
        .filter(|p| {
            p.result
                .as_deref()
                .is_some_and(|r| r.eq_ignore_ascii_case("victory"))
        })
        .count();
    let played = session.points.len();
    caps_at(
        &painter,
        pos2(band.right() - space::SM, band.center().y),
        Align2::RIGHT_CENTER,
        &format!("{won} of {played} won"),
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
}

/// One block per match of the session, oldest left, in the colour of the
/// result.
///
/// Five greens and a red is a session you can read at a glance. "+37" on
/// its own is a number you have to think about.
fn results(painter: &egui::Painter, session: &overseer_core::Session, band: Rect, from: f32) {
    let mut x = from;
    for point in &session.points {
        let tint = match point.delta.unwrap_or(0).signum() {
            1 => colour::ALLY,
            -1 => colour::ENEMY,
            _ => colour::TEXT_FAINT,
        };
        let block = Rect::from_min_size(pos2(x, band.center().y - 5.0), vec2(14.0, 10.0));
        if block.right() > band.right() - 140.0 {
            break;
        }
        painter.rect_filled(block, 0, tint.gamma_multiply(0.85));
        x = block.right() + 3.0;
    }
}

/// Whatever the backend wants said, and what the session has cost, at the
/// foot of the board.
///
/// Pushed to the bottom of whatever room is left rather than sitting
/// directly under the last row. Ten rows never fill a window, and a strip
/// floating in the middle of the space reads as the board having stopped
/// early; the same strip against the bottom edge reads as a footer.
pub(crate) fn board_foot(ui: &mut Ui, board: &Board) {
    let wanted = space::ROW + space::MD + space::XL;
    // The ladder is the first thing to go on a short window. It is the only
    // block down here that restates something the rows already said, and a
    // picture is worth less than the rows it would push off the screen.
    let room = ui.available_height() - wanted > LADDER + space::XXL;
    let spare = ui.available_height() - wanted - if room { LADDER + space::XL } else { 0.0 };
    if spare > 0.0 {
        ui.add_space(spare);
    }
    if room {
        ladder(ui, board);
    }
    notice(ui, board);
    session(ui, board);
}
/// How tall the ladder block is, when the window has room for it.
///
/// A heading, a row of marks, the strip, and a row of marks back. Nothing in
/// it is arbitrary except the air between the marks and the strip, which is
/// the smallest gap at which a stem reads as a stem.
pub(crate) const LADDER: f32 = 84.0;

/// Where one account sits on the ladder, in tiers times a hundred.
///
/// Riot number tiers three to a rank from Iron at three, and rating runs
/// nought to a hundred inside one. Multiplying out gives a single axis that
/// is linear in the only sense that matters: one tier is one tier wide
/// everywhere along it.
fn rung(player: &Player) -> Option<f32> {
    let tier = player.rank_tier.filter(|t| *t >= 3)?;
    let rr = player.rr.unwrap_or(0).clamp(0, 99) as f32;
    Some(tier as f32 * 100.0 + rr)
}

/// How wide the whole axis is, in rating.
fn axis(first: u32, last: u32) -> f32 {
    (last - first + 1) as f32 * 100.0
}

/// The whole lobby on one ladder, enemies above it and allies below.
///
/// Every other answer on this board is a number per person, so the question
/// asked before any other, which is whether this lobby is above you or below
/// you, costs ten readings and some arithmetic. A ladder answers it without
/// being read. The strip is Riot's own tier colours in Riot's own order, so
/// the bands are a scale the player already knows by sight, and every mark
/// on it is the same tile that person has out on their own row.
pub(crate) fn ladder(ui: &mut Ui, board: &Board) {
    let placed: Vec<(&Player, f32)> = board
        .players
        .iter()
        .filter_map(|p| rung(p).map(|at| (p, at)))
        .collect();
    let (Some(first), Some(last)) = (
        placed.iter().filter_map(|(p, _)| p.rank_tier).min(),
        placed.iter().filter_map(|(p, _)| p.rank_tier).max(),
    ) else {
        return;
    };
    if placed.len() < 2 {
        return;
    }
    // Out to whole ranks rather than whole tiers. A strip that starts at
    // Bronze 3 has a band one tier wide at each end, too narrow to carry its
    // own name, and the two bands nobody can name are the two the eye goes
    // to first. Three tiers each and every band on the scale says what it is.
    let (first, last) = (first.div_euclid(3) * 3, last.div_euclid(3) * 3 + 2);
    ui.add_space(space::XL);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), LADDER), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let band = Rect::from_min_max(
        pos2(rect.left() + GUTTER, rect.top()),
        pos2(rect.right() - GUTTER, rect.bottom()),
    );
    painter.hline(band.x_range(), band.top(), (1.0, colour::LINE));
    let head = band.top() + space::LG;
    caps_at(
        &painter,
        pos2(band.left() + space::SM, head),
        Align2::LEFT_CENTER,
        "ladder",
        Face::Display.at(size::LABEL),
        colour::TEXT_DIM,
    );
    caps_at(
        &painter,
        pos2(band.right() - space::SM, head),
        Align2::RIGHT_CENTER,
        &standing(board, &placed),
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );

    let strip = Rect::from_min_max(
        pos2(band.left(), band.top() + 46.0),
        pos2(band.right(), band.top() + 56.0),
    );
    let step = band.width() / (last - first + 1) as f32;
    bands(&painter, first, last, strip, step);
    rungs(&painter, first, last, strip, step);

    let span = axis(first, last);
    let floor = first as f32 * 100.0;
    let at = |rung: f32| band.left() + (rung - floor) / span * band.width();
    for (_label, tint, team) in teams(board, true) {
        let mine: Vec<(&Player, f32)> = placed
            .iter()
            .filter(|(p, _)| p.team.as_deref() == Some(team.as_str()))
            .copied()
            .collect();
        let above = tint == colour::ENEMY;
        let y = if above {
            band.top() + 32.0
        } else {
            band.top() + 70.0
        };
        marks(
            &painter,
            &mine,
            &at,
            Row {
                band,
                y,
                above,
                strip,
            },
        );
    }
}

/// Where one team's marks go and which way they face.
#[derive(Debug, Clone, Copy)]
struct Row {
    /// The block the marks have to stay inside.
    band: Rect,
    /// The middle of the row of marks.
    y: f32,
    /// Above the strip, which is where the enemy go.
    above: bool,
    /// The strip itself, which the stems have to reach.
    strip: Rect,
}

/// One team's marks, nudged apart where two of them land on the same point.
///
/// Two tiles overlapping is two people you cannot count, and on this strip
/// the count is half the answer. So a tile that would land inside the one
/// before it is pushed clear, which costs a little accuracy on a lobby that
/// is all one rank and buys back the ability to see that it is. The stem
/// stays on the true point, so nothing is being hidden.
fn marks(painter: &egui::Painter, team: &[(&Player, f32)], at: &dyn Fn(f32) -> f32, row: Row) {
    let mut ordered: Vec<(&Player, f32)> = team.to_vec();
    ordered.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut taken = f32::MIN;
    for (player, rung) in ordered {
        let true_x = at(rung).clamp(row.band.left() + 9.0, row.band.right() - 9.0);
        let x = true_x.max(taken + 20.0);
        taken = x;
        let (from, to) = if row.above {
            (row.y + 9.0, row.strip.top())
        } else {
            (row.strip.bottom(), row.y - 9.0)
        };
        painter.vline(true_x, from..=to, (1.0, colour::LINE));
        if (x - true_x).abs() > 1.0 {
            let shelf = if row.above { row.y + 9.0 } else { row.y - 9.0 };
            painter.hline(true_x.min(x)..=true_x.max(x), shelf, (1.0, colour::LINE));
        }
        let tile = agent_tile(painter, player, pos2(x, row.y));
        if player.is_self {
            painter.rect_stroke(
                tile.expand(2.0),
                0,
                egui::Stroke::new(1.0, colour::YOU),
                egui::StrokeKind::Outside,
            );
        }
    }
}

/// The strip itself: one segment per tier, in that tier's own colour.
fn bands(painter: &egui::Painter, first: u32, last: u32, strip: Rect, step: f32) {
    for tier in first..=last {
        let from = strip.left() + (tier - first) as f32 * step;
        let segment =
            Rect::from_min_max(pos2(from, strip.top()), pos2(from + step, strip.bottom()));
        let tint = rank(Some(tier));
        painter.add(egui::Shape::gradient_rect(
            segment,
            egui::Direction::TopDown,
            [tint.gamma_multiply(0.74), tint.gamma_multiply(0.42)],
        ));
        // A hairline at every tier, so the strip is a scale with marks on it
        // rather than a few wide bands of colour.
        if tier > first {
            painter.vline(from, strip.y_range(), (1.0, colour::VOID));
        }
    }
}

/// The name of each rank group, set into its own band.
///
/// The colours are Riot's and most players read them without help, but most
/// is not all, and a scale nobody can name is a decoration. A name is only
/// set where the band is wide enough to hold it with air on both sides:
/// half a word cropped by a tier boundary would be worse than the colour on
/// its own.
fn rungs(painter: &egui::Painter, first: u32, last: u32, strip: Rect, step: f32) {
    for group in first.div_euclid(3)..=last.div_euclid(3) {
        let from = group.saturating_mul(3).max(first);
        let to = (group.saturating_mul(3) + 2).min(last);
        let left = strip.left() + (from - first) as f32 * step;
        let right = strip.left() + (to + 1 - first) as f32 * step;
        let name = overseer_ui::rank_group(from);
        let font = Face::Display.at(size::MICRO);
        if overseer_ui::caps_width(painter, name, font.clone()) + space::XL > right - left {
            continue;
        }
        // Against the band as it is actually painted, not against the pure
        // tier colour: the strip is washed down towards the ground it sits
        // on, and ink chosen for the swatch is ink chosen for a colour that
        // is not on the screen.
        let tint = shape::ink_on(rank(Some(from)).gamma_multiply(0.58));
        caps_at(
            painter,
            pos2(f32::midpoint(left, right), strip.center().y + 0.5),
            Align2::CENTER_CENTER,
            name,
            font,
            tint.gamma_multiply(0.8),
        );
    }
}

/// The one sentence the ladder is worth saying out loud.
///
/// "Three of them above you" is the whole reason to look at it, and it is
/// the one thing a picture of ten marks does not say by itself.
fn standing(board: &Board, placed: &[(&Player, f32)]) -> String {
    let unranked = board.players.len().saturating_sub(placed.len());
    let tail = if unranked == 0 {
        String::new()
    } else {
        format!(" \u{b7} {unranked} off the ladder")
    };
    let ours = board.self_team.as_deref().unwrap_or("Blue");
    let Some((_me, mine)) = placed.iter().find(|(p, _)| p.is_self) else {
        let low = placed.iter().min_by(|a, b| a.1.total_cmp(&b.1));
        let high = placed.iter().max_by(|a, b| a.1.total_cmp(&b.1));
        let ends = (
            low.and_then(|(p, _)| p.rank.clone()),
            high.and_then(|(p, _)| p.rank.clone()),
        );
        return match ends {
            (Some(low), Some(high)) if low != high => format!("{low} to {high}{tail}"),
            (Some(only), _) => format!("all {only}{tail}"),
            _ => tail.trim_start_matches(" \u{b7} ").to_owned(),
        };
    };
    let over = placed
        .iter()
        .filter(|(p, rung)| p.team.as_deref() != Some(ours) && rung > mine)
        .count();
    match over {
        0 => format!("none of them above you{tail}"),
        1 => format!("one of them above you{tail}"),
        n => format!("{n} of them above you{tail}"),
    }
}

/// Anything the backend needs to say about the board itself.
///
/// It is the only place in the window where the app speaks rather than
/// reports, so it gets the accent and it gets to interrupt the layout. A
/// message the backend sent and nothing displayed is a message nobody will
/// ever see.
fn notice(ui: &mut Ui, board: &Board) {
    let Some(notice) = board.notice.as_ref() else {
        return;
    };
    let Some(message) = notice.message.as_deref().filter(|m| !m.is_empty()) else {
        return;
    };
    let tint = match notice.level.as_deref() {
        Some("error") => colour::ENEMY,
        Some("warn" | "warning") => colour::WARN,
        _ => colour::INFO,
    };
    overseer_ui::say(ui, GUTTER, tint, message, notice.action.as_deref());
}

/// A team's heading: a band in their colour, what to worry about, and how
/// they compare.
///
/// The only block of colour on the board, and the thing the eye lands on
/// first. It answers the question asked before any other: which half of this
/// lobby am I looking at, and is anybody on it a problem.
pub(crate) fn team_heading(
    ui: &mut Ui,
    label: &str,
    tint: Color32,
    board: &Board,
    team: &str,
) -> bool {
    let width = ui.available_width();
    let (rect, _response) =
        ui.allocate_exact_size(vec2(width, space::ROW + space::MD), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let painter = ui.painter().clone();
    let band = Rect::from_min_max(
        pos2(rect.left() + GUTTER, rect.top()),
        pos2(rect.right() - GUTTER, rect.bottom() - space::SM),
    );
    painter.add(shape::cut_filled(band, shape::CHAMFER, colour::BG_RAISED));
    painter.add(shape::cut_wash(
        band,
        shape::CHAMFER,
        tint.gamma_multiply(0.34),
        Color32::TRANSPARENT,
    ));
    // The bar is a wash too, bright at the top. A three point rectangle in
    // a flat colour is a rule; the same bar lit from above is an edge.
    painter.add(egui::Shape::gradient_rect(
        Rect::from_min_size(band.min, vec2(3.0, band.height())),
        egui::Direction::TopDown,
        [shape::blend(tint, colour::TEXT_STRONG, 0.35), tint],
    ));

    let mut x = band.left() + space::MD + space::SM;
    let drawn = caps_text(
        &painter,
        pos2(x, band.center().y),
        Align2::LEFT_CENTER,
        label,
        Face::Display.at(size::TITLE),
        tint,
    );
    x = drawn.right() + space::LG;

    let jump = worth_chip(ui, &painter, board, team, pos2(x, band.center().y));
    averages(&painter, board, team, band);
    jump
}

/// The count of accounts worth a look, as a chip that takes you to one.
///
/// The chip says there is somebody worth looking at, so it may as well be
/// the way to look at them: one click from the question to the answer, in
/// an app whose whole job is that question.
fn worth_chip(ui: &Ui, painter: &egui::Painter, board: &Board, team: &str, at: Pos2) -> bool {
    let flagged = board.team(team).iter().filter(|p| p.smurf).count();
    if flagged == 0 {
        return false;
    }
    let plate = chip(
        painter,
        at,
        &format!("{flagged} worth a look"),
        colour::WARN,
    );
    let hit = ui.interact(plate, ui.id().with(("worth", team)), Sense::click());
    if hit.hovered() {
        painter.add(shape::cut_wash(
            plate,
            4.0,
            colour::WARN.gamma_multiply(0.22),
            colour::WARN.gamma_multiply(0.10),
        ));
        drop(hit.clone().on_hover_cursor(egui::CursorIcon::PointingHand));
    }
    hit.clicked()
}

/// A side's averages, laid out right to left so they finish on the same
/// edge the numbers in the rows below finish on.
fn averages(painter: &egui::Painter, board: &Board, team: &str, band: Rect) {
    let Some(stats) = board.stats(team) else {
        return;
    };
    let mut right = band.right() - space::LG;
    for (name, value, tint) in [
        (
            "win",
            stats.avg_win_rate.map(|v| format!("{}%", v.round())),
            colour::NEUTRAL,
        ),
        (
            "k/d",
            stats.avg_kd.map(|v| format!("{v:.2}")),
            kd(stats.avg_kd),
        ),
        (
            "rank",
            stats.avg_rank.clone(),
            rank(stats.avg_rank_tier.map(|t| t.round() as u32)),
        ),
    ] {
        let Some(value) = value else { continue };
        let drawn = painter.text(
            pos2(right, band.center().y),
            Align2::RIGHT_CENTER,
            value,
            Face::Body.at(size::BODY),
            tint,
        );
        let drawn = caps_text(
            painter,
            pos2(drawn.left() - space::SM, band.center().y),
            Align2::RIGHT_CENTER,
            name,
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        right = drawn.left() - space::XL;
    }
}

/// A word on a tinted plate, cut at the corners. Returns where it ended.
///
/// For the handful of things that are claims rather than measurements: a
/// count of accounts worth a look, a tag somebody wrote. A claim on a plate
/// reads as a claim; the same words as plain text read as another column.
pub(crate) fn chip(painter: &egui::Painter, at: Pos2, text: &str, tint: Color32) -> Rect {
    let galley = painter.layout_job(caps(text, Face::Display.at(size::MICRO), tint));
    let plate = Rect::from_min_size(
        pos2(at.x, at.y - 9.0),
        vec2(galley.size().x + space::MD * 2.0, 18.0),
    );
    painter.add(shape::cut_wash(
        plate,
        4.0,
        tint.gamma_multiply(0.24),
        tint.gamma_multiply(0.10),
    ));
    painter.galley(
        pos2(
            plate.left() + space::MD,
            plate.center().y - galley.size().y / 2.0,
        ),
        galley,
        tint,
    );
    plate
}

/// What a missing value looks like, in one place.
fn dash() -> String {
    "-".to_owned()
}

/// A K/D to two places, or a dash.
fn number(value: Option<f64>) -> String {
    value.map_or_else(dash, |v| format!("{v:.2}"))
}

/// Which rows share a party, and where each group starts and ends.
///
/// Worked out over the team as it will be drawn, not as it arrived, because
/// a sorted board moves the rows and a bracket that spans two rows with a
/// stranger between them is a lie.
pub(crate) fn brackets(players: &[&Player]) -> Vec<Option<Bracket>> {
    let key = |p: &Player| p.party.as_ref().and_then(|party| party.number);
    players
        .iter()
        .enumerate()
        .map(|(i, player)| {
            let number = key(player)?;
            let tint =
                hex(player.party.as_ref().and_then(|p| p.color.as_deref())).unwrap_or(colour::INFO);
            let same = |at: usize| players.get(at).is_some_and(|p| key(p) == Some(number));
            // A party of one is not a party. Riot reports a number for a
            // solo queue player too, and a bracket around one row is an
            // ornament rather than a fact.
            let above = i > 0 && same(i - 1);
            let below = same(i + 1);
            (above || below).then_some(Bracket {
                tint,
                top: !above,
                bottom: !below,
            })
        })
        .collect()
}

/// The teams, in reading order.
///
/// Enemies first by default, which is a deliberate break from the game's own
/// Tab screen. The game already shows you your side at the top; the reason
/// to open this app at all is the other five, and putting them second means
/// the answer you came for is below the answer you already had. Anybody who
/// disagrees has a switch.
pub(crate) fn teams(board: &Board, enemies_first: bool) -> [(&'static str, Color32, String); 2] {
    let ours = board.self_team.as_deref().unwrap_or("Blue").to_owned();
    let theirs = if ours == "Blue" { "Red" } else { "Blue" }.to_owned();
    let allies = ("allies", colour::ALLY, ours);
    let enemies = ("enemies", colour::ENEMY, theirs);
    if enemies_first {
        [enemies, allies]
    } else {
        [allies, enemies]
    }
}

#[cfg(test)]
mod tests {
    use super::{COLUMNS, Priority, columns_for, number, width_of};

    /// The act has to lose its spaces without losing its meaning, and an
    /// act label this build has never seen must not come out mangled.
    #[test]
    fn an_act_is_shortened_rather_than_cut() {
        assert_eq!(super::short_act("V25 Act 3"), "V25A3");
        assert_eq!(super::short_act("E7 Act 2"), "E7A2");
        assert_eq!(super::short_act("Something Else"), "SomethingElse");
        assert_eq!(super::short_act(""), "");
    }

    #[test]
    fn missing_numbers_read_as_missing() {
        assert_eq!(number(None), "-");
        assert_eq!(number(Some(0.914)), "0.91");
    }

    /// The table sheds from the right and from the bottom of the priority
    /// list, and it never sheds the three columns that are the point.
    #[test]
    fn a_narrow_window_keeps_what_matters() {
        let wide = columns_for(1600.0, &[]);
        assert_eq!(wide.len(), COLUMNS.len(), "everything fits at 1600");

        let narrow = columns_for(360.0, &[]);
        assert!(narrow.len() < wide.len(), "nothing was dropped at 360");
        for head in ["player", "rank", "k/d"] {
            assert!(
                narrow.iter().any(|c| c.head == head),
                "{head} was dropped, and it is core"
            );
        }
        assert!(
            narrow.iter().all(|c| c.priority != Priority::Low),
            "a low priority column outlived a high priority one"
        );
    }

    /// Shedding has to stop rather than empty the table, however narrow it
    /// gets, because the alternative is a board with no columns in it.
    #[test]
    fn shedding_bottoms_out() {
        assert_eq!(columns_for(10.0, &[]).len(), 3);
    }

    /// The set that fits must actually fit, or every row wraps and the whole
    /// table becomes two lines per player.
    #[test]
    fn the_columns_that_fit_really_fit() {
        for width in [320.0_f32, 480.0, 640.0, 900.0, 1400.0, 1920.0] {
            let keep = columns_for(width, &[]);
            let needed = width_of(&keep);
            assert!(
                needed <= width || keep.len() == 3,
                "at {width} the columns need {needed}"
            );
        }
    }

    /// Switching a column off gives its room to the others rather than
    /// leaving a hole, which is the whole reason the setting exists.
    #[test]
    fn a_hidden_column_frees_its_own_width() {
        let all = columns_for(700.0, &[]);
        let without = columns_for(700.0, &["agent".to_owned()]);
        assert!(!without.iter().any(|c| c.head == "agent"));
        assert!(
            without.len() >= all.len(),
            "hiding agent left {:?}, which is fewer than {:?}",
            without.iter().map(|c| c.head).collect::<Vec<_>>(),
            all.iter().map(|c| c.head).collect::<Vec<_>>(),
        );
    }

    /// Every column has to be wide enough for the heading over it, and has to
    /// be able to say what it is for on the settings screen.
    #[test]
    fn a_column_can_hold_its_own_heading() {
        for column in &COLUMNS {
            assert!(column.width >= 40.0, "{} is too narrow", column.head);
            assert!(
                !column.about.is_empty(),
                "{} has nothing to say",
                column.head
            );
        }
    }
}
