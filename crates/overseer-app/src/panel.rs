//! The detail panel: one player, in full.
//!
//! The board answers "who is here". This answers "what about them", and it is
//! the only place in the app where a paragraph is allowed. Its hierarchy is
//! deliberately steep: a name, then the one thing worth knowing, then the
//! numbers, then the small print. Somebody who reads only the first two lines
//! should still have got the point.
//!
//! Everything the backend knows is in here somewhere, which is the answer to
//! "where did that field go": the board shows what fits, and this shows the
//! rest.

use egui::{Align2, Color32, Rect, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_core::Player;

use crate::design::{Face, colour, kd, label_text, rank, size, space};

/// Where a value starts, so labels and values have a spine down the middle.
const VALUE_X: f32 = 60.0;

/// Draws the panel for a player, or the reason there is nobody to draw.
pub(crate) fn show(ui: &mut Ui, player: Option<&Player>) {
    let Some(player) = player else {
        heading(ui, "no one selected");
        line(
            ui,
            "Click a row, or use the arrow keys.",
            colour::TEXT_DIM,
            size::BODY,
        );
        return;
    };
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            name(ui, player);
            identity(ui, player);
            if !player.smurf_reasons.is_empty() {
                flags(ui, player);
            }
            ranks(ui, player);
            form(ui, player);
            numbers(ui, player);
            group(ui, player);
            met(ui, player);
            loadout(ui, player);
            ui.add_space(space::XL);
        });
}

/// The player's name, at the top, in the brightest thing there is.
fn name(ui: &mut Ui, player: &Player) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XXL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    // The tag is part of the name and not part of the point, so it is drawn
    // quieter rather than dropped: two people with the same name is exactly
    // when the tag matters.
    let full = player.display_name().to_owned();
    let (stem, tag) = full
        .split_once('#')
        .map_or((full.as_str(), ""), |(a, b)| (a, b));
    let painter = ui.painter().clone();
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
    if player.name_hidden {
        painter.text(
            pos2(rect.right() - space::LG, rect.center().y),
            Align2::RIGHT_CENTER,
            label_text("hidden"),
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

/// Level, role, agent and title, because they are one thought.
fn identity(ui: &mut Ui, player: &Player) {
    let mut parts: Vec<String> = Vec::new();
    if let Some(level) = player.level {
        parts.push(if player.level_hidden {
            "Level hidden".to_owned()
        } else {
            format!("Level {level}")
        });
    }
    if let Some(role) = player.role.as_deref() {
        parts.push(role.to_owned());
    }
    if let Some(agent) = player.agent.as_deref() {
        parts.push(agent.to_owned());
    }
    if !parts.is_empty() {
        line(ui, &parts.join("  \u{b7}  "), colour::TEXT_DIM, size::BODY);
    }
    if let Some(title) = player.title.as_deref() {
        line(ui, title, colour::TEXT_FAINT, size::MICRO);
    }
}

/// Why this account is worth a second look, in the backend's words.
///
/// Gold, and only gold: this and the encounter count are the two things in
/// the app that are claims about a person rather than measurements of one,
/// and they share a colour so that the colour means something.
fn flags(ui: &mut Ui, player: &Player) {
    ui.add_space(space::LG);
    heading_tinted(
        ui,
        if player.smurf {
            "smurf"
        } else {
            "worth a look"
        },
        colour::WARN,
    );
    for reason in &player.smurf_reasons {
        line(ui, reason, colour::WARN, size::BODY);
    }
    if !player.smurf {
        line(
            ui,
            "One signal only, so not a flag.",
            colour::TEXT_FAINT,
            size::MICRO,
        );
    }
}

/// Where they are now, and the best they have ever been.
fn ranks(ui: &mut Ui, player: &Player) {
    ui.add_space(space::LG);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    if ui.is_rect_visible(rect) {
        rank_line(ui, player, rect);
    }
    if let Some(place) = player.leaderboard {
        line(
            ui,
            &format!("Leaderboard #{place}"),
            colour::WARN,
            size::BODY,
        );
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
    if let Some(previous) = player.previous_rank.as_deref() {
        line(
            ui,
            &format!("Last act {previous}"),
            colour::TEXT_FAINT,
            size::MICRO,
        );
    }
}

/// Rank, rating, what the last match did to it, and how far through they are.
fn rank_line(ui: &Ui, player: &Player, rect: Rect) {
    let painter = ui.painter().clone();
    let mut x = rect.left() + space::LG;
    let after = painter.text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        player.rank.clone().unwrap_or_else(|| "Unranked".to_owned()),
        Face::Body.at(size::BODY),
        rank(player.rank_tier),
    );
    x = after.right() + space::MD;

    let Some(rr) = player.rr else { return };
    let after = painter.text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        rr.to_string(),
        Face::Number.at(size::BODY),
        colour::TEXT,
    );
    let mut after = painter.text(
        pos2(after.right() + space::SM, rect.center().y),
        Align2::LEFT_CENTER,
        "RR",
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    if let Some(delta) = player.rr_earned.filter(|d| *d != 0) {
        let tint = if delta > 0 { colour::GOOD } else { colour::BAD };
        let text = if delta > 0 {
            format!("+{delta}")
        } else {
            delta.to_string()
        };
        after = painter.text(
            pos2(after.right() + space::MD, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            Face::Number.at(size::MICRO),
            tint,
        );
    }
    // How far through the rank they are, as a bar rather than a second
    // number: the number is already there, and the bar is the thing that is
    // read without being read.
    let x = after.right() + space::MD;
    let track = Rect::from_min_size(
        pos2(x, rect.center().y - 2.0),
        vec2((rect.right() - space::LG - x).clamp(0.0, 72.0), 4.0),
    );
    painter.rect_filled(track, 0, colour::LINE);
    let mut filled = track;
    filled.set_width(track.width() * (rr as f32 / 100.0).clamp(0.0, 1.0));
    painter.rect_filled(filled, 0, colour::INFO);
}

/// Recent results, the run they are on, and what they play.
fn form(ui: &mut Ui, player: &Player) {
    if player.form.is_empty() && player.top_agents.is_empty() {
        return;
    }
    ui.add_space(space::LG);
    heading(ui, "form");
    if !player.form.is_empty() {
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
        if ui.is_rect_visible(rect) {
            pips(ui, player, rect);
        }
    }
    let mains: Vec<String> = player
        .top_agents
        .iter()
        .take(3)
        .map(|a| {
            format!(
                "{} {}",
                a.agent.clone().unwrap_or_else(|| "?".to_owned()),
                a.games.unwrap_or(0)
            )
        })
        .collect();
    if !mains.is_empty() {
        stat(ui, "mains", &mains.join("   "), colour::TEXT, "");
    }
}

/// The last ten results, and the run if there is one worth saying.
fn pips(ui: &Ui, player: &Player, rect: Rect) {
    let painter = ui.painter().clone();
    let mut x = rect.left() + space::LG;
    for result in player.form.iter().take(10) {
        let tint = match result.chars().next() {
            Some('W' | 'w') => colour::ALLY,
            Some('D' | 'd') => colour::WARN,
            _ => colour::ENEMY,
        };
        let pip = Rect::from_min_size(pos2(x, rect.center().y - 4.0), vec2(6.0, 8.0));
        painter.rect_filled(pip, 0, tint.gamma_multiply(0.85));
        x += 8.0;
    }
    let Some(streak) = player.streak.as_ref() else {
        return;
    };
    let count = streak.count.unwrap_or(0);
    if count < 2 {
        return;
    }
    let won = streak.kind.as_deref() == Some("W");
    painter.text(
        pos2(x + space::MD, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{count} {} in a row", if won { "won" } else { "lost" }),
        Face::Body.at(size::MICRO),
        if won { colour::ALLY } else { colour::ENEMY },
    );
}

/// The numbers, with the sample size behind each one.
///
/// A number with nothing behind it reads as a career average, and the K/D
/// here is the last few matches. Saying so costs four characters.
fn numbers(ui: &mut Ui, player: &Player) {
    ui.add_space(space::LG);
    heading(ui, "numbers");
    let over = if player.form.is_empty() {
        String::new()
    } else {
        format!("last {}", player.form.len())
    };
    stat(
        ui,
        "k/d",
        &player.kd.map_or_else(dash, |v| format!("{v:.2}")),
        kd(player.kd),
        &over,
    );
    stat(
        ui,
        "hs",
        &player.hs_pct.map_or_else(dash, |v| format!("{v:.1}%")),
        colour::TEXT,
        &over,
    );
    let win = player
        .win_rate
        .map_or_else(dash, |v| format!("{}%", v.round()));
    let games = player
        .games
        .map_or_else(String::new, |g| format!("over {g}"));
    stat(ui, "win", &win, colour::TEXT, &games);
    if let Some(map) = player
        .map_win_rate
        .as_ref()
        .filter(|m| m.games.unwrap_or(0) > 0)
    {
        let rate = format!("{}%", map.win_rate.unwrap_or(0.0).round());
        let note = format!("this map, over {}", map.games.unwrap_or(0));
        stat(ui, "map", &rate, colour::TEXT, &note);
    }
}

/// Who they came with, and how sure we are.
fn group(ui: &mut Ui, player: &Player) {
    if let Some(size) = player
        .party
        .as_ref()
        .and_then(|p| p.size)
        .filter(|s| *s > 1)
    {
        ui.add_space(space::LG);
        heading_tinted(ui, "party", colour::INFO);
        line(
            ui,
            &format!("{}, and Riot says so", stack_word(size)),
            colour::INFO,
            size::BODY,
        );
        return;
    }
    let Some(guess) = player.stack_guess.as_ref() else {
        return;
    };
    ui.add_space(space::LG);
    heading_tinted(ui, "probably together", colour::WARN);
    line(
        ui,
        &stack_word(guess.size.unwrap_or(0)),
        colour::WARN,
        size::BODY,
    );
    // The evidence, always. A guess presented as a fact is a lie, and this
    // one accuses strangers of queueing together.
    line(
        ui,
        &format!(
            "{} of {} lobbies on the same side, {}% sure",
            guess.same.unwrap_or(0),
            guess.shared.unwrap_or(0),
            guess.confidence.unwrap_or(0)
        ),
        colour::TEXT_FAINT,
        size::MICRO,
    );
}

/// How a group of that size is said out loud.
fn stack_word(size: u32) -> String {
    match size {
        0 | 1 => "On their own".to_owned(),
        2 => "A duo".to_owned(),
        3 => "A trio".to_owned(),
        n => format!("A {n} stack"),
    }
}

/// What history you have with them.
fn met(ui: &mut Ui, player: &Player) {
    let Some(encounter) = player.encounter.as_ref().filter(|e| e.total() > 0) else {
        return;
    };
    ui.add_space(space::LG);
    heading_tinted(
        ui,
        &format!("met {} times before", encounter.total()),
        colour::WARN,
    );
    let with = encounter.with_count.unwrap_or(0);
    if with > 0 {
        stat(
            ui,
            "with",
            &record(
                encounter.wins_with,
                encounter.losses_with,
                encounter.draws_with,
            ),
            colour::TEXT,
            &format!("over {with}"),
        );
    }
    let against = encounter.against_count.unwrap_or(0);
    if against > 0 {
        stat(
            ui,
            "against",
            &record(
                encounter.wins_against,
                encounter.losses_against,
                encounter.draws_against,
            ),
            colour::TEXT,
            &format!("over {against}"),
        );
    }
}

/// A win, loss and draw record, said the way a player says it.
fn record(wins: Option<u32>, losses: Option<u32>, draws: Option<u32>) -> String {
    let draws = draws.unwrap_or(0);
    let base = format!("{}W-{}L", wins.unwrap_or(0), losses.unwrap_or(0));
    if draws > 0 {
        format!("{base}-{draws}D")
    } else {
        base
    }
}

/// What they are carrying, when the backend could see it.
fn loadout(ui: &mut Ui, player: &Player) {
    let skins: Vec<String> = player
        .weapons
        .iter()
        .filter_map(|w| {
            let weapon = w.weapon.clone()?;
            let skin = w.skin.as_ref()?.name.clone()?;
            Some(format!("{weapon}  {skin}"))
        })
        .take(4)
        .collect();
    if skins.is_empty() {
        return;
    }
    ui.add_space(space::LG);
    heading(ui, "carrying");
    for skin in skins {
        line(ui, &skin, colour::TEXT_DIM, size::MICRO);
    }
}

/// A section heading: caps, tracked, faint, with air above it.
fn heading(ui: &mut Ui, text: &str) {
    heading_tinted(ui, text, colour::TEXT_FAINT);
}

/// A section heading in a colour, for the sections that are claims.
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

/// A label, a value, and a quieter note after it.
fn stat(ui: &mut Ui, label: &str, value: &str, tint: Color32, note: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    painter.text(
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        label_text(label),
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    let after = painter.text(
        pos2(rect.left() + space::LG + VALUE_X, rect.center().y),
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

/// What a missing value looks like, in one place.
fn dash() -> String {
    "-".to_owned()
}

#[cfg(test)]
mod tests {
    use super::{record, stack_word};

    #[test]
    fn a_record_only_mentions_draws_when_there_were_some() {
        assert_eq!(record(Some(3), Some(1), Some(0)), "3W-1L");
        assert_eq!(record(Some(3), Some(1), Some(2)), "3W-1L-2D");
        assert_eq!(record(None, None, None), "0W-0L");
    }

    /// The words a player would use, rather than the number the code has.
    #[test]
    fn a_group_is_named_the_way_it_is_said() {
        assert_eq!(stack_word(1), "On their own");
        assert_eq!(stack_word(2), "A duo");
        assert_eq!(stack_word(3), "A trio");
        assert_eq!(stack_word(5), "A 5 stack");
    }
}
