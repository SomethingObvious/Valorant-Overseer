//! What sits over each side: a plate in the side's colour, red for the
//! enemy and teal for your team, cut the same way, and the column heads.

use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, pos2, vec2};
use overseer_core::Board;
use overseer_ui::{Face, caps_text, colour, space};

use super::Side;
use super::grid::Grid;
use super::paint;
use crate::sort::{Direction, Sort};

/// How tall a side's plate is.
pub(crate) const PLATE: f32 = 36.0;

/// The plate over one side, with its averages. True when its worth-a-look
/// chip was clicked.
pub(crate) fn team(ui: &mut Ui, board: &Board, side: Side, team: &str) -> bool {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), PLATE), Sense::hover());
    ui.add_space(space::SM);
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let painter = ui.painter().clone();
    let (fill, title) = match side {
        Side::Enemy if team == super::EVERYONE => (colour::ENEMY, "The Lobby"),
        Side::Enemy => (colour::ENEMY, "Enemy Team"),
        Side::Ally => (colour::ALLY, "Your Team"),
    };
    plate(&painter, rect, fill);
    let after = caps_text(
        &painter,
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        Face::Cut.at(17.0),
        colour::BG,
    );
    let mut taken = after.right();
    let mut clicked = false;
    let flagged = board
        .players
        .iter()
        .filter(|p| side == Side::Enemy && super::on(p, team) && p.smurf)
        .count();
    if flagged > 0 {
        let chip = worth_chip(
            &painter,
            pos2(after.right() + space::LG, rect.center().y),
            flagged,
        );
        taken = chip.right();
        let hit = ui.interact(chip, ui.id().with(("worth", team)), Sense::click());
        hit.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("{flagged} worth a look, show the next"),
            )
        });
        if hit.hovered() {
            tilted_chip(&painter, chip, colour::WARN.gamma_multiply(0.18), -2.0);
        }
        clicked = hit
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked();
    }
    // The shard's right edge leans in by its foot.
    let right = rect.width().mul_add(-0.006, rect.right()) - space::LG;
    taken = economy(
        ui,
        &painter,
        board,
        team,
        (taken + space::LG, right, rect.center().y),
    );
    averages(
        &painter,
        board,
        team,
        (taken + space::LG, right),
        rect.center().y,
    );
    clicked
}

/// A side's plate: a shard in its colour whose edges are off square by a
/// degree or so, and a thin rule under it that drops a little faster than
/// its foot, the website's cut.
fn plate(painter: &egui::Painter, rect: Rect, fill: Color32) {
    let at = |x: f32, y: f32| {
        pos2(
            rect.width().mul_add(x, rect.left()),
            rect.height().mul_add(y, rect.top()),
        )
    };
    painter.add(egui::Shape::convex_polygon(
        vec![at(0.0, 0.10), at(1.0, 0.0), at(0.994, 1.0), at(0.004, 0.92)],
        fill,
        egui::Stroke::NONE,
    ));
    let foot = at(0.0, 0.92).y;
    // The plate's foot drops to the right, so a rule that rose would run
    // into it on a wide board. This one drops with it, a touch faster.
    let (left, right) = (foot + 3.0, rect.width().mul_add(0.001, rect.bottom() + 3.0));
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(rect.left(), left),
            pos2(rect.right(), right),
            pos2(rect.right(), right + 2.5),
            pos2(rect.left(), left + 2.5),
        ],
        fill,
        egui::Stroke::NONE,
    ));
}

/// A chip slanted at both ends and turned by `degrees` about its centre, so
/// a row of them on the plate reads as cut out and laid down by hand.
fn tilted_chip(painter: &egui::Painter, chip: Rect, fill: Color32, degrees: f32) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let centre = chip.center();
    let points = paint::slanted(chip, true, true)
        .into_iter()
        .map(|p| {
            let off = p - centre;
            pos2(
                off.x.mul_add(cos, -off.y * sin) + centre.x,
                off.x.mul_add(sin, off.y * cos) + centre.y,
            )
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(
        points,
        fill,
        egui::Stroke::NONE,
    ));
}

/// A black chip on the red plate, in the flag's amber.
fn worth_chip(painter: &egui::Painter, at: Pos2, flagged: usize) -> Rect {
    let text = format!("{flagged} Worth a Look");
    let galley = painter.layout_job(overseer_ui::caps(&text, Face::Cut.at(10.5), colour::WARN));
    let chip = Rect::from_min_size(
        pos2(at.x, at.y - 10.0),
        vec2(space::MD.mul_add(2.0, galley.size().x), 20.0),
    );
    tilted_chip(painter, chip, colour::BG, -2.0);
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
) -> f32 {
    let mut end = x;
    let font = Face::Cut.at(10.5);
    for ((tag, word), turn) in [("forces", "Force"), ("saves", "Save")]
        .into_iter()
        .zip([1.5, -1.0])
    {
        let who: Vec<&str> = board
            .players
            .iter()
            .filter(|p| super::on(p, team))
            .filter(|p| p.auto_tags.iter().any(|t| t.tag.as_deref() == Some(tag)))
            .map(|p| super::shown_name(p).0)
            .collect();
        let said = format!("{} {word}", who.len());
        let wide = space::MD.mul_add(2.0, overseer_ui::caps_width(painter, &said, font.clone()));
        if who.is_empty() || end + wide > limit {
            continue;
        }
        // A black chip like the worth-a-look count's.
        let chip = Rect::from_min_size(pos2(end, y - 10.0), vec2(wide, 20.0));
        tilted_chip(painter, chip, colour::BG, turn);
        let _said = caps_text(
            painter,
            chip.center(),
            Align2::CENTER_CENTER,
            &said,
            font.clone(),
            colour::TEXT_STRONG,
        );
        let id = ui.id().with(("economy", team, tag));
        if ui.interact(chip, id, Sense::hover()).hovered() {
            super::tip::say_under(
                ui.ctx(),
                id.with("card"),
                chip,
                &format!(
                    "{} after a lost pistol round: {}",
                    if tag == "forces" { "Buys" } else { "Saves" },
                    who.join(", ")
                ),
            );
        }
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
) {
    let Some(stats) = board.stats(team) else {
        return;
    };
    let value_font = Face::Heavy.at(17.0);
    let mut x = right;
    for (name, value) in [
        ("Win", stats.win_rate.map(|v| format!("{v:.0}%"))),
        ("K/D", stats.kd.map(|v| format!("{v:.2}"))),
        ("Avg", stats.rank.clone()),
    ] {
        let Some(value) = value else { continue };
        let needs = overseer_ui::caps_width(painter, &value, value_font.clone())
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
            value_font.clone(),
            colour::BG,
        );
        let drawn = caps_text(
            painter,
            pos2(drawn.left() - space::SM, y + 1.0),
            Align2::RIGHT_CENTER,
            name,
            paint::label(),
            colour::BG,
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
        hit.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Sort by {}", column.label),
            )
        });
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
        if hit.hovered() {
            super::tip::say_under(ui.ctx(), hit.id.with("card"), hit.rect, column.about);
        }
        if hit
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            clicked = Some(column.head);
        }
    }
    missing(ui, &painter, rect, &grid.dropped);
    clicked
}

/// How many columns are on but don't fit, at the end of the heads, with
/// their names on its hover.
fn missing(ui: &Ui, painter: &egui::Painter, rect: Rect, dropped: &[&str]) {
    let Some((last, others)) = dropped.split_last() else {
        return;
    };
    let drawn = caps_text(
        painter,
        pos2(rect.right() - space::SM, rect.center().y),
        Align2::RIGHT_CENTER,
        &format!("+{}", dropped.len()),
        paint::label(),
        colour::TEXT_FAINT,
    );
    let hit = ui.interact(
        drawn.expand(space::SM),
        ui.id().with("missing"),
        Sense::hover(),
    );
    let named = if others.is_empty() {
        format!("{last} doesn't fit")
    } else {
        format!("{} and {last} don't fit", others.join(", "))
    };
    let said =
        format!("{named} at this width. Widen the window or close the detail panel to see them.");
    hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &said));
    if hit.hovered() {
        super::tip::say_under(ui.ctx(), hit.id.with("card"), hit.rect, &said);
    }
}
