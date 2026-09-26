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

use egui::{Align2, Color32, Rect, RichText, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_core::Player;

use crate::career::{self, Career};
use crate::notes::{self, Notes};
use overseer_ui::{
    Face, art, caps_at, caps_text, colour, hex, kd, motion, rank, shape, size, space,
};

/// Where a value starts, so labels and values have a spine down the middle.
const VALUE_X: f32 = 60.0;

/// Draws the panel for a player, or the reason there is nobody to draw.
///
/// Returns whether the notes want writing to disk, which is the one thing in
/// here that changes anything outside the window.
pub(crate) fn show(
    ui: &mut Ui,
    player: Option<&Player>,
    store: &mut Notes,
    career: &Career,
) -> bool {
    let Some(player) = player else {
        heading(ui, "no one selected");
        line(
            ui,
            "Click a row, or use the arrow keys.",
            colour::TEXT_DIM,
            size::BODY,
        );
        return false;
    };
    let mut save = false;
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(space::MD);
            let plate = ui.painter().add(egui::Shape::Noop);
            let top = ui.cursor().top();
            name(ui, player);
            identity(ui, player);
            identity_plate(ui, player, plate, top);
            ui.add_space(space::MD);
            if !player.smurf_reasons.is_empty() {
                flags(ui, player);
            }
            save = notes(ui, player, store);
            ranks(ui, player);
            form(ui, player);
            numbers(ui, player);
            group(ui, player);
            met(ui, player);
            loadout(ui, player);
            // Last, because it is the part that arrives late and the part
            // that is longest. Everything above it is on the board already
            // and should not move when an answer lands.
            if let Some(puuid) = player.puuid.as_deref() {
                career::show(ui, career, puuid);
            }
            ui.add_space(space::XL);
        });
    save
}

/// What you know about them that no API does.
///
/// High up, above the ranks, because a line you wrote yourself beats every
/// number under it: "duos with the Jett, plays for picks" decides a match in a
/// way a win rate does not.
///
/// Written back to the store on every keystroke so the mark on the row and the
/// tags appear as you type, but only committed to disk when a box loses the
/// keyboard. The tag line keeps its raw text in egui's own scratch space while
/// it has focus, because tidying "toxic," into "toxic" under somebody's cursor
/// eats the comma they just typed.
fn notes(ui: &mut Ui, player: &Player, store: &mut Notes) -> bool {
    let Some(id) = player.puuid.as_deref() else {
        return false;
    };
    // Most people you meet are not worth writing about, and two empty boxes
    // pushing the ranks down the panel for all of them is a bad trade. Shut
    // until there is something in it, or until you say there will be.
    let opened = egui::Id::new(("note-open", id));
    let mut note = store.get(id);
    if note.is_empty() && !ui.data(|d| d.get_temp::<bool>(opened)).unwrap_or(false) {
        if invite(ui).clicked() {
            ui.data_mut(|d| d.insert_temp(opened, true));
        }
        return false;
    }
    heading(ui, "your notes");
    let mut text = note.text.clone();
    let key = egui::Id::new(("tags", id));
    let mut tags = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| notes::tags_line(&note.tags));
    let (mut touched, mut done) = (false, false);
    ui.horizontal(|ui| {
        ui.add_space(space::LG);
        let width = (ui.available_width() - space::LG).max(120.0);
        ui.vertical(|ui| {
            // Ids from the account, not from where the box sits. Two
            // players' boxes are at the same place in the panel, and with
            // positional ids the focused box quietly became the next
            // player's box when the pointer moved, and the rest of the
            // sentence was saved under their name.
            let prose = ui.add(
                egui::TextEdit::multiline(&mut text)
                    .id(egui::Id::new(("note-text", id)))
                    .desired_width(width)
                    .desired_rows(2)
                    .font(Face::Body.at(size::BODY))
                    .hint_text(hint("what you want to remember about them")),
            );
            ui.add_space(space::SM);
            let labels = ui.add(
                egui::TextEdit::singleline(&mut tags)
                    .id(egui::Id::new(("note-tags", id)))
                    .desired_width(width)
                    .font(Face::Body.at(size::MICRO))
                    .hint_text(hint("tags, separated by commas")),
            );
            touched = prose.changed() || labels.changed();
            done = prose.lost_focus() || labels.lost_focus();
        });
    });
    if touched || done {
        note.text = text;
        note.tags = notes::parse_tags(&tags);
        player.display_name().clone_into(&mut note.name);
        store.set(id, note);
    }
    if done {
        // Let the tidied form come back the next time it is looked at.
        ui.data_mut(|d| d.remove::<String>(key));
    } else if touched {
        ui.data_mut(|d| d.insert_temp(key, tags));
    }
    done
}

/// The one faint line that stands in for the notes section until there is a
/// note. Small and quiet: it is an offer, not a thing to do.
fn invite(ui: &mut Ui) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::click());
    if ui.is_rect_visible(rect) {
        let tint = if response.hovered() {
            colour::TEXT_DIM
        } else {
            colour::TEXT_FAINT
        };
        ui.painter().text(
            pos2(rect.left() + space::LG, rect.center().y),
            Align2::LEFT_CENTER,
            "+ write a note about them",
            Face::Body.at(size::MICRO),
            tint,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Grey placeholder text, in the panel's own face.
fn hint(text: &str) -> RichText {
    RichText::new(text)
        .color(colour::TEXT_FAINT)
        .font(Face::Body.at(size::MICRO))
}

/// The player's name, at the top, in the brightest thing there is.
fn name(ui: &mut Ui, player: &Player) {
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::XXL + space::MD),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    // No rail down the side any more: the plate behind this block is washed
    // in the agent's own colour, which ties the panel to the row it came
    // from more plainly than three points of bar ever did, and two marks
    // saying the same thing is one mark too many.
    // The tag is part of the name and not part of the point, so it is drawn
    // quieter rather than dropped: two people with the same name is exactly
    // when the tag matters.
    let full = player.display_name().to_owned();
    let (stem, tag) = full
        .split_once('#')
        .map_or((full.as_str(), ""), |(a, b)| (a, b));
    let painter = ui.painter().clone();
    // Cut rather than run off the edge. Riot allow sixteen characters and
    // this column is three hundred points wide with a face in the first
    // sixty of them, so the long ones do not fit and never did.
    let mut job = egui::text::LayoutJob::simple_singleline(
        stem.to_owned(),
        Face::Body.at(size::DISPLAY),
        colour::TEXT_STRONG,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: rect.width() - BESIDE_FACE - space::LG - 44.0,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    let after = Rect::from_min_size(
        pos2(
            rect.left() + BESIDE_FACE,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley.size(),
    );
    painter.galley(after.min, galley, colour::TEXT_STRONG);
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
        caps_at(
            &painter,
            pos2(rect.right() - space::LG, rect.center().y),
            Align2::RIGHT_CENTER,
            "hidden",
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

/// How big the portrait on the panel's own plate is.
const FACE: f32 = 42.0;

/// Where everything beside that portrait starts.
const BESIDE_FACE: f32 = space::MD + FACE + space::XL;

/// The surface the name and the line under it sit on, in the agent's own
/// colour.
///
/// The panel is the one part of the window with nothing behind it: a column
/// of text and hairlines, which is what a document looks like rather than
/// what a card about a person looks like. Its top is now a plate washed from
/// the colour of whoever it is about, which does two jobs at once. It gives
/// the column something to start on, and it is the second place the agent's
/// colour appears, so moving the pointer down the roster changes the colour
/// of the panel and you can see the subject change out of the corner of your
/// eye without reading a word.
fn identity_plate(ui: &Ui, player: &Player, at: egui::layers::ShapeIdx, top: f32) {
    let tint = hex(player.agent_color.as_deref()).unwrap_or(colour::BG_RAISED);
    let plate = Rect::from_min_max(
        pos2(ui.max_rect().left() + space::LG, top),
        pos2(
            ui.max_rect().right() - space::LG,
            ui.cursor().top() - space::SM,
        ),
    );
    if plate.height() < space::ROW {
        return;
    }
    let mut shapes = vec![shape::lit(
        plate,
        shape::CHAMFER,
        shape::blend(colour::BG_RAISED, tint, 0.20),
        shape::blend(colour::BG_RAISED, tint, 0.04),
    )];
    // The face, at the size a face is worth drawing at. On the board it is
    // twenty points and doing the job of a letter; here there is room for
    // it to be the thing you recognise the panel by.
    let face = Rect::from_center_size(
        pos2(plate.left() + space::MD + FACE / 2.0, plate.center().y),
        vec2(FACE, FACE),
    );
    shapes.push(shape::lit(
        face,
        5.0,
        shape::blend(tint, colour::TEXT_STRONG, 0.30),
        shape::blend(tint, colour::VOID, 0.20),
    ));
    if let Some(portrait) = art::agent(ui.ctx(), player.agent.as_deref().unwrap_or("")) {
        shapes.push(shape::cut_image(face, 5.0, portrait.id()));
    }
    ui.painter().set(at, egui::Shape::Vec(shapes));
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
        line_at(
            ui,
            &parts.join("  \u{b7}  "),
            colour::TEXT_DIM,
            size::BODY,
            BESIDE_FACE,
        );
    }
    if let Some(title) = player.title.as_deref() {
        line_at(ui, title, colour::TEXT_FAINT, size::MICRO, BESIDE_FACE);
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
    rank_bar(ui, player);
    // Zero is not a place. The backend sends it for anybody who is not
    // on the leaderboard at all, and "Leaderboard #0" reads as a rank.
    if let Some(place) = player.leaderboard.filter(|p| *p > 0) {
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
    let tint = rank(player.rank_tier);
    let name = player.rank.clone().unwrap_or_else(|| "Unranked".to_owned());
    let galley = painter.layout_no_wrap(name, Face::Body.at(size::BODY), tint);
    let plate = Rect::from_min_size(
        pos2(rect.left() + space::LG, rect.center().y - 10.0),
        vec2(galley.size().x + space::MD * 2.0, 20.0),
    );
    if player.rank_tier.unwrap_or(0) > 0 {
        painter.add(shape::cut_wash(
            plate,
            5.0,
            tint.gamma_multiply(0.26),
            tint.gamma_multiply(0.10),
        ));
    }
    painter.galley(
        pos2(
            plate.left() + space::MD,
            plate.center().y - galley.size().y / 2.0,
        ),
        galley,
        tint,
    );

    let Some(rr) = player.rr else { return };
    let after = painter.text(
        pos2(plate.right() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        rr.to_string(),
        Face::Number.at(size::TITLE),
        colour::TEXT_STRONG,
    );
    let after = caps_text(
        &painter,
        pos2(after.right() + space::SM, rect.center().y + 1.0),
        Align2::LEFT_CENTER,
        "rr",
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
        painter.text(
            pos2(after.right() + space::LG, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            Face::Number.at(size::BODY),
            tint,
        );
    }
}

/// How far through the rank they are, across the whole panel.
///
/// A bar rather than a second number, because the number is already on the
/// line above and the bar is the thing that is read without being read. It
/// slides to its new length over the better part of a second: long, and
/// decelerating, so the length reads as a measurement being taken rather
/// than a value being set.
fn rank_bar(ui: &mut Ui, player: &Player) {
    let Some(rr) = player.rr else { return };
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::MD), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    // One id for every player rather than one each: switching subject should
    // slide the bar from where it was, which is the comparison you wanted
    // when you clicked the second name.
    let share = ui.ctx().animate_value_with_time(
        egui::Id::new("panel-rr"),
        (rr as f32 / 100.0).clamp(0.0, 1.0),
        motion::MEASURE,
    );
    let track = Rect::from_min_max(
        pos2(rect.left() + space::LG, rect.center().y - 2.0),
        pos2(rect.right() - space::LG, rect.center().y + 2.0),
    );
    let painter = ui.painter();
    painter.rect_filled(track, 0, colour::BG_INSET);
    let mut filled = track;
    filled.set_width(track.width() * share);
    painter.add(shape::pip(filled, rank(player.rank_tier)));
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
        let pip = Rect::from_min_size(
            pos2(x, rect.center().y - space::PIP / 2.0),
            vec2(space::PIP - 2.0, space::PIP),
        );
        painter.add(shape::pip(pip, tint));
        x += space::PIP + 2.0;
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
    let skins: Vec<(String, String)> = player
        .weapons
        .iter()
        .filter_map(|w| Some((w.weapon.clone()?, w.skin.as_ref()?.name.clone()?)))
        .take(4)
        .collect();
    if skins.is_empty() {
        return;
    }
    ui.add_space(space::LG);
    heading(ui, "carrying");
    for (weapon, skin) in skins {
        named(ui, &weapon, &skin);
    }
}

/// A label and a word, on the grid the numbers beside it are already on.
///
/// Four rows of "Vandal Heartstopper" set as one phrase each start at four
/// different places, which is four rows that do not line up with the two
/// sections above and below them. The weapon is the label and the skin is
/// the value, and both sit where every other label and value in this panel
/// sits. Set in the reading face rather than the mono one: digits line up
/// under each other and words do not, and a word in mono looks like a file
/// name.
fn named(ui: &mut Ui, label: &str, value: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    caps_at(
        &painter,
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    let at = rect.left() + space::LG + VALUE_X;
    let mut job = egui::text::LayoutJob::simple_singleline(
        value.to_owned(),
        Face::Body.at(size::MICRO),
        colour::TEXT_DIM,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: rect.right() - space::LG - at,
        max_rows: 1,
        break_anywhere: false,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(at, rect.center().y - galley.size().y / 2.0),
        galley,
        colour::TEXT_DIM,
    );
}

/// A section heading: caps, tracked, faint, with air above it.
pub(crate) fn heading(ui: &mut Ui, text: &str) {
    heading_tinted(ui, text, colour::TEXT_FAINT);
}

/// A section heading in a colour, for the sections that are claims.
///
/// A tick, the word, and a hairline running to the edge. The tick is what
/// makes a heading a heading: caps alone at this size disappear into the
/// values under them, and the eye needs somewhere to start each time it
/// comes back to the panel.
fn heading_tinted(ui: &mut Ui, text: &str, tint: Color32) {
    ui.add_space(space::MD);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.add(shape::tick(
        pos2(rect.left() + space::LG, rect.center().y - 5.0),
        10.0,
        tint,
    ));
    let drawn = caps_text(
        painter,
        pos2(rect.left() + space::LG + space::MD, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        Face::Display.at(size::LABEL),
        tint,
    );
    let from = drawn.right() + space::MD;
    if from < rect.right() - space::LG {
        painter.hline(
            from..=rect.right() - space::LG,
            rect.center().y,
            (1.0, colour::LINE),
        );
    }
}

/// One line of prose, at the panel's left margin.
pub(crate) fn line(ui: &mut Ui, text: &str, tint: Color32, points: f32) {
    line_at(ui, text, tint, points, space::LG);
}

/// The same line, indented past something.
fn line_at(ui: &mut Ui, text: &str, tint: Color32, points: f32, indent: f32) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().text(
        pos2(rect.left() + indent, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        Face::Body.at(points),
        tint,
    );
}

/// A label, a value, and a quieter note after it.
pub(crate) fn stat(ui: &mut Ui, label: &str, value: &str, tint: Color32, note: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    caps_at(
        &painter,
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        label,
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
    use egui::Ui;
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use overseer_core::Player;

    use super::{Notes, record, stack_word};
    use crate::notes::Note;

    /// A keystroke in the panel has to reach the store on the same frame.
    ///
    /// The boxes are drawn from the store every frame, so anything not
    /// written back is a letter the person watched themselves type and then
    /// watched disappear. Nothing else in the app has that shape, and no
    /// snapshot would catch it.
    #[test]
    fn what_you_type_about_somebody_is_kept() {
        let player = Player {
            puuid: Some("p1".to_owned()),
            name: Some("Day#9932".to_owned()),
            ..Player::default()
        };
        // Started with a tag and no prose, because the section is shut until
        // there is something in it and the invite that opens it is painted
        // rather than a widget, so there is no node to click.
        let mut store = Notes::default();
        store.set(
            "p1",
            Note {
                tags: vec!["duo".to_owned()],
                ..Note::default()
            },
        );

        let mut harness = Harness::builder()
            .with_size(egui::vec2(320.0, 300.0))
            .build_ui_state(
                move |ui: &mut Ui, state: &mut (bool, Notes)| {
                    if state.0 {
                        super::notes(ui, &player, &mut state.1);
                    }
                },
                (false, store),
            );
        // Same first frame rule as every other harness here: the faces are
        // bound on the frame after they are installed.
        overseer_ui::install_fonts(&harness.ctx);
        harness
            .ctx
            .set_style_of(egui::Theme::Dark, overseer_ui::style());
        harness.run();
        harness.state_mut().0 = true;
        harness.run();

        harness.get_by_role(Role::MultilineTextInput).focus();
        harness.run();
        harness
            .get_by_role(Role::MultilineTextInput)
            .type_text("instalocks");
        harness.run();
        assert_eq!(harness.state().1.get("p1").text, "instalocks");
        assert!(harness.state().1.has("p1"));
    }

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
