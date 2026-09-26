//! The board: two teams, one row per player.
//!
//! A column is a spec rather than a guess. Width, alignment and heading live
//! in one table, and the heading row and the data rows both read from it, so
//! a heading cannot drift off its own numbers. That is the single thing that
//! separates a table somebody built from a table somebody assembled.

use egui::text::{LayoutJob, TextWrapping};
use egui::{Align2, Color32, FontId, Rect, Response, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Player};

use crate::design::{Face, colour, kd, label_text, rank, size, space};

/// Which way a column's content sits against its own width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    /// Words: agents, names, ranks.
    Left,
    /// Numbers. Right, always, so the digits line up under each other.
    Right,
}

/// How much a column matters when the window is too narrow for all of them.
/// The same order the terminal front end sheds in, so the two apps lose the
/// same things at the same point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    /// Never dropped. Without these there is no table.
    Core,
    /// Dropped last.
    High,
    /// Dropped first.
    Low,
}

/// One column of the board.
struct Column {
    /// What the heading says, before it is put into caps and tracked.
    head: &'static str,
    /// Drawn width in points, not counting the gutter after it.
    width: f32,
    /// Which way the content sits.
    align: Align,
    /// Which face draws it. Numbers get the mono face so they cannot drift.
    face: Face,
    /// When it is dropped.
    priority: Priority,
}

/// The board's columns, in reading order: who they are, then how they play.
const COLUMNS: [Column; 6] = [
    Column {
        head: "agent",
        width: 68.0,
        align: Align::Left,
        face: Face::Body,
        priority: Priority::High,
    },
    Column {
        head: "player",
        width: 152.0,
        align: Align::Left,
        face: Face::Body,
        priority: Priority::Core,
    },
    Column {
        head: "rank",
        width: 96.0,
        align: Align::Left,
        face: Face::Body,
        priority: Priority::Core,
    },
    Column {
        head: "k/d",
        width: 48.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::Core,
    },
    Column {
        head: "win",
        width: 48.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::High,
    },
    Column {
        head: "lvl",
        width: 44.0,
        align: Align::Right,
        face: Face::Number,
        priority: Priority::Low,
    },
];

/// The mark in the flag column, and how loud it is.
const FLAG_WIDTH: f32 = 18.0;

/// What one row needs beyond the player themselves.
pub(crate) struct RowStyle {
    /// The team's colour, which the name is drawn in.
    pub(crate) team: Color32,
    /// Whether this row is the selected one.
    pub(crate) selected: bool,
    /// How tall the row is, which the window's width decides.
    pub(crate) height: f32,
}

/// What a set of columns needs, gutters and margins included.
fn width_of(keep: &[&Column]) -> f32 {
    let columns: f32 = keep.iter().map(|c| c.width + space::MD).sum();
    space::LG.mul_add(2.0, columns + FLAG_WIDTH)
}

/// Which columns fit in the width available, widest set first.
fn columns_for(width: f32) -> Vec<&'static Column> {
    let mut keep: Vec<&Column> = COLUMNS.iter().collect();
    loop {
        let needed = width_of(&keep);
        if needed <= width || keep.len() <= 3 {
            return keep;
        }
        // Drop the lowest priority, rightmost first, which is why this walks
        // backwards rather than taking the first match.
        let Some(at) = keep
            .iter()
            .enumerate()
            .filter(|(_, c)| c.priority != Priority::Core)
            .map(|(i, c)| (c.priority, i))
            .max()
            .map(|(_, i)| i)
        else {
            return keep;
        };
        keep.remove(at);
    }
}

/// The heading row, drawn from the same spec the data uses.
pub(crate) fn headings(ui: &mut Ui, width: f32) {
    let (rect, _response) = ui.allocate_exact_size(vec2(width, space::ROW_TIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let mut x = rect.left() + space::LG;
    for column in columns_for(width) {
        let (pos, anchor) = match column.align {
            Align::Left => (pos2(x, rect.center().y), Align2::LEFT_CENTER),
            Align::Right => (
                pos2(x + column.width, rect.center().y),
                Align2::RIGHT_CENTER,
            ),
        };
        painter.text(
            pos,
            anchor,
            label_text(column.head),
            Face::Display.at(size::LABEL),
            colour::TEXT_FAINT,
        );
        x += column.width + space::MD;
    }
    painter.hline(rect.x_range(), rect.bottom() - 1.0, (1.0, colour::LINE));
}

/// Draws one cell, ending in an ellipsis rather than mid glyph.
///
/// A severed letter reads as a bug; three dots read as "there is more". Both
/// cost one layout, so there is no reason to take the one that looks broken.
fn cell_text(ui: &Ui, text: &str, font: FontId, tint: Color32, rect: Rect, align: Align) {
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

/// One player. Painted rather than assembled out of widgets: a row is a
/// handful of strings at known offsets, and going through egui's layout for
/// each of them costs more than the row is worth when ten redraw together.
pub(crate) fn row(ui: &mut Ui, player: &Player, style: &RowStyle) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, style.height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter().clone();

    if style.selected {
        painter.rect_filled(rect, 0, colour::BG_SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0, colour::BG_HOVER);
    }

    // The state bar: you are light, an enemy is red, an ally is green. That is
    // the game's own convention, and a companion app that inverts it is worse
    // than one with no colour at all.
    let mut bar = rect;
    bar.set_width(2.0);
    let bar_colour = if player.is_self {
        colour::YOU
    } else {
        style.team
    };
    painter.rect_filled(
        bar,
        0,
        bar_colour.gamma_multiply(if player.is_self { 1.0 } else { 0.5 }),
    );

    let name_colour = if player.is_self {
        colour::YOU
    } else {
        style.team
    };
    if player.smurf {
        painter.text(
            pos2(rect.right() - space::LG, rect.center().y),
            Align2::RIGHT_CENTER,
            "!",
            Face::Display.at(size::BODY),
            colour::WARN,
        );
    }
    painter.hline(rect.x_range(), rect.bottom(), (1.0, colour::LINE_SOFT));

    let mut x = rect.left() + space::LG;
    for column in columns_for(width) {
        let (text, tint) = cell(column.head, player, name_colour);
        let cell_rect = Rect::from_min_size(pos2(x, rect.top()), vec2(column.width, rect.height()));
        cell_text(
            ui,
            &text,
            column.face.at(size::BODY),
            tint,
            cell_rect,
            column.align,
        );
        x += column.width + space::MD;
    }
    response
}

/// What one column says about one player, and in what colour.
fn cell(head: &str, player: &Player, name_colour: Color32) -> (String, Color32) {
    match head {
        "agent" => (
            player.agent.clone().unwrap_or_else(dash),
            player
                .agent
                .as_ref()
                .map_or(colour::TEXT_FAINT, |_| colour::INFO),
        ),
        "player" => (player.name.clone().unwrap_or_else(dash), name_colour),
        "rank" => (
            player.rank.clone().unwrap_or_else(dash),
            rank(player.rank_tier),
        ),
        "k/d" => (number(player.kd), kd(player.kd)),
        "win" => (percent(player.win_rate), colour::TEXT),
        "lvl" => (
            player.level.map_or_else(dash, |l| l.to_string()),
            colour::TEXT_FAINT,
        ),
        _ => (dash(), colour::TEXT_FAINT),
    }
}

/// A team's heading: who they are, and what is worth knowing about them.
pub(crate) fn team_heading(ui: &mut Ui, label: &str, tint: Color32, players: &[&Player]) {
    let width = ui.available_width();
    let (rect, _response) = ui.allocate_exact_size(vec2(width, space::ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let mut x = rect.left() + space::LG;

    // A four point bar in the team's colour, then the name. The bar is the
    // only ornament on the screen and it carries information.
    let bar = Rect::from_min_size(
        pos2(x, rect.top() + space::MD),
        vec2(3.0, rect.height() - space::XL),
    );
    painter.rect_filled(bar, 0, tint);
    x += space::MD;

    let after = painter.text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        label_text(label),
        Face::Display.at(size::LABEL),
        tint,
    );
    x = after.right() + space::LG;

    let flagged = players.iter().filter(|p| p.smurf).count();
    if flagged > 0 {
        let text = if flagged == 1 {
            "1 worth a look".to_owned()
        } else {
            format!("{flagged} worth a look")
        };
        painter.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            Face::Body.at(size::MICRO),
            colour::WARN,
        );
    }
}

/// What a missing value looks like, in one place.
fn dash() -> String {
    "-".to_owned()
}

/// A K/D to two places, or a dash.
fn number(value: Option<f64>) -> String {
    value.map_or_else(dash, |v| format!("{v:.2}"))
}

/// A percentage with no decimal, or a dash.
fn percent(value: Option<f64>) -> String {
    value.map_or_else(dash, |v| format!("{}%", v.round()))
}

/// The teams, in reading order: ours, then theirs.
pub(crate) fn teams(board: &Board) -> [(&'static str, Color32, Vec<&Player>); 2] {
    let ours = board.self_team.as_deref().unwrap_or("Blue");
    let theirs = if ours == "Blue" { "Red" } else { "Blue" };
    [
        ("allies", colour::ALLY, board.team(ours)),
        ("enemies", colour::ENEMY, board.team(theirs)),
    ]
}

#[cfg(test)]
mod tests {
    use super::{COLUMNS, Priority, columns_for, number, percent, width_of};

    #[test]
    fn missing_numbers_read_as_missing() {
        assert_eq!(number(None), "-");
        assert_eq!(number(Some(0.914)), "0.91");
        assert_eq!(percent(None), "-");
        assert_eq!(percent(Some(52.4)), "52%");
    }

    /// The table sheds from the right and from the bottom of the priority
    /// list, and it never sheds the three columns that are the point.
    #[test]
    fn a_narrow_window_keeps_what_matters() {
        let wide = columns_for(1200.0);
        assert_eq!(wide.len(), COLUMNS.len(), "everything fits at 1200");

        let narrow = columns_for(360.0);
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
        let tiny = columns_for(10.0);
        assert_eq!(tiny.len(), 3);
    }

    /// The set that fits must actually fit, or every row wraps and the whole
    /// table becomes two lines per player.
    #[test]
    fn the_columns_that_fit_really_fit() {
        for width in [320.0_f32, 480.0, 640.0, 900.0, 1400.0] {
            let keep = columns_for(width);
            let needed = width_of(&keep);
            assert!(
                needed <= width || keep.len() == 3,
                "at {width} the kept columns need {needed}"
            );
        }
    }

    /// Every column has to be wide enough for the heading over it, or the
    /// heading is the thing that gets an ellipsis.
    #[test]
    fn a_column_can_hold_its_own_heading() {
        for column in &COLUMNS {
            assert!(
                column.width >= 40.0,
                "{} is too narrow for a value and a heading",
                column.head
            );
        }
    }
}
