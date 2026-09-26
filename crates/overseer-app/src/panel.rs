//! The detail panel: one player, in full.
//!
//! The board answers "who is here". This answers "what about them", and it is
//! the only place in the app where a paragraph is allowed. Its hierarchy is
//! deliberately steep: a name, then the one thing worth knowing, then the
//! numbers, then the small print. If a person reads only the first two lines
//! they should still have got the point.

use egui::{Align2, Color32, Rect, Sense, Ui, pos2, vec2};
use overseer_core::Player;

use crate::design::{Face, colour, kd, label_text, rank, size, space};

/// Draws the panel for a player, or the reason there is nobody to draw.
pub(crate) fn show(ui: &mut Ui, player: Option<&Player>) {
    let Some(player) = player else {
        heading(ui, "no one selected");
        paragraph(ui, "Click a row, or press up and down.");
        return;
    };

    name(ui, player);
    identity(ui, player);
    if !player.smurf_reasons.is_empty() {
        flags(ui, player);
    }
    ranks(ui, player);
    numbers(ui, player);
}

/// The player's name, at the top, in the brightest thing there is.
fn name(ui: &mut Ui, player: &Player) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XXL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    // The tag is part of the name and not part of the point, so it is drawn
    // quieter rather than dropped: a player with the same name as somebody
    // else is exactly when you need to see it.
    let full = player.name.clone().unwrap_or_else(|| "-".to_owned());
    let (stem, tag) = full
        .split_once('#')
        .map_or((full.as_str(), ""), |(a, b)| (a, b));
    let painter = ui.painter();
    let after = painter.text(
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        stem,
        Face::Body.at(size::TITLE),
        colour::TEXT_STRONG,
    );
    if !tag.is_empty() {
        painter.text(
            pos2(after.right() + space::SM, rect.center().y + 1.0),
            Align2::LEFT_CENTER,
            format!("#{tag}"),
            Face::Body.at(size::LABEL),
            colour::TEXT_FAINT,
        );
    }
}

/// Level, role and agent on one line, because they are one thought.
fn identity(ui: &mut Ui, player: &Player) {
    let mut parts: Vec<String> = Vec::new();
    if let Some(level) = player.level {
        parts.push(format!("Level {level}"));
    }
    if let Some(role) = player.role.as_deref() {
        parts.push(role.to_owned());
    }
    if let Some(agent) = player.agent.as_deref() {
        parts.push(agent.to_owned());
    }
    if parts.is_empty() {
        return;
    }
    line(ui, &parts.join("  ·  "), colour::TEXT_DIM, size::BODY);
}

/// Why this account is worth a second look, in the backend's words.
///
/// Gold, and only gold: this and the encounter count are the two things in the
/// app that are claims about a person rather than measurements of one, and
/// they share a colour so that colour means something.
fn flags(ui: &mut Ui, player: &Player) {
    ui.add_space(space::LG);
    let title = if player.smurf {
        "smurf"
    } else {
        "worth a look"
    };
    heading_tinted(ui, title, colour::WARN);
    for reason in &player.smurf_reasons {
        line(ui, reason, colour::WARN, size::BODY);
    }
}

/// Where they are now, and the best they have ever been.
fn ranks(ui: &mut Ui, player: &Player) {
    ui.add_space(space::LG);
    let width = ui.available_width();
    let (rect, _response) = ui.allocate_exact_size(vec2(width, space::ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let mut x = rect.left() + space::LG;
    let after = painter.text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        player.rank.clone().unwrap_or_else(|| "Unranked".to_owned()),
        Face::Body.at(size::BODY),
        rank(player.rank_tier),
    );
    x = after.right() + space::MD;

    if let Some(rr) = player.rr {
        let after = painter.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            format!("{rr}"),
            Face::Number.at(size::BODY),
            colour::TEXT,
        );
        let after = painter.text(
            pos2(after.right() + space::SM, rect.center().y),
            Align2::LEFT_CENTER,
            "RR",
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        x = after.right() + space::MD;
        // How far through the rank they are, as a bar rather than a second
        // number: the number is already there, and the bar is the thing you
        // read without reading.
        let track = Rect::from_min_size(
            pos2(x, rect.center().y - 2.0),
            vec2((rect.right() - space::LG - x).clamp(0.0, 80.0), 4.0),
        );
        painter.rect_filled(track, 0, colour::LINE);
        let share = (rr as f32 / 100.0).clamp(0.0, 1.0);
        let mut filled = track;
        filled.set_width(track.width() * share);
        painter.rect_filled(filled, 0, colour::INFO);
    }

    if let Some(peak) = player.peak_rank.as_deref() {
        let act = player.peak_act.as_deref().unwrap_or("");
        let text = if act.is_empty() {
            format!("Peak {peak}")
        } else {
            format!("Peak {peak}  {act}")
        };
        line(ui, &text, colour::TEXT_DIM, size::BODY);
    }
}

/// The three numbers the board could not fit, with their sample sizes.
///
/// A number with no sample size behind it reads as a career average, and the
/// K/D here is the last few matches. Saying so costs four characters.
fn numbers(ui: &mut Ui, player: &Player) {
    ui.add_space(space::LG);
    heading(ui, "form");
    stat(
        ui,
        "k/d",
        &player
            .kd
            .map_or_else(|| "-".to_owned(), |v| format!("{v:.2}")),
        kd(player.kd),
    );
    let win = player
        .win_rate
        .map_or_else(|| "-".to_owned(), |v| format!("{}%", v.round()));
    let over = player
        .games
        .map_or_else(String::new, |g| format!("over {g}"));
    stat_with_note(ui, "win", &win, colour::TEXT, &over);
}

/// A section heading: caps, tracked, dim, with air above it.
fn heading(ui: &mut Ui, text: &str) {
    heading_tinted(ui, text, colour::TEXT_FAINT);
}

/// A section heading in a colour, for the two sections that are claims.
fn heading_tinted(ui: &mut Ui, text: &str, tint: Color32) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().text(
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        label_text(text),
        Face::Display.at(size::LABEL),
        tint,
    );
}

/// One line of prose, at the panel's left margin.
fn line(ui: &mut Ui, text: &str, tint: Color32, points: f32) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().text(
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        Face::Body.at(points),
        tint,
    );
}

/// A paragraph, which is the one thing the board may not have.
fn paragraph(ui: &mut Ui, text: &str) {
    line(ui, text, colour::TEXT_DIM, size::BODY);
}

/// A label and its value, on one line, value in the mono face.
fn stat(ui: &mut Ui, label: &str, value: &str, tint: Color32) {
    stat_with_note(ui, label, value, tint, "");
}

/// A label, a value, and a quieter note after it.
fn stat_with_note(ui: &mut Ui, label: &str, value: &str, tint: Color32, note: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.text(
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        label_text(label),
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    // The values start at one column, not wherever their label ended, so the
    // panel has a spine down the middle instead of a ragged edge.
    let value_x = rect.left() + space::LG + 52.0;
    let after = painter.text(
        pos2(value_x, rect.center().y),
        Align2::LEFT_CENTER,
        value,
        Face::Number.at(size::BODY),
        tint,
    );
    if !note.is_empty() {
        painter.text(
            pos2(after.right() + space::MD, rect.center().y),
            Align2::LEFT_CENTER,
            note,
            Face::Body.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}
