//! What sits over each side: the enemy's solid plate, your team's quiet
//! rule, and the column heads. The enemy plate is the one solid colour on the
//! board, so the eye lands there first. Your own side only gets a rule and
//! its name, since you already know who is on your team.

use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, pos2, vec2};
use overseer_core::Board;
use overseer_ui::{Face, caps_text, colour, space};

use super::Side;
use super::grid::Grid;
use super::paint;
use crate::sort::{Direction, Sort};

/// How tall the enemy's plate is.
pub(crate) const PLATE: f32 = 36.0;

/// What was clicked in a heading.
#[derive(Debug, Default)]
pub(crate) struct Clicked {
    /// The worth-a-look chip, which jumps to the next flagged account.
    pub(crate) worth: bool,
}

/// The heading over one side, with its averages.
pub(crate) fn team(ui: &mut Ui, board: &Board, side: Side, team: &str) -> Clicked {
    match side {
        Side::Enemy => enemy(ui, board, team),
        Side::Ally => ally(ui, board, team),
    }
}

/// The enemy's plate: solid red, black type, the tail cut to the slant.
fn enemy(ui: &mut Ui, board: &Board, team: &str) -> Clicked {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), PLATE), Sense::hover());
    ui.add_space(space::SM);
    let mut clicked = Clicked::default();
    if !ui.is_rect_visible(rect) {
        return clicked;
    }
    let painter = ui.painter().clone();
    painter.add(paint::slant(rect, false, true, colour::ENEMY));
    let after = caps_text(
        &painter,
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        if team == super::EVERYONE {
            "The Lobby"
        } else {
            "Enemy Team"
        },
        Face::Heavy.at(22.0),
        colour::BG,
    );
    let mut taken = after.right();
    let flagged = board
        .players
        .iter()
        .filter(|p| super::on(p, team) && p.smurf)
        .count();
    if flagged > 0 {
        let chip = worth_chip(
            &painter,
            pos2(after.right() + space::LG, rect.center().y),
            flagged,
        );
        taken = chip.right();
        let hit = ui.interact(chip, ui.id().with(("worth", team)), Sense::click());
        if hit.hovered() {
            painter.add(paint::slant(
                chip,
                true,
                true,
                colour::WARN.gamma_multiply(0.18),
            ));
        }
        clicked.worth = hit
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked();
    }
    let right = rect.height().mul_add(-paint::LEAN, rect.right()) - space::LG;
    taken = economy(
        ui,
        &painter,
        board,
        team,
        (taken + space::LG, right, rect.center().y),
        colour::BG,
    );
    averages(
        &painter,
        board,
        team,
        (taken + space::LG, right),
        rect.center().y,
        colour::BG,
    );
    clicked
}

/// A black chip on the red plate, in the flag's amber.
fn worth_chip(painter: &egui::Painter, at: Pos2, flagged: usize) -> Rect {
    let text = format!("{flagged} Worth a Look");
    let galley = painter.layout_job(overseer_ui::caps(&text, paint::label(), colour::WARN));
    let chip = Rect::from_min_size(
        pos2(at.x, at.y - 10.0),
        vec2(space::LG.mul_add(2.0, galley.size().x), 20.0),
    );
    painter.add(paint::slant(chip, true, true, colour::BG));
    painter.galley(
        pos2(
            chip.center().x - galley.size().x / 2.0,
            at.y - galley.size().y / 2.0,
        ),
        galley,
        colour::WARN,
    );
    chip
}

/// Your side: a rule in your colour and your team's name, nothing solid.
fn ally(ui: &mut Ui, board: &Board, team: &str) -> Clicked {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    ui.add_space(space::SM);
    if !ui.is_rect_visible(rect) {
        return Clicked::default();
    }
    let painter = ui.painter().clone();
    painter.rect_filled(
        Rect::from_min_size(
            pos2(rect.left(), rect.top() + 4.0),
            vec2(3.0, rect.height() - 8.0),
        ),
        0,
        colour::ALLY,
    );
    let named = caps_text(
        &painter,
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        "Your Team",
        Face::Heavy.at(17.0),
        colour::ALLY,
    );
    let taken = economy(
        ui,
        &painter,
        board,
        team,
        (named.right() + space::XL, rect.right(), rect.center().y),
        colour::TEXT_DIM,
    );
    averages(
        &painter,
        board,
        team,
        (taken.max(rect.left() + 140.0), rect.right()),
        rect.center().y,
        colour::TEXT_DIM,
    );
    Clicked::default()
}

/// How many on a side buy after a lost pistol round and how many save, from
/// the tags, with who on hover. Round two is where it pays to know. Each
/// stops short of `limit`. Returns where it ended, which is `x` when nobody
/// has either habit.
fn economy(
    ui: &Ui,
    painter: &egui::Painter,
    board: &Board,
    team: &str,
    (x, limit, y): (f32, f32, f32),
    ink: Color32,
) -> f32 {
    let mut end = x;
    for (tag, word) in [("forces", "Force"), ("saves", "Save")] {
        let who: Vec<&str> = board
            .players
            .iter()
            .filter(|p| super::on(p, team))
            .filter(|p| p.auto_tags.iter().any(|t| t.tag.as_deref() == Some(tag)))
            .map(|p| super::shown_name(p).0)
            .collect();
        let said = format!("{} {word}", who.len());
        let wide = space::LG.mul_add(2.0, overseer_ui::caps_width(painter, &said, paint::label()));
        if who.is_empty() || end + wide > limit {
            continue;
        }
        // A chip like the worth-a-look count's: black on the red plate, and
        // a raised grey on your own side's dark header.
        let chip = Rect::from_min_size(pos2(end, y - 10.0), vec2(wide, 20.0));
        let fill = if ink == colour::BG {
            colour::BG
        } else {
            colour::BG_INSET
        };
        painter.add(paint::slant(chip, true, true, fill));
        let _said = caps_text(
            painter,
            chip.center(),
            Align2::CENTER_CENTER,
            &said,
            paint::label(),
            colour::TEXT_STRONG,
        );
        let _who = ui
            .interact(chip, ui.id().with(("economy", team, tag)), Sense::hover())
            .on_hover_text(format!(
                "{} after a lost pistol round: {}",
                if tag == "forces" { "Buys" } else { "Saves" },
                who.join(", ")
            ));
        end = chip.right() + space::SM;
    }
    end + space::SM
}

/// The side's averages, right to left: the win rate, the K/D and the
/// average rank, each measured first and dropped rather than drawn over
/// whatever is to its left.
fn averages(
    painter: &egui::Painter,
    board: &Board,
    team: &str,
    (limit, right): (f32, f32),
    y: f32,
    ink: Color32,
) {
    let Some(stats) = board.stats(team) else {
        return;
    };
    let on_plate = ink == colour::BG;
    let mut x = right;
    for (name, value, tint) in [
        ("Win", stats.win_rate.map(|v| format!("{v:.0}%")), ink),
        ("K/D", stats.kd.map(|v| format!("{v:.2}")), ink),
        (
            "Avg",
            stats.rank.clone(),
            if on_plate { ink } else { colour::TEXT },
        ),
    ] {
        let Some(value) = value else { continue };
        let needs = overseer_ui::caps_width(painter, &value, Face::Heavy.at(17.0))
            + overseer_ui::caps_width(painter, name, paint::label())
            + space::SM;
        if x - needs < limit {
            break;
        }
        let drawn = caps_text(
            painter,
            pos2(x, y),
            Align2::RIGHT_CENTER,
            &value,
            Face::Heavy.at(17.0),
            tint,
        );
        let drawn = caps_text(
            painter,
            pos2(drawn.left() - space::SM, y + 1.0),
            Align2::RIGHT_CENTER,
            name,
            paint::label(),
            ink.gamma_multiply(0.75),
        );
        x = drawn.left() - space::XL;
    }
}

/// The column heads over a side, drawn from the same grid as its rows, and
/// the one that was clicked. Clicking a head sorts by it.
pub(crate) fn columns(ui: &mut Ui, grid: &Grid, sort: &Sort) -> Option<&'static str> {
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::XL + space::SM),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let painter = ui.painter().clone();
    let mut clicked = None;
    for placed in &grid.placed {
        let column = placed.column;
        let area = Rect::from_min_max(
            pos2(rect.left() + placed.left, rect.top()),
            pos2(rect.left() + placed.right, rect.bottom()),
        );
        let hit = ui.interact(area, ui.id().with(("head", column.head)), Sense::click());
        let arrow = sort.arrow(column.head);
        let tint = if arrow.is_some() || hit.hovered() {
            colour::TEXT_STRONG
        } else {
            colour::TEXT_FAINT
        };
        let (anchor, x) = if column.numeric {
            (Align2::RIGHT_CENTER, area.right())
        } else {
            (Align2::LEFT_CENTER, area.left())
        };
        let drawn = caps_text(
            &painter,
            pos2(x, area.center().y),
            anchor,
            column.label,
            paint::label(),
            tint,
        );
        if let Some(direction) = arrow {
            let tip = if column.numeric {
                drawn.left() - 7.0
            } else {
                drawn.right() + 7.0
            };
            let y = area.center().y;
            let (a, b) = match direction {
                Direction::Down => (y - 2.0, y + 2.5),
                Direction::Up => (y + 2.0, y - 2.5),
            };
            painter.add(egui::Shape::convex_polygon(
                vec![pos2(tip - 3.5, a), pos2(tip + 3.5, a), pos2(tip, b)],
                colour::TEXT_STRONG,
                egui::Stroke::NONE,
            ));
        }
        if hit
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(column.about)
            .clicked()
        {
            clicked = Some(column.head);
        }
    }
    clicked
}
