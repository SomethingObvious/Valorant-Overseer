//! The History screen: your last few matches, newest first. Each is a strip
//! with its map behind it, the result and your own line, and a click opens
//! its end-of-game scoreboard under it.

use std::collections::HashSet;

use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, pos2, vec2};
use overseer_core::{Bridge, Game, Line, Player, Recent};
use overseer_ui::{Face, caps_text, caps_width, colour, motion, size, space};

use crate::board::{self, Side, paint};
use crate::{header, view};

/// How many games the screen can be set to show.
pub(crate) const COUNTS: [u32; 3] = [5, 10, 20];

/// How tall a match's strip is.
const STRIP: f32 = 60.0;
/// How tall one line of a scoreboard is.
const ROW: f32 = 30.0;
/// How wide the agent's crop is on a scoreboard line, two across one down.
const CROP: f32 = 60.0;
/// The least room a name keeps before a column is dropped for it.
const NAME: f32 = 150.0;

/// A scoreboard column: its heading, how wide it is, what it shows, and how
/// good a figure is.
struct Column {
    /// The heading, in caps.
    head: &'static str,
    /// Its width, numbers set right against the right edge.
    width: f32,
    /// The figure for one player, or `None` when there isn't one.
    cell: fn(&Line) -> Option<String>,
    /// How good the figure is, from ordinary at 0 to as good as it gets at 1,
    /// or `None` for a figure that isn't better for being bigger.
    good: fn(&Line) -> Option<f32>,
}

/// Where a figure sits between ordinary and excellent.
fn scale(value: f64, ordinary: f64, excellent: f64) -> f32 {
    ((value - ordinary) / (excellent - ordinary)) as f32
}

/// Every column, left to right. A narrow window drops them from the right,
/// so the rarer figures go before kills and deaths do.
const COLUMNS: [Column; 9] = [
    Column {
        head: "K",
        width: 34.0,
        cell: |l| l.kills.map(|n| n.to_string()),
        good: |l| l.kills.map(|n| scale(f64::from(n), 12.0, 26.0)),
    },
    Column {
        head: "D",
        width: 34.0,
        cell: |l| l.deaths.map(|n| n.to_string()),
        good: |_| None,
    },
    Column {
        head: "A",
        width: 34.0,
        cell: |l| l.assists.map(|n| n.to_string()),
        good: |l| l.assists.map(|n| scale(f64::from(n), 4.0, 12.0)),
    },
    Column {
        head: "K/D",
        width: 52.0,
        cell: |l| kd(l).map(|kd| format!("{kd:.2}")),
        good: |l| kd(l).map(|kd| scale(kd, 0.9, 1.7)),
    },
    Column {
        head: "ACS",
        width: 50.0,
        cell: |l| l.acs.map(|n| n.to_string()),
        good: |l| l.acs.map(|n| scale(f64::from(n), 180.0, 300.0)),
    },
    Column {
        head: "ADR",
        width: 50.0,
        cell: |l| l.adr.map(|n| n.to_string()),
        good: |l| l.adr.map(|n| scale(f64::from(n), 120.0, 190.0)),
    },
    Column {
        head: "HS%",
        width: 50.0,
        cell: |l| l.hs_pct.map(|n| format!("{n}%")),
        good: |l| l.hs_pct.map(|n| scale(f64::from(n), 16.0, 35.0)),
    },
    Column {
        head: "KAST",
        width: 56.0,
        cell: |l| l.kast.map(|n| format!("{n}%")),
        good: |l| l.kast.map(|n| scale(f64::from(n), 65.0, 85.0)),
    },
    Column {
        head: "FB",
        width: 34.0,
        cell: |l| l.first_bloods.map(|n| n.to_string()),
        good: |l| l.first_bloods.map(|n| scale(f64::from(n), 1.0, 5.0)),
    },
];

/// The games, or where the request for them has got to.
#[derive(Debug, Default)]
enum Past {
    /// Nothing asked yet.
    #[default]
    Idle,
    /// Asked, with the last answer kept on screen until the new one lands.
    Asking {
        /// The bridge's id for the question.
        id: u64,
        /// What was showing when it was asked.
        kept: Option<Box<Recent>>,
    },
    /// The answer.
    Have(Box<Recent>),
    /// Why there are no games, in words for a player.
    Refused(String),
}

/// The screen's state: the games, and which of them are open.
#[derive(Debug, Default)]
pub(crate) struct History {
    /// The games, or where the request for them has got to.
    past: Past,
    /// The matches whose scoreboards are open, by id.
    open: HashSet<String>,
}

impl History {
    /// Asks for the newest `count`, keeping what is on screen until the
    /// answer lands.
    pub(crate) fn ask(&mut self, bridge: &Bridge, count: u32, live: bool) {
        // A question sent into a dead socket is queued until it reconnects,
        // and by then nobody is looking at the screen.
        if !live {
            return;
        }
        let kept = match std::mem::take(&mut self.past) {
            Past::Have(games) => Some(games),
            Past::Asking { kept, .. } => kept,
            Past::Idle | Past::Refused(_) => None,
        };
        let id = bridge.ask("history", serde_json::json!({ "count": count }));
        self.past = Past::Asking { id, kept };
    }

    /// Whether the answer with this id is the one being waited on.
    pub(crate) const fn waiting_on(&self, id: u64) -> bool {
        matches!(self.past, Past::Asking { id: waiting, .. } if waiting == id)
    }

    /// Takes the answer. The newest match opens by itself the first time,
    /// so the screen shows a scoreboard without a click.
    pub(crate) fn answered(&mut self, result: Result<serde_json::Value, String>) {
        let read = result.and_then(|value| {
            serde_json::from_value::<Recent>(value)
                .map_err(|_| "Your games came back in a shape this build can't read.".to_owned())
        });
        self.past = match read {
            Ok(games) => {
                if self.open.is_empty()
                    && let Some(id) = games.games.first().and_then(|g| g.match_id.clone())
                {
                    self.open.insert(id);
                }
                Past::Have(Box::new(games))
            }
            Err(why) => Past::Refused(why),
        };
    }

    /// A screen already holding `recent`, for the snapshot.
    #[cfg(test)]
    pub(crate) fn showing(recent: serde_json::Value) -> Self {
        let mut history = Self::default();
        history.answered(Ok(recent));
        history
    }

    /// Drops a question the socket lost, going back to what was showing.
    pub(crate) fn reconnected(&mut self) {
        if let Past::Asking { kept, .. } = &mut self.past {
            let restored = kept.take().map_or(Past::Idle, Past::Have);
            self.past = restored;
        }
    }

    /// Draws the screen, and returns a new count when one was picked.
    pub(crate) fn show(&mut self, ui: &mut Ui, count: u32, now_ms: i64, live: bool) -> Option<u32> {
        let mut picked = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(space::LG);
                view::title(
                    ui,
                    "History",
                    "Your most recent games. Click one for its scoreboard, or press Esc to go back.",
                );
                picked = counts(ui, count);
                if let Some(words) = self.status(count, live) {
                    view::note(ui, &words);
                }
                let games = match &self.past {
                    Past::Have(games) => Some(games),
                    Past::Asking { kept, .. } => kept.as_ref(),
                    Past::Idle | Past::Refused(_) => None,
                };
                let all: Vec<&Game> = games.iter().flat_map(|g| g.games.iter()).collect();
                for (at, game) in all.iter().enumerate() {
                    let id = egui::Id::new(("history-strip", at));
                    motion::arrive(ui, id, motion::stagger(at, all.len()), |ui| {
                        strip(ui, game, &mut self.open, now_ms);
                    });
                }
                ui.add_space(space::XXL);
            });
        picked
    }

    /// A line on where things stand, when there is something to say.
    fn status(&self, count: u32, live: bool) -> Option<String> {
        match &self.past {
            Past::Idle if !live => Some("Waiting for the backend to connect.".to_owned()),
            Past::Asking { .. } => Some(format!("Loading your last {count} games.")),
            Past::Refused(why) => Some(why.clone()),
            Past::Have(games) if games.games.is_empty() => {
                Some("There are no games on this account yet.".to_owned())
            }
            Past::Have(games) if games.partial => Some(format!(
                "Riot is limiting requests, so {} of {} came back. Open History again in a minute for the rest.",
                games.games.len(),
                games.asked.unwrap_or(count)
            )),
            Past::Idle | Past::Have(_) => None,
        }
    }
}

/// The row of counts to pick from. Returns the one clicked, if it changed.
fn counts(ui: &mut Ui, count: u32) -> Option<u32> {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    let painter = ui.painter().clone();
    let middle = rect.center().y;
    let said = caps_text(
        &painter,
        pos2(rect.left() + space::XL, middle),
        Align2::LEFT_CENTER,
        "Games to Show",
        paint::label(),
        colour::TEXT_FAINT,
    );
    let mut x = said.right() + space::LG;
    let mut picked = None;
    for n in COUNTS {
        let text = n.to_string();
        let wide = space::LG.mul_add(2.0, caps_width(&painter, &text, paint::label()));
        let chip = Rect::from_min_size(pos2(x, middle - 10.0), vec2(wide, 20.0));
        let hit = ui
            .interact(chip, ui.id().with(("history-count", n)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let on = n == count;
        let fill = if on {
            colour::TEXT_STRONG
        } else if hit.hovered() {
            colour::BG_HOVER
        } else {
            colour::BG_INSET
        };
        painter.add(paint::slant(chip, true, true, fill));
        let _drawn = caps_text(
            &painter,
            chip.center(),
            Align2::CENTER_CENTER,
            &text,
            paint::label(),
            if on { colour::BG } else { colour::TEXT_DIM },
        );
        hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &text));
        if hit.clicked() && !on {
            picked = Some(n);
        }
        x = chip.right() + space::SM;
    }
    picked
}

/// One match's strip, and its scoreboard under it when it is open.
fn strip(ui: &mut Ui, game: &Game, open: &mut HashSet<String>, now_ms: i64) {
    ui.add_space(space::MD);
    let (outer, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), STRIP), Sense::click());
    let rect = outer.shrink2(vec2(space::XL, 0.0));
    let id = game.match_id.clone().unwrap_or_default();
    if response.clicked() && !open.remove(&id) {
        open.insert(id.clone());
    }
    let is_open = open.contains(&id);
    let map = game.map.as_deref().unwrap_or("Unknown Map");
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, map));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        header::backdrop(&painter, rect, game.map.as_deref());
        if ui.rect_contains_pointer(rect) {
            painter.rect_filled(rect, 0, colour::TEXT_STRONG.gamma_multiply(0.04));
        }
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(4.0, STRIP)),
            0,
            outcome(game),
        );
        let limit = left(&painter, rect, game, now_ms);
        right(&painter, rect, game, (limit, is_open));
    }
    if is_open {
        scoreboard(ui, game);
    }
}

/// The map and what kind of match it was, when and how long. Returns where
/// the words end, which the right side must not cross.
fn left(painter: &egui::Painter, rect: Rect, game: &Game, now_ms: i64) -> f32 {
    let map = caps_text(
        painter,
        pos2(rect.left() + space::XL, rect.top() + 23.0),
        Align2::LEFT_CENTER,
        game.map.as_deref().unwrap_or("Unknown Map"),
        Face::Heavy.at(22.0),
        colour::TEXT_STRONG,
    );
    let mut about: Vec<String> = game.mode.iter().cloned().collect();
    if let Some(at) = game.started_at {
        about.push(ago(now_ms, at));
    }
    if let Some(ms) = game.length_ms.filter(|ms| *ms > 0) {
        about.push(format!("{} min", (ms as f64 / 60_000.0).round()));
    }
    let meta = caps_text(
        painter,
        pos2(rect.left() + space::XL + 1.0, rect.top() + 44.0),
        Align2::LEFT_CENTER,
        &about.join("  \u{b7}  "),
        Face::Display.at(size::MICRO),
        colour::TEXT_DIM,
    );
    map.right().max(meta.right())
}

/// How wide each of the strip's right-hand columns is, right to left: your
/// RR, your ACS, your K/D/A, your agent and the score. Fixed, so each sits at
/// the same place in every match whatever its figures are.
const RR_WIDE: f32 = 52.0;
/// See [`RR_WIDE`].
const ACS_WIDE: f32 = 52.0;
/// See [`RR_WIDE`].
const KDA_WIDE: f32 = 108.0;
/// See [`RR_WIDE`].
const SCORE_WIDE: f32 = 88.0;

/// The score, your agent, your line and your RR, each in its own fixed
/// column from the strip's end, and left out when it would reach `limit`.
fn right(painter: &egui::Painter, rect: Rect, game: &Game, (limit, is_open): (f32, bool)) {
    let end = rect.right() - space::LG;
    chevron(painter, pos2(end - 5.0, rect.center().y), is_open);
    let rr = end - 10.0 - space::XXL;
    let acs = rr - RR_WIDE - space::XL;
    let line_at = acs - ACS_WIDE - space::XL;
    let agent = line_at - KDA_WIDE - space::XL;
    let score_at = agent - CROP - space::XL;
    let fits = |right: f32, wide: f32| right - wide > limit + space::LG;
    if let Some(change) = game.rr_delta
        && fits(rr, RR_WIDE)
    {
        let tint = match change.signum() {
            1 => colour::ALLY,
            -1 => colour::ENEMY,
            _ => colour::TEXT_DIM,
        };
        let value = if change > 0 {
            format!("+{change}")
        } else {
            change.to_string()
        };
        figure(painter, (rr, rect), (&value, "RR"), tint);
    }
    let Some(you) = game.players.iter().find(|p| p.is_subject) else {
        return;
    };
    if fits(acs, ACS_WIDE) {
        let value = you.acs.map_or_else(|| "-".to_owned(), |n| n.to_string());
        figure(painter, (acs, rect), (&value, "ACS"), colour::TEXT_STRONG);
    }
    if fits(line_at, KDA_WIDE) {
        figure(
            painter,
            (line_at, rect),
            (&kda(you), "K / D / A"),
            colour::TEXT_STRONG,
        );
    }
    if fits(agent, CROP) {
        let face =
            Rect::from_min_size(pos2(agent - CROP, rect.center().y - 15.0), vec2(CROP, 30.0));
        paint::crop(painter, &stand_in(you), face);
    }
    if fits(score_at, SCORE_WIDE) {
        score(painter, (score_at, rect), game);
    }
}

/// A big figure over its small label, right against `x`.
fn figure(
    painter: &egui::Painter,
    (x, rect): (f32, Rect),
    (value, label): (&str, &str),
    tint: Color32,
) {
    let _value = caps_text(
        painter,
        pos2(x, rect.top() + 24.0),
        Align2::RIGHT_CENTER,
        value,
        Face::Heavy.at(20.0),
        tint,
    );
    let _label = caps_text(
        painter,
        pos2(x, rect.top() + 44.0),
        Align2::RIGHT_CENTER,
        label,
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
}

/// Your rounds in the result's colour, theirs dim beside them, and the
/// result under both, right against `x`, when the mode has a score.
fn score(painter: &egui::Painter, (x, rect): (f32, Rect), game: &Game) {
    let Some([ours, theirs]) = game.score else {
        return;
    };
    let big = Face::Heavy.at(24.0);
    let result = game.result.as_deref().unwrap_or("");
    let tint = outcome(game);
    let after = caps_text(
        painter,
        pos2(x, rect.top() + 24.0),
        Align2::RIGHT_CENTER,
        &theirs.to_string(),
        big.clone(),
        colour::TEXT_DIM,
    );
    let _ours = caps_text(
        painter,
        pos2(after.left() - space::MD, rect.top() + 24.0),
        Align2::RIGHT_CENTER,
        &ours.to_string(),
        big,
        tint,
    );
    let _result = caps_text(
        painter,
        pos2(x, rect.top() + 44.0),
        Align2::RIGHT_CENTER,
        result,
        Face::Display.at(size::MICRO),
        tint,
    );
}

/// A small arrow that points down under an open strip and right otherwise.
fn chevron(painter: &egui::Painter, c: Pos2, open: bool) {
    let stroke = Stroke::new(1.4, colour::TEXT_DIM);
    let points = if open {
        vec![
            c + vec2(-5.0, -2.5),
            c + vec2(0.0, 2.5),
            c + vec2(5.0, -2.5),
        ]
    } else {
        vec![
            c + vec2(-2.5, -5.0),
            c + vec2(2.5, 0.0),
            c + vec2(-2.5, 5.0),
        ]
    };
    painter.add(egui::Shape::line(points, stroke));
}

/// The match's scoreboard: your side then theirs, or everybody at once in a
/// mode with no sides.
fn scoreboard(ui: &mut Ui, game: &Game) {
    let width = 2.0f32.mul_add(-space::XL, ui.available_width());
    let columns = fitting(width);
    let mine = game.your_team.as_deref();
    let sides: HashSet<&str> = game
        .players
        .iter()
        .filter_map(|p| p.team.as_deref())
        .collect();
    ui.add_space(space::SM);
    if sides.len() == 2 && mine.is_some() {
        let (ours, theirs): (Vec<&Line>, Vec<&Line>) =
            game.players.iter().partition(|p| p.team.as_deref() == mine);
        block(ui, ("Your Team", colour::ALLY, Side::Ally), &ours, columns);
        block(
            ui,
            ("Enemy Team", colour::ENEMY, Side::Enemy),
            &theirs,
            columns,
        );
    } else {
        let everybody: Vec<&Line> = game.players.iter().collect();
        block(
            ui,
            ("The Lobby", colour::TEXT_STRONG, Side::Enemy),
            &everybody,
            columns,
        );
    }
}

/// As many columns as leave a name its room, from the left.
fn fitting(width: f32) -> &'static [Column] {
    let mut keep = COLUMNS.len();
    while keep > 0
        && CROP + NAME + COLUMNS.iter().take(keep).map(|c| c.width).sum::<f32>() + space::LG > width
    {
        keep -= 1;
    }
    COLUMNS.get(..keep).unwrap_or(&[])
}

/// One side's heading with the column names, then a line for each player.
fn block(
    ui: &mut Ui,
    (title, tint, side): (&str, Color32, Side),
    players: &[&Line],
    columns: &[Column],
) {
    let (outer, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    let rect = outer.shrink2(vec2(space::XL, 0.0));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(
            Rect::from_min_size(pos2(rect.left(), rect.center().y - 7.0), vec2(3.0, 14.0)),
            0,
            tint,
        );
        let _title = caps_text(
            painter,
            pos2(rect.left() + space::MD, rect.center().y),
            Align2::LEFT_CENTER,
            title,
            Face::Heavy.at(size::TITLE),
            tint,
        );
        let mut x = rect.right() - space::LG;
        for column in columns.iter().rev() {
            let _head = caps_text(
                painter,
                pos2(x, rect.center().y + 1.0),
                Align2::RIGHT_CENTER,
                column.head,
                paint::label(),
                colour::TEXT_FAINT,
            );
            x -= column.width;
        }
    }
    for (n, player) in players.iter().enumerate() {
        line(ui, player, side, columns, n.is_multiple_of(2));
    }
    ui.add_space(space::MD);
}

/// One player's line: their agent, their name, and a figure per column.
fn line(ui: &mut Ui, player: &Line, side: Side, columns: &[Column], stripe: bool) {
    let (outer, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    let rect = outer.shrink2(vec2(space::XL, 0.0));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    if player.is_subject {
        painter.rect_filled(rect, 0, colour::BG_SELECTED);
    } else if stripe {
        painter.rect_filled(rect, 0, colour::BG_RAISED);
    }
    let face = stand_in(player);
    paint::crop(
        painter,
        &face,
        Rect::from_min_size(rect.min + vec2(0.0, 1.0), vec2(CROP, ROW - 2.0)),
    );
    let mut x = rect.right() - space::LG;
    let numbers = Face::Number.at(size::BODY + 1.0);
    for column in columns.iter().rev() {
        let text = (column.cell)(player).unwrap_or_else(|| "-".to_owned());
        // Coloured by how good it is, in the side's colour, so the stand-out
        // figures on each team show without reading every number.
        let tint = match (column.good)(player) {
            Some(good) => paint::heat(side, good),
            None if player.is_subject => colour::TEXT_STRONG,
            None => colour::TEXT,
        };
        let _cell = paint::numeral(painter, &text, pos2(x, rect.center().y), &numbers, tint);
        x -= column.width;
    }
    name(
        painter,
        &face,
        (rect.left() + CROP + space::LG, x - space::LG),
        rect.center().y,
        player.is_subject,
    );
}

/// The name and its Riot tag, cut to fit between `left` and `right`.
fn name(painter: &egui::Painter, face: &Player, (left, right): (f32, f32), middle: f32, you: bool) {
    let (name, tag, _standing_in) = board::shown_name(face);
    let font = Face::Display.at(size::BODY + 1.0);
    let shown = board::fit(painter, name, &font, right - left);
    let drawn = caps_text(
        painter,
        pos2(left, middle),
        Align2::LEFT_CENTER,
        &shown,
        font,
        if you {
            colour::TEXT_STRONG
        } else {
            colour::TEXT
        },
    );
    let tag_font = Face::Number.at(size::MICRO);
    let tag = format!("#{tag}");
    if tag.len() > 1
        && drawn.right() + space::SM + caps_width(painter, &tag, tag_font.clone()) < right
    {
        let _tag = caps_text(
            painter,
            pos2(drawn.right() + space::SM, middle + 1.0),
            Align2::LEFT_CENTER,
            &tag,
            tag_font,
            colour::TEXT_FAINT,
        );
    }
}

/// A board player carrying just the name and agent, so the board's own crop
/// and name code can draw a scoreboard line.
fn stand_in(line: &Line) -> Player {
    Player {
        name: line.name.clone(),
        agent: line.agent.clone(),
        ..Player::default()
    }
}

/// Kills over deaths, or the kills when they never died.
fn kd(line: &Line) -> Option<f64> {
    let kills = f64::from(line.kills?);
    Some(
        line.deaths
            .filter(|d| *d > 0)
            .map_or(kills, |d| kills / f64::from(d)),
    )
}

/// Kills, deaths and assists the way the game's own scoreboard puts them.
fn kda(line: &Line) -> String {
    let each = |n: Option<u32>| n.map_or_else(|| "-".to_owned(), |n| n.to_string());
    format!(
        "{} / {} / {}",
        each(line.kills),
        each(line.deaths),
        each(line.assists)
    )
}

/// The result's colour: your green for a win, red for a loss, and dim for a
/// draw or a mode without one.
fn outcome(game: &Game) -> Color32 {
    match game.result.as_deref() {
        Some("Victory") => colour::ALLY,
        Some("Defeat") => colour::ENEMY,
        _ => colour::TEXT_FAINT,
    }
}

/// How long ago a match started, the way a person would say it.
fn ago(now_ms: i64, then_ms: i64) -> String {
    let minutes = (now_ms - then_ms).max(0) as f64 / 60_000.0;
    let hours = minutes / 60.0;
    let days = hours / 24.0;
    let count = |n: f64, unit: &str| {
        let n = n.floor();
        if n <= 1.0 {
            format!("1 {unit} ago")
        } else {
            format!("{n} {unit}s ago")
        }
    };
    if minutes < 1.0 {
        "Just now".to_owned()
    } else if hours < 1.0 {
        count(minutes, "minute")
    } else if days < 1.0 {
        count(hours, "hour")
    } else if days < 2.0 {
        "Yesterday".to_owned()
    } else if days < 14.0 {
        count(days, "day")
    } else {
        count(days / 7.0, "week")
    }
}

#[cfg(test)]
mod tests {
    use super::{COLUMNS, History, ago, fitting, kd, kda};
    use overseer_core::Line;

    /// Times read the way a person says them, at each boundary.
    #[test]
    fn times_ago_read_like_speech() {
        let minute = 60_000;
        let hour = 60 * minute;
        let day = 24 * hour;
        for (gap, said) in [
            (20_000, "Just now"),
            (minute, "1 minute ago"),
            (59 * minute, "59 minutes ago"),
            (hour, "1 hour ago"),
            (5 * hour + 30 * minute, "5 hours ago"),
            (day + hour, "Yesterday"),
            (3 * day, "3 days ago"),
            (15 * day, "2 weeks ago"),
        ] {
            assert_eq!(
                ago(1_000_000_000_000, 1_000_000_000_000 - gap),
                said,
                "{gap}"
            );
        }
        // A clock that disagrees with Riot's never says a game is in the future.
        assert_eq!(ago(0, 5 * minute), "Just now");
    }

    /// A player who never died has their kills as their K/D, and a missing
    /// figure is a dash rather than a zero.
    #[test]
    fn a_line_reads_without_its_gaps_turning_into_zeroes() {
        let line = Line {
            kills: Some(12),
            deaths: Some(0),
            assists: None,
            ..Line::default()
        };
        assert_eq!(kd(&line), Some(12.0));
        assert_eq!(kda(&line), "12 / 0 / -");
    }

    /// A wide window gets every column and a narrow one keeps kills and
    /// deaths longest.
    #[test]
    fn narrow_windows_drop_the_rarer_columns_first() {
        assert_eq!(fitting(2000.0).len(), COLUMNS.len());
        let heads: Vec<&str> = fitting(460.0).iter().map(|c| c.head).collect();
        assert_eq!(heads.first(), Some(&"K"));
        assert!(!heads.contains(&"FB"), "{heads:?}");
        assert!(fitting(0.0).is_empty());
    }

    /// The first answer opens the newest match, and a later one leaves what
    /// was opened by hand alone.
    #[test]
    fn the_newest_match_opens_by_itself_once() {
        let answer = |first: &str| {
            Ok(serde_json::json!({ "games": [{ "matchId": first }, { "matchId": "older" }] }))
        };
        let mut history = History::default();
        history.answered(answer("newest"));
        assert!(history.open.contains("newest"));
        history.open.clear();
        history.open.insert("older".to_owned());
        history.answered(answer("newer still"));
        assert_eq!(history.open.len(), 1);
        assert!(history.open.contains("older"));
    }
}
