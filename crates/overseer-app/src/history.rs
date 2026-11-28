use std::collections::HashSet;

use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, pos2, vec2};
use overseer_core::{Bridge, Game, Line, Player, Recent, Usual};
use overseer_ui::{Face, caps_text, caps_width, colour, motion, size, space};

use crate::board::{self, Side, paint};
use crate::notes::Notes;
use crate::{controls, header, view};

/// How many games History asks for, which is the most Riot gives at once.
pub(crate) const GAMES: u32 = 20;

const STRIP: f32 = 60.0;
const ROW: f32 = 30.0;
const CROP: f32 = 60.0;
const NAME: f32 = 150.0;
const RANK: f32 = 124.0;
const MET: f32 = 40.0;
/// What sits between the name and the figures: times met, rank and peak.
const STANDING: f32 = MET + RANK + RANK;

struct Column {
    head: &'static str,
    width: f32,
    cell: fn(&Line) -> Option<String>,
    /// How good the figure is, from ordinary at 0 to as good as it gets at 1,
    /// or `None` for one left plain. Only K/D and ACS are coloured, since a
    /// whole row lit up hides the game that stood out.
    good: fn(&Line) -> Option<f32>,
}

fn scale(value: f64, ordinary: f64, excellent: f64) -> f32 {
    ((value - ordinary) / (excellent - ordinary)) as f32
}

const COLUMNS: [Column; 9] = [
    Column {
        head: "K",
        width: 34.0,
        cell: |l| l.kills.map(|n| n.to_string()),
        good: |_| None,
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
        good: |_| None,
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
        good: |_| None,
    },
    Column {
        head: "HS%",
        width: 50.0,
        cell: |l| l.hs_pct.map(|n| format!("{n}%")),
        good: |_| None,
    },
    Column {
        head: "KAST",
        width: 56.0,
        cell: |l| l.kast.map(|n| format!("{n}%")),
        good: |_| None,
    },
    Column {
        head: "FB",
        width: 34.0,
        cell: |l| l.first_bloods.map(|n| n.to_string()),
        good: |_| None,
    },
];

#[derive(Debug, Default)]
enum Past {
    #[default]
    Idle,
    Asking {
        id: u64,
        kept: Option<Box<Recent>>,
    },
    Have(Box<Recent>),
    Refused {
        why: String,
        kept: Option<Box<Recent>>,
    },
}

impl Past {
    fn shown(&self) -> Option<&Recent> {
        match self {
            Self::Have(games) => Some(games),
            Self::Asking { kept, .. } | Self::Refused { kept, .. } => kept.as_deref(),
            Self::Idle => None,
        }
    }

    fn into_kept(self) -> Option<Box<Recent>> {
        match self {
            Self::Have(games) => Some(games),
            Self::Asking { kept, .. } | Self::Refused { kept, .. } => kept,
            Self::Idle => None,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct History {
    past: Past,
    open: HashSet<String>,
    pub(crate) notes: Notes,
}

impl History {
    pub(crate) fn new(notes: Notes) -> Self {
        Self {
            notes,
            ..Self::default()
        }
    }

    pub(crate) fn ask(&mut self, bridge: &Bridge, live: bool) {
        if !live {
            return;
        }
        let kept = std::mem::take(&mut self.past).into_kept();
        let id = bridge.ask("history", serde_json::json!({ "count": GAMES }));
        self.past = Past::Asking { id, kept };
    }

    pub(crate) const fn waiting_on(&self, id: u64) -> bool {
        matches!(self.past, Past::Asking { id: waiting, .. } if waiting == id)
    }

    pub(crate) fn answered(&mut self, result: Result<serde_json::Value, String>) {
        let read = result.and_then(|value| {
            serde_json::from_value::<Recent>(value)
                .map_err(|_| "Your games came back in a shape this version can't read. Updating Overseer usually fixes this.".to_owned())
        });
        let kept = std::mem::take(&mut self.past).into_kept();
        self.past = match read {
            Ok(games) => {
                if self.open.is_empty()
                    && let Some(id) = games.games.first().and_then(|g| g.match_id.clone())
                {
                    self.open.insert(id);
                }
                Past::Have(Box::new(games))
            }
            Err(why) => Past::Refused { why, kept },
        };
    }

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

    /// True when a note changed.
    pub(crate) fn show(&mut self, ui: &mut Ui, now_ms: i64, live: bool) -> bool {
        let mut noted = false;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(space::LG);
                view::title(
                    ui,
                    "History",
                    "Your last 20 games. Click one for its scoreboard, or press Esc to go back.",
                );
                if let Some(words) = self.status(live, now_ms) {
                    view::note(ui, &words);
                }
                let all: Vec<&Game> = self
                    .past
                    .shown()
                    .iter()
                    .flat_map(|g| g.games.iter())
                    .collect();
                for (at, game) in all.iter().enumerate() {
                    let id = egui::Id::new(("history-strip", at));
                    motion::arrive(ui, id, motion::stagger(at, all.len()), |ui| {
                        noted |= strip(ui, game, (&mut self.open, &mut self.notes), now_ms);
                    });
                }
                ui.add_space(space::XXL);
            });
        noted
    }

    fn status(&self, live: bool, now_ms: i64) -> Option<String> {
        match &self.past {
            Past::Idle if !live => Some("Waiting for the backend to connect.".to_owned()),
            Past::Refused { why, kept: None } => Some(why.clone()),
            Past::Refused { why, kept: Some(_) } => Some(format!(
                "Couldn't load newer games, so these are the ones from before. {why}"
            )),
            Past::Have(games) if games.games.is_empty() => {
                Some("There are no games on this account yet.".to_owned())
            }
            Past::Have(games) if games.saved_at.is_some() => Some(format!(
                "VALORANT isn't open or Riot can't be reached, so these are your saved games. The newest ended {}.",
                kept_ago(now_ms, games.saved_at)
            )),
            Past::Have(games) if games.partial => Some(format!(
                "Riot is limiting requests, so {} of {} came back. Open History again in a minute for the rest.",
                games.games.len(),
                games.asked.unwrap_or(GAMES)
            )),
            Past::Idle | Past::Asking { .. } | Past::Have(_) => None,
        }
    }
}

fn strip(
    ui: &mut Ui,
    game: &Game,
    (open, notes): (&mut HashSet<String>, &mut Notes),
    now_ms: i64,
) -> bool {
    ui.add_space(space::MD);
    let (outer, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), STRIP), Sense::click());
    let rect = outer.shrink2(vec2(space::XL, 0.0));
    let id = game.match_id.as_deref().unwrap_or_default();
    if response.clicked() && !open.remove(id) {
        open.insert(id.to_owned());
    }
    let is_open = open.contains(id);
    let map = game.map.as_deref().unwrap_or("Unknown Map");
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, map));
    let ctx = ui.ctx().clone();
    let hover = ctx.animate_bool_with_time(
        egui::Id::new(("history-hover", id)),
        ui.rect_contains_pointer(rect),
        motion::lasting(&ctx, motion::INSTANT),
    );
    let turned = ctx.animate_bool_with_time(
        egui::Id::new(("history-turn", id)),
        is_open,
        motion::lasting(&ctx, motion::QUICK),
    );
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        header::backdrop(&painter, rect, game.map.as_deref());
        if hover > 0.0 {
            painter.rect_filled(rect, 0, colour::TEXT_STRONG.gamma_multiply(0.05 * hover));
        }
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0f32.mul_add(hover, 4.0), STRIP)),
            0,
            outcome(game),
        );
        let limit = left(&painter, rect, game, (now_ms, notes.has(id)));
        right(&painter, rect, game, (limit, turned));
    }
    folding(ui, egui::Id::new(("history-fold", id)), is_open, |ui| {
        motion::arrive(ui, egui::Id::new(("history-open", id)), 0.0, |ui| {
            against(ui, game);
            let done = note(ui, game, notes);
            scoreboard(ui, game);
            done
        })
    })
    .unwrap_or(false)
}

/// Draws `add` while `open`, unfolding to its height as it opens and folding
/// back up as it closes, instead of jumping.
fn folding<R>(ui: &mut Ui, id: egui::Id, open: bool, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let ctx = ui.ctx().clone();
    let t = ctx.animate_bool_with_time_and_easing(
        id,
        open,
        motion::lasting(&ctx, motion::ENTER),
        egui::emath::easing::cubic_out,
    );
    if t <= 0.0 {
        return None;
    }
    let height = id.with("height");
    let full = ctx.data(|d| d.get_temp::<f32>(height)).unwrap_or(0.0);
    let top = ui.cursor().min;
    let width = ui.available_width();
    let shown = if t < 1.0 { full * t } else { f32::INFINITY };
    // A child the list doesn't grow for, so only the unfolded part of it
    // takes room and the games under it slide.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(top, vec2(width, FOLD_MOST)))
            .layout(*ui.layout()),
    );
    let mut clip = child.clip_rect();
    clip.max.y = clip.max.y.min(top.y + shown);
    child.set_clip_rect(clip);
    let out = add(&mut child);
    let drawn = child.min_rect().height();
    ctx.data_mut(|d| d.insert_temp(height, drawn));
    let _room = ui.allocate_space(vec2(width, drawn.min(shown)));
    Some(out)
}

/// Taller than any game's scoreboard and note, so none is cut short.
const FOLD_MOST: f32 = 100_000.0;

/// The game's note. The box only shows while you write, and the rest of the
/// time it is the note as written, or an invite when there is none. A click
/// on either opens the box, and Done, Esc or a click away closes it.
fn note(ui: &mut Ui, game: &Game, notes: &mut Notes) -> bool {
    let Some(id) = game.match_id.as_deref() else {
        return false;
    };
    let open_id = egui::Id::new(("game-note-open", id));
    let text_id = egui::Id::new(("game-note", id));
    let writing = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let mut kept = notes.get(id);
    ui.add_space(space::SM);
    let mut done = false;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(space::XL as i8, 0))
        .show(ui, |ui| {
            if !writing {
                if written(ui, &kept.text).clicked() {
                    ui.data_mut(|d| d.insert_temp(open_id, true));
                }
                return;
            }
            let mut text = kept.text.clone();
            let response = controls::notes_named(
                ui,
                &mut text,
                "Who you queued with, what went wrong, what to try next time",
                text_id,
            );
            // Focus goes in once the box exists, on the frame it first shows.
            if !ui.data(|d| d.get_temp::<bool>(text_id).unwrap_or(false)) {
                response.request_focus();
                ui.data_mut(|d| d.insert_temp(text_id, true));
            }
            if response.changed() {
                kept.text = text;
                kept.name = [game.map.as_deref(), game.mode.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(", ");
                notes.set(id, kept);
            }
            ui.add_space(space::SM);
            let finished = ui
                .allocate_ui_with_layout(
                    vec2(ui.available_width(), space::XXL),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| controls::button(ui, "Done", controls::Tone::Primary, true).clicked(),
                )
                .inner;
            let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if finished || escaped || response.lost_focus() {
                ui.data_mut(|d| {
                    d.insert_temp(open_id, false);
                    d.insert_temp(text_id, false);
                });
                done = true;
            }
        });
    done
}

/// The note as written on its slab, or the invite to write one, as one
/// click target.
fn written(ui: &mut Ui, text: &str) -> egui::Response {
    let text = text.trim();
    let width = ui.available_width();
    if text.is_empty() {
        let (rect, response) = ui.allocate_exact_size(vec2(width, space::XL), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Add a note")
        });
        if ui.is_rect_visible(rect) {
            ui.painter().text(
                pos2(rect.left(), rect.center().y),
                Align2::LEFT_CENTER,
                "+ Add a Note",
                Face::Body.at(size::MICRO),
                if response.hovered() {
                    colour::TEXT_STRONG
                } else {
                    colour::TEXT_DIM
                },
            );
        }
        return response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    let pad = vec2(space::LG, space::MD);
    let galley = ui.painter().layout(
        text.to_owned(),
        Face::Body.at(size::BODY),
        colour::TEXT,
        2.0f32.mul_add(-pad.x, width),
    );
    let label = size::MICRO + space::SM;
    let tall = 2.0f32.mul_add(pad.y, galley.size().y + label);
    let (rect, response) = ui.allocate_exact_size(vec2(width, tall), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let ground = if response.hovered() {
            colour::BG_HOVER
        } else {
            colour::BG_RAISED
        };
        painter.add(paint::slant(rect, false, true, ground));
        let _label = caps_text(
            painter,
            rect.min + pad,
            Align2::LEFT_TOP,
            "Your Note",
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        if response.hovered() {
            let _edit = caps_text(
                painter,
                pos2(rect.right() - pad.x - space::MD, rect.top() + pad.y),
                Align2::RIGHT_TOP,
                "Edit",
                Face::Display.at(size::MICRO),
                colour::TEXT_STRONG,
            );
        }
        painter.galley(rect.min + pad + vec2(0.0, label), galley, colour::TEXT);
    }
    response.on_hover_cursor(egui::CursorIcon::Text)
}

const AGAINST_WIDE: f32 = 104.0;

fn against(ui: &mut Ui, game: &Game) {
    let (Some(usual), Some(you)) = (
        game.usual.as_ref(),
        game.players.iter().find(|p| p.is_subject),
    ) else {
        return;
    };
    let (outer, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
    let rect = outer.shrink2(vec2(space::XL, 0.0));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let said = caps_text(
        painter,
        pos2(rect.left() + space::MD, rect.top() + 18.0),
        Align2::LEFT_CENTER,
        "Against Your Usual",
        Face::Heavy.at(size::TITLE),
        colour::TEXT_DIM,
    );
    let games = usual.games.unwrap_or_default();
    let over = format!(
        "Your {games} {} games before this one",
        game.mode.as_deref().unwrap_or("earlier")
    );
    let sub = caps_text(
        painter,
        pos2(rect.left() + space::MD, rect.top() + 36.0),
        Align2::LEFT_CENTER,
        &over,
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    let mut x = rect.right() - space::LG;
    let limit = said.right().max(sub.right()) + space::LG;
    for (head, now, then, unit) in figures(you, usual).into_iter().rev() {
        let (Some(now), Some(then)) = (now, then) else {
            continue;
        };
        if x - AGAINST_WIDE < limit {
            break;
        }
        compared(painter, (x, rect.top()), head, (now, then), unit);
        x -= AGAINST_WIDE;
    }
}

type Figure = (&'static str, Option<f64>, Option<f64>, Unit);

fn figures(you: &Line, usual: &Usual) -> [Figure; 5] {
    let whole = |n: Option<u32>| n.map(f64::from);
    [
        ("K/D", kd(you), usual.kd, Unit::Ratio),
        ("ACS", whole(you.acs), usual.acs, Unit::Whole),
        ("ADR", whole(you.adr), usual.adr, Unit::Whole),
        ("HS%", whole(you.hs_pct), usual.hs_pct, Unit::Share),
        ("KAST", whole(you.kast), usual.kast, Unit::Share),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    /// Two places, like a K/D of 1.32.
    Ratio,
    /// A whole number, like an ACS of 242.
    Whole,
    Share,
}

impl Unit {
    fn write(self, n: f64) -> String {
        match self {
            Self::Ratio => format!("{n:.2}"),
            Self::Whole => format!("{n:.0}"),
            Self::Share => format!("{n:.0}%"),
        }
    }

    const fn even(self) -> f64 {
        match self {
            Self::Ratio => 0.05,
            Self::Whole | Self::Share => 2.0,
        }
    }
}

fn compared(
    painter: &egui::Painter,
    (x, top): (f32, f32),
    head: &str,
    (now, then): (f64, f64),
    unit: Unit,
) {
    let gap = now - then;
    let tint = if gap.abs() < unit.even() {
        colour::TEXT_DIM
    } else if gap > 0.0 {
        colour::ALLY
    } else {
        colour::ENEMY
    };
    let sign = if gap < 0.0 { "-" } else { "+" };
    let _head = caps_text(
        painter,
        pos2(x, top + 12.0),
        Align2::RIGHT_CENTER,
        head,
        paint::label(),
        colour::TEXT_FAINT,
    );
    let numbers = Face::Number.at(size::BODY + 1.0);
    let moved = paint::numeral(
        painter,
        &format!("{sign}{}", unit.write(gap.abs())),
        pos2(x, top + 32.0),
        &numbers,
        tint,
    );
    let _now = paint::numeral(
        painter,
        &unit.write(now),
        pos2(moved.left() - space::SM, top + 32.0),
        &numbers,
        colour::TEXT_STRONG,
    );
}

fn left(painter: &egui::Painter, rect: Rect, game: &Game, (now_ms, noted): (i64, bool)) -> f32 {
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
    if noted {
        about.push("Noted".to_owned());
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

const RR_WIDE: f32 = 52.0;
const RANK_WIDE: f32 = 136.0;
const ACS_WIDE: f32 = 52.0;
const KDA_WIDE: f32 = 108.0;
const SCORE_WIDE: f32 = 88.0;

fn right(painter: &egui::Painter, rect: Rect, game: &Game, (limit, turned): (f32, f32)) {
    let end = rect.right() - space::LG;
    chevron_turned(
        painter,
        pos2(end - 5.0, rect.center().y),
        turned,
        colour::TEXT_DIM,
    );
    let rr = end - 10.0 - space::XXL;
    let fits = |right: f32, wide: f32| right - wide > limit + space::LG;
    let rank_at = rr - RR_WIDE - space::XL;
    let ranked = game.tier_after.is_some_and(|t| t >= 3) && fits(rank_at, RANK_WIDE);
    if ranked {
        rank_after(painter, (rank_at, rect), game);
    }
    let acs = if ranked {
        rank_at - RANK_WIDE - space::XL
    } else {
        rr - RR_WIDE - space::XL
    };
    let line_at = acs - ACS_WIDE - space::XL;
    let agent = line_at - KDA_WIDE - space::XL;
    let score_at = agent - CROP - space::XL;
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

/// The rank a game left you at, with its emblem, its name and the RR in it,
/// and a chip when the game moved you up or down a rank. A rank up comes in
/// lit, and its emblem keeps its glow.
fn rank_after(painter: &egui::Painter, (x, rect): (f32, Rect), game: &Game) {
    let Some(tier) = game.tier_after else {
        return;
    };
    let left = x - RANK_WIDE;
    let moved = game.tier_before.filter(|b| *b >= 3).map(|b| tier.cmp(&b));
    let up = moved == Some(std::cmp::Ordering::Greater);
    let id = egui::Id::new(("history-rank", game.match_id.as_deref().unwrap_or_default()));
    let lit = if up {
        1.0 - motion::enter(painter.ctx(), id, 0.25)
    } else {
        0.0
    };
    let emblem = 34.0;
    paint::emblem(
        painter,
        tier,
        pos2(left + emblem / 2.0, rect.center().y),
        emblem,
        if up { 1.0 } else { 0.0 },
    );
    let text_x = left + emblem + space::MD;
    let _name = caps_text(
        painter,
        pos2(text_x, rect.top() + 24.0),
        Align2::LEFT_CENTER,
        game.rank_after.as_deref().unwrap_or_default(),
        Face::Heavy.at(size::TITLE + 2.0),
        colour::TEXT_STRONG,
    );
    let rr = game
        .rr_after
        .map_or_else(String::new, |rr| format!("{rr} RR"));
    let said = caps_text(
        painter,
        pos2(text_x, rect.top() + 44.0),
        Align2::LEFT_CENTER,
        &rr,
        Face::Display.at(size::MICRO),
        colour::TEXT_DIM,
    );
    let (word, tint) = match moved {
        Some(std::cmp::Ordering::Greater) => ("Rank Up", colour::ALLY),
        Some(std::cmp::Ordering::Less) => ("Derank", colour::ENEMY),
        _ => return,
    };
    let font = Face::Cut.at(size::MICRO);
    let wide = space::MD.mul_add(2.0, caps_width(painter, word, font.clone()));
    let at = if rr.is_empty() {
        text_x
    } else {
        said.right() + space::MD
    };
    let chip = Rect::from_min_size(pos2(at, rect.top() + 44.0 - 7.5), vec2(wide, 15.0));
    let fill = overseer_ui::shape::blend(tint, colour::TEXT_STRONG, lit);
    painter.add(paint::slant(chip, true, true, fill));
    let _word = caps_text(
        painter,
        chip.center(),
        Align2::CENTER_CENTER,
        word,
        font,
        colour::BG,
    );
}

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
        Face::Heavy.at(size::DISPLAY),
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
    let ours = caps_text(
        painter,
        pos2(after.left() - space::MD, rect.top() + 24.0),
        Align2::RIGHT_CENTER,
        &ours.to_string(),
        big,
        tint,
    );
    if result.is_empty() {
        return;
    }
    let font = Face::Cut.at(size::MICRO);
    let wide = space::MD.mul_add(2.0, caps_width(painter, result, font.clone()));
    let chip = Rect::from_center_size(
        pos2(f32::midpoint(ours.left(), x), rect.top() + 44.0),
        vec2(wide, 15.0),
    );
    painter.add(paint::slant(chip, true, true, tint));
    let _result = caps_text(
        painter,
        chip.center(),
        Align2::CENTER_CENTER,
        result,
        font,
        colour::BG,
    );
}

pub(crate) fn chevron(painter: &egui::Painter, c: Pos2, open: bool, tint: Color32) {
    chevron_turned(painter, c, if open { 1.0 } else { 0.0 }, tint);
}

/// A chevron turned from pointing right at 0 to pointing down at 1.
fn chevron_turned(painter: &egui::Painter, c: Pos2, turned: f32, tint: Color32) {
    let (sin, cos) = (turned * std::f32::consts::FRAC_PI_2).sin_cos();
    let points = [vec2(-2.5, -5.0), vec2(2.5, 0.0), vec2(-2.5, 5.0)]
        .map(|p| c + vec2(p.x.mul_add(cos, -p.y * sin), p.x.mul_add(sin, p.y * cos)))
        .to_vec();
    painter.add(egui::Shape::line(points, Stroke::new(1.4, tint)));
}

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

fn fitting(width: f32) -> &'static [Column] {
    let mut keep = COLUMNS.len();
    while keep > 0
        && CROP
            + NAME
            + STANDING
            + COLUMNS.iter().take(keep).map(|c| c.width).sum::<f32>()
            + space::LG
            > width
    {
        keep -= 1;
    }
    COLUMNS.get(..keep).unwrap_or(&[])
}

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
        for (head, at, align) in [
            ("Met", met_right(x), Align2::RIGHT_CENTER),
            ("Rank", x - RANK - RANK, Align2::LEFT_CENTER),
            ("Peak", x - RANK, Align2::LEFT_CENTER),
        ] {
            let _head = caps_text(
                painter,
                pos2(at, rect.center().y + 1.0),
                align,
                head,
                paint::label(),
                colour::TEXT_FAINT,
            );
        }
    }
    for (n, player) in players.iter().enumerate() {
        let id = ui.id().with(("history-line", n, title));
        motion::arrive(ui, id, motion::stagger(n, players.len()), |ui| {
            line(ui, player, side, columns, n.is_multiple_of(2));
        });
    }
    ui.add_space(space::MD);
}

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
        let tint = match (column.good)(player) {
            Some(good) => paint::heat(side, good),
            None if player.is_subject => colour::TEXT_STRONG,
            None => colour::TEXT,
        };
        let _cell = paint::numeral(painter, &text, pos2(x, rect.center().y), &numbers, tint);
        x -= column.width;
    }
    let middle = rect.center().y;
    rank(
        painter,
        (player.tier, player.rank.as_deref()),
        pos2(x - RANK - RANK, middle),
        player.is_subject,
    );
    rank(
        painter,
        (player.peak, player.peak_rank.as_deref()),
        pos2(x - RANK, middle),
        player.is_subject,
    );
    if let Some(met) = player.met.filter(|&n| n > 0) {
        let _met = paint::numeral(
            painter,
            &format!("{met}x"),
            pos2(met_right(x), middle),
            &Face::Number.at(size::BODY),
            colour::INFO,
        );
    }
    name(
        painter,
        &face,
        (rect.left() + CROP + space::LG, x - STANDING - space::LG),
        rect.center().y,
        player.is_subject,
    );
}

/// Where times met ends, right aligned like the figures, given where the
/// figures begin.
fn met_right(figures: f32) -> f32 {
    figures - RANK - RANK - space::LG
}

/// A rank with its emblem, their rank in that game or their peak, or a dash
/// when there is none to show.
fn rank(painter: &egui::Painter, (tier, name): (Option<u32>, Option<&str>), at: Pos2, you: bool) {
    let tier = tier.unwrap_or(0);
    let Some(name) = name.filter(|_| tier >= 3) else {
        let _none = caps_text(
            painter,
            at,
            Align2::LEFT_CENTER,
            "-",
            paint::label(),
            colour::TEXT_FAINT,
        );
        return;
    };
    let emblem = 20.0;
    paint::emblem(painter, tier, pos2(at.x + emblem / 2.0, at.y), emblem, 0.0);
    let _name = caps_text(
        painter,
        pos2(at.x + emblem + space::SM, at.y),
        Align2::LEFT_CENTER,
        name,
        Face::Display.at(size::LABEL + 1.0),
        if you {
            colour::TEXT_STRONG
        } else {
            colour::TEXT
        },
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

fn stand_in(line: &Line) -> Player {
    Player {
        name: line.name.clone(),
        agent: line.agent.clone(),
        ..Player::default()
    }
}

fn kd(line: &Line) -> Option<f64> {
    let kills = f64::from(line.kills?);
    Some(
        line.deaths
            .filter(|d| *d > 0)
            .map_or(kills, |d| kills / f64::from(d)),
    )
}

fn kda(line: &Line) -> String {
    let each = |n: Option<u32>| n.map_or_else(|| "-".to_owned(), |n| n.to_string());
    format!(
        "{} / {} / {}",
        each(line.kills),
        each(line.deaths),
        each(line.assists)
    )
}

fn outcome(game: &Game) -> Color32 {
    match game.result.as_deref() {
        Some("Victory") => colour::ALLY,
        Some("Defeat") => colour::ENEMY,
        _ => colour::TEXT_FAINT,
    }
}

/// When something kept at `at`, in seconds since 1970, was, for the middle
/// of a sentence: "3 hours ago", or "yesterday".
pub(crate) fn kept_ago(now_ms: i64, at: Option<f64>) -> String {
    at.map_or_else(
        || "earlier".to_owned(),
        |at| ago(now_ms, (at * 1000.0) as i64).to_lowercase(),
    )
}

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
    use super::{History, Past, Unit};

    #[test]
    fn figures_are_written_by_kind() {
        assert_eq!(Unit::Ratio.write(1.234), "1.23");
        assert_eq!(Unit::Whole.write(241.6), "242");
        assert_eq!(Unit::Share.write(21.4), "21%");
    }

    #[test]
    fn a_failed_refresh_keeps_the_games_from_before() {
        let mut history = History::showing(serde_json::json!({ "games": [] }));
        let Past::Have(games) = std::mem::take(&mut history.past) else {
            panic!("the games didn't land");
        };
        history.past = Past::Asking {
            id: 1,
            kept: Some(games),
        };
        history.answered(Err("Riot is down.".to_owned()));
        assert!(matches!(history.past, Past::Refused { kept: Some(_), .. }));
        assert!(
            history
                .status(true, 0)
                .is_some_and(|said| said.contains("from before"))
        );
    }
}
