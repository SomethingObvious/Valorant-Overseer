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
use egui::{Align2, Color32, FontId, Rect, Response, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Player};

use crate::sort::{Direction, Sort};
use overseer_ui::{Face, caps, caps_text, colour, hex, kd, motion, rank, shape, size, space};

/// The margin down both sides of the board.
///
/// Wide enough on the left for a party bracket to sit in it without
/// touching a row, which is the whole reason it is not the default gap: a
/// bracket drawn inside the rows would be a twelfth column, and drawn
/// against the window edge it would look like a rendering fault.
const GUTTER: f32 = 18.0;

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
        width: 68.0,
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
        width: 92.0,
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
        width: 120.0,
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
        about: "Headshot percentage over those same matches",
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

/// The room kept on the right for the flags: a smurf mark and a party mark.
const FLAG_WIDTH: f32 = 34.0;
/// One result pip in the form column.
const PIP: f32 = 8.0;

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
    space::LG.mul_add(2.0, columns + FLAG_WIDTH)
}

/// Which columns fit in the width available, after the ones switched off.
///
/// Built by adding rather than by shedding. Shedding overshoots: dropping a
/// 128 point column to save 40 leaves 88 points of room that nothing is ever
/// offered, and the test that noticed said the board kept fewer columns after
/// one was switched off, which is the opposite of what switching one off is
/// for. Adding in priority order cannot overshoot, and it is twelve
/// comparisons on a table of twelve.
pub(crate) fn columns_for(width: f32, hidden: &[String]) -> Vec<&'static Column> {
    let visible = |c: &&Column| !hidden.iter().any(|h| h == c.head);
    // The core columns are not negotiable: without them there is no table,
    // and a window too narrow for them is a window that gets a wide row.
    let mut keep: Vec<&Column> = COLUMNS
        .iter()
        .filter(|c| c.priority == Priority::Core)
        .filter(visible)
        .collect();
    for priority in [Priority::High, Priority::Mid, Priority::Low] {
        for column in COLUMNS
            .iter()
            .filter(|c| c.priority == priority)
            .filter(visible)
        {
            let mut candidate = keep.clone();
            candidate.push(column);
            // Kept in table order rather than in the order they were added,
            // so the board reads the same however many columns are on.
            candidate.sort_by_key(|c| COLUMNS.iter().position(|o| o.head == c.head).unwrap_or(0));
            if width_of(&candidate) <= width {
                keep = candidate;
            }
        }
    }
    keep
}

/// Draws one cell, ending in an ellipsis rather than mid glyph.
///
/// A severed letter reads as a bug; three dots read as "there is more". Both
/// cost one layout, so there is no reason to take the one that looks broken.
fn cell_text(ui: &Ui, text: &str, font: FontId, tint: Color32, rect: Rect, align: Align) {
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
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let y = rect.center().y - galley.size().y / 2.0;
    let x = match align {
        Align::Left => rect.left(),
        Align::Right => rect.right() - galley.size().x,
    };
    ui.painter().galley(pos2(x, y), galley, tint);
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
            "rr" => rr_cell(ui, player, cell_rect),
            "last 5" => form_cell(ui, player, cell_rect),
            "rank" => rank_cell(&painter, player, cell_rect),
            _ => {
                let (text, tint) = cell(column.head, player, name_colour);
                cell_text(
                    ui,
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
    response
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
    if chosen > 0.0 {
        painter.rect_filled(inner, 0, colour::BG_SELECTED.gamma_multiply(chosen));
    }
    if hover > 0.0 && chosen < 1.0 {
        painter.rect_filled(
            inner,
            0,
            colour::BG_HOVER.gamma_multiply(hover * (1.0 - chosen)),
        );
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
    let tint = rank(player.rank_tier);
    let galley = painter.layout_no_wrap(name.to_owned(), Face::Body.at(size::BODY), tint);
    let plate = Rect::from_min_size(
        pos2(rect.left(), rect.center().y - 9.0),
        vec2((galley.size().x + space::MD * 2.0).min(rect.width()), 18.0),
    );
    // Unranked is a state rather than a tier, so it gets no plate: a grey
    // plate next to nine coloured ones reads as a tenth rank.
    if player.rank_tier.unwrap_or(0) > 0 {
        painter.add(shape::cut_filled(plate, 4.0, tint.gamma_multiply(0.15)));
    }
    painter.galley(
        pos2(
            plate.left() + space::MD,
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
        let drawn = painter.text(
            pos2(x, rect.center().y),
            Align2::RIGHT_CENTER,
            "!",
            Face::Display.at(size::BODY),
            colour::WARN,
        );
        x = drawn.left() - space::MD;
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
fn rr_cell(ui: &Ui, player: &Player, rect: Rect) {
    let Some(rr) = player.rr else {
        cell_text(
            ui,
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
        ui,
        &text,
        Face::Number.at(size::MICRO),
        tint,
        delta_rect,
        Align::Right,
    );
    let value_rect = Rect::from_min_size(rect.min, vec2(rect.width() - 28.0, rect.height()));
    cell_text(
        ui,
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
fn form_cell(ui: &Ui, player: &Player, rect: Rect) {
    let painter = ui.painter();
    let mut x = rect.left();
    for result in player.form.iter().take(5) {
        let tint = match result.chars().next() {
            Some('W' | 'w') => colour::ALLY,
            Some('D' | 'd') => colour::WARN,
            _ => colour::ENEMY,
        };
        let pip = Rect::from_min_size(pos2(x, rect.center().y - PIP / 2.0), vec2(PIP - 2.0, PIP));
        painter.rect_filled(pip, 0, tint.gamma_multiply(0.85));
        x += PIP + 2.0;
    }
    if let Some(streak) = player.streak.as_ref().filter(|s| s.count.unwrap_or(0) >= 3) {
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
    if gap >= 3 {
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

/// A team's heading: a band in their colour, what to worry about, and how
/// they compare.
///
/// The only block of colour on the board, and the thing the eye lands on
/// first. It answers the question asked before any other: which half of this
/// lobby am I looking at, and is anybody on it a problem.
pub(crate) fn team_heading(ui: &mut Ui, label: &str, tint: Color32, board: &Board, team: &str) {
    let width = ui.available_width();
    let (rect, _response) =
        ui.allocate_exact_size(vec2(width, space::ROW + space::MD), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let band = Rect::from_min_max(
        pos2(rect.left() + GUTTER, rect.top()),
        pos2(rect.right() - GUTTER, rect.bottom() - space::SM),
    );
    painter.add(shape::cut_filled(
        band,
        shape::CHAMFER,
        tint.gamma_multiply(0.10),
    ));
    painter.rect_filled(
        Rect::from_min_size(band.min, vec2(3.0, band.height())),
        0,
        tint,
    );

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

    let flagged = board.team(team).iter().filter(|p| p.smurf).count();
    if flagged > 0 {
        chip(
            &painter,
            pos2(x, band.center().y),
            &format!("{flagged} worth a look"),
            colour::WARN,
        );
    }

    // The averages, laid out right to left so they finish on the same edge
    // the numbers in the rows below finish on.
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
            &painter,
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
pub(crate) fn chip(painter: &egui::Painter, at: egui::Pos2, text: &str, tint: Color32) -> Rect {
    let galley = painter.layout_job(caps(text, Face::Display.at(size::MICRO), tint));
    let plate = Rect::from_min_size(
        pos2(at.x, at.y - 9.0),
        vec2(galley.size().x + space::MD * 2.0, 18.0),
    );
    painter.add(shape::cut_filled(plate, 4.0, tint.gamma_multiply(0.16)));
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
