//! The bottom half of the panel: one account's recent history. The board is
//! pushed once a second and has to stay small, so this is only asked for when
//! somebody is selected. It answers what the board raises, like whether that
//! Diamond 3 came last week on the way up or last month on the way down.

use std::collections::HashMap;

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use overseer_core::{Bridge, CareerMatch, Profile};

use crate::board::{self, Side};
use crate::panel::{heading, line, stat};
use overseer_ui::{Face, caps_text, colour, size, space};

/// How tall a chart is: enough for a shape to read as one, and short enough
/// that the numbers under it stay on the panel.
const CHART: f32 = 44.0;
/// How many matches to list. The history has eight to ten, and listing all
/// of them pushes the sections under it out of reach.
const LISTED: usize = 5;

/// Where the request for one account's history has got to, so the panel can
/// say it is waiting or was refused instead of going blank.
#[derive(Debug, Default)]
pub(crate) enum Career {
    /// Nobody has been selected yet.
    #[default]
    Nothing,
    /// Asked, waiting.
    Asking {
        /// The id the answer will carry.
        id: u64,
        /// Who it is about.
        puuid: String,
    },
    /// Answered.
    Have {
        /// Who it is about.
        puuid: String,
        /// Boxed because it is much larger than the other variants, and the
        /// enum would be that size everywhere it is stored.
        profile: Box<Profile>,
    },
    /// Answered with a reason there is nothing to show.
    Refused {
        /// Who it was about.
        puuid: String,
        /// What the backend said, which is usually actionable.
        why: String,
    },
}

impl Career {
    /// Which account this concerns, whatever state it is in.
    fn about(&self) -> Option<&str> {
        match self {
            Self::Nothing => None,
            Self::Asking { puuid, .. } | Self::Have { puuid, .. } | Self::Refused { puuid, .. } => {
                Some(puuid)
            }
        }
    }

    /// Asks for whoever is selected now, unless this is already about them.
    pub(crate) fn follow(
        &mut self,
        bridge: &Bridge,
        selected: Option<&str>,
        live: bool,
        seen: &HashMap<String, Box<Profile>>,
    ) {
        let Some(puuid) = selected else {
            *self = Self::Nothing;
            return;
        };
        if self.about() == Some(puuid) {
            return;
        }
        // Fetched earlier in this lobby, so there is nothing new to ask.
        if let Some(profile) = seen.get(puuid) {
            *self = Self::Have {
                puuid: puuid.to_owned(),
                profile: profile.clone(),
            };
            return;
        }
        // A question sent into a dead socket is queued until it reconnects,
        // and by then nobody wants the answer.
        if !live {
            return;
        }
        let id = bridge.ask("profile", serde_json::json!({ "puuid": puuid }));
        *self = Self::Asking {
            id,
            puuid: puuid.to_owned(),
        };
    }

    /// Forgets a question that was in flight when the socket dropped, since
    /// nothing will answer it now.
    pub(crate) fn reconnected(&mut self) {
        if matches!(self, Self::Asking { .. }) {
            *self = Self::Nothing;
        }
    }

    /// Takes an answer, if it is the one being waited on.
    pub(crate) fn answered(
        &mut self,
        id: u64,
        result: Result<serde_json::Value, String>,
        seen: &mut HashMap<String, Box<Profile>>,
    ) {
        let Self::Asking { id: waiting, puuid } = self else {
            return;
        };
        // Any other id answers a question abandoned when the selection moved
        // on, and taking it would put one player's history under another's
        // name.
        if *waiting != id {
            return;
        }
        let puuid = puuid.clone();
        *self = match result {
            Err(why) => Self::Refused { puuid, why },
            Ok(value) => match serde_json::from_value::<Profile>(value) {
                Ok(profile) => {
                    let profile = Box::new(profile);
                    seen.insert(puuid.clone(), profile.clone());
                    Self::Have { puuid, profile }
                }
                // Worded for a player, because the decoder's own error is a
                // line of field names.
                Err(_) => Self::Refused {
                    puuid,
                    why: "Their history came back in a shape this build cannot read.".to_owned(),
                },
            },
        };
    }
}

/// Draws whatever there is for this account.
pub(crate) fn show(ui: &mut Ui, career: &Career, puuid: &str, side: Side) {
    match career {
        Career::Asking { puuid: who, .. } if who == puuid => {
            heading(ui, "Career");
            line(
                ui,
                "Reading their last matches.",
                colour::TEXT_FAINT,
                size::MICRO,
            );
        }
        Career::Refused { puuid: who, why } if who == puuid => {
            heading(ui, "Career");
            line(ui, why, colour::TEXT_DIM, size::MICRO);
        }
        Career::Have {
            puuid: who,
            profile,
        } if who == puuid => {
            let id = egui::Id::new(("career-in", who));
            overseer_ui::motion::arrive(ui, id, 0.0, |ui| history(ui, profile, side));
        }
        _ => {}
    }
}

/// Everything a full history has to say, in the order it is worth reading.
fn history(ui: &mut Ui, profile: &Profile, side: Side) {
    if profile.matches.is_empty() {
        heading(ui, "Career");
        line(ui, "No matches on record.", colour::TEXT_DIM, size::MICRO);
        return;
    }
    averages(ui, profile);
    rating(ui, profile, side);
    per_match_kd(ui, profile, side);
    guns(ui, profile);
    matches(ui, profile, side);
    together(ui, profile);
    habits(ui, profile);
}

/// Kills, deaths and assists a game. The K/D stays with the numbers above, so
/// there are not two K/Ds over different games to choose between.
fn averages(ui: &mut Ui, profile: &Profile) {
    let a = &profile.averages;
    let games = a.games.unwrap_or(profile.matches.len() as u32);
    heading(ui, &format!("Career, Last {games}"));
    let [kills, deaths, assists] =
        [a.kills, a.deaths, a.assists].map(|v| format!("{:.1}", v.unwrap_or(0.0)));
    crate::panel::figures(
        ui,
        &[
            (&kills, colour::TEXT, "Kills a game"),
            (&deaths, colour::TEXT, "Deaths"),
            (&assists, colour::TEXT, "Assists"),
        ],
    );
}

/// Where their rating has been going, as a line of ladder positions so a
/// promotion reads as a rise.
fn rating(ui: &mut Ui, profile: &Profile, side: Side) {
    let run = profile.rating_run();
    if run.len() < 2 {
        return;
    }
    heading(ui, "Rating");
    let points: Vec<f32> = run.iter().map(|m| ladder(m)).collect();
    // A rating that never moved gets a sentence instead of a flat line with a
    // dot at each end.
    let flat = points
        .iter()
        .all(|p| (p - points.first().copied().unwrap_or(0.0)).abs() < 0.5);
    if !flat {
        rating_chart(ui, &run, &points);
    }

    let first = run.first().and_then(|m| m.rank_after.clone());
    let last = run.last().and_then(|m| m.rank_after.clone());
    let moved = points
        .last()
        .zip(points.first())
        .map_or(0.0, |(end, start)| end - start);
    let path = match (first, last) {
        (Some(a), Some(b)) if a != b => format!("{a} to {b}"),
        (_, Some(b)) => b,
        _ => String::new(),
    };
    let change = if flat {
        "Unchanged".to_owned()
    } else {
        format!("{moved:+.0} RR")
    };
    let tint = if moved > 0.0 {
        board::paint::win(side)
    } else {
        colour::TEXT_DIM
    };
    rating_summary(ui, &path, (&change, tint), run.len());
}

/// Under the chart, set like the panel's other figures: where they went on
/// the left, and on the right what it came to and over how many games.
fn rating_summary(ui: &mut Ui, path: &str, (change, tint): (&str, egui::Color32), games: usize) {
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::XL + space::SM),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let y = rect.center().y;
    let over = caps_text(
        &painter,
        pos2(rect.right() - space::LG, y + 1.0),
        Align2::RIGHT_CENTER,
        &format!("over {games} games"),
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    let changed = caps_text(
        &painter,
        pos2(over.left() - space::MD, y),
        Align2::RIGHT_CENTER,
        change,
        Face::Heavy.at(size::TITLE + 1.0),
        tint,
    );
    let font = Face::Display.at(size::LABEL + 1.0);
    let room = changed.left() - space::LG - (rect.left() + space::LG);
    let _path = caps_text(
        &painter,
        pos2(rect.left() + space::LG, y),
        Align2::LEFT_CENTER,
        &board::fit(&painter, path, &font, room),
        font,
        colour::TEXT,
    );
}

/// The agent, plus the mode when it was not competitive, since a swiftplay
/// K/D is not a ranked one.
fn played(m: &CareerMatch) -> String {
    let mode = m
        .mode
        .as_deref()
        .filter(|mode| !mode.eq_ignore_ascii_case("competitive"));
    [m.agent.as_deref(), mode]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

/// How tall the rating chart is, taller than the others so each tier it
/// crosses has a band tall enough to name.
const RATING_CHART: f32 = 96.0;

/// Room down the rating chart's left for the tier names.
const TIER_LABELS: f32 = 64.0;

/// How far the chart's top and bottom lines stand off the rating line.
const BREATHE: f32 = 4.0;

/// The rating chart's scale: a dashed line at every tier's edge, each tier
/// named down the left in its own colour, and a barely there 0 and 100.
fn scale(painter: &egui::Painter, (rect, inner): (Rect, Rect), (low, high): (f32, f32)) {
    let y_of = |v: f32| inner.bottom() - inner.height() * (v - low) / (high - low);
    let tiers = ((high - low) / 100.0).round() as u32;
    let band = inner.height() / tiers.max(1) as f32;
    // Every band named when they are tall enough to read, every other one
    // when a long slide squeezes them.
    let every = if band >= 13.0 { 1 } else { 2 };
    let first = (low / 100.0).round() as u32;
    // Dashed and fainter than the panel's dividers, so the lines read as a
    // scale and not as the end of a section.
    let rule = egui::Stroke::new(1.0, colour::TEXT_FAINT.gamma_multiply(0.35));
    for k in 0..=tiers {
        let y = y_of(100.0f32.mul_add(k as f32, low));
        // The top and bottom lines stand a little off the chart, so a game
        // at 0 or 100 RR has room around it instead of lying on the line.
        let (nudge, edge) = match k {
            0 => (BREATHE, Some("0")),
            k if k == tiers => (-BREATHE, Some("100")),
            _ => (0.0, None),
        };
        painter.extend(egui::Shape::dashed_line(
            &[
                pos2(inner.left(), y + nudge),
                pos2(inner.right(), y + nudge),
            ],
            rule,
            3.0,
            3.0,
        ));
        if let Some(rr) = edge {
            painter.text(
                pos2(inner.left() - space::SM, y + nudge),
                Align2::RIGHT_CENTER,
                rr,
                Face::Number.at(size::MICRO - 2.0),
                colour::TEXT_FAINT.gamma_multiply(0.55),
            );
        }
        if k < tiers && k % every == 0 {
            let tier = first + k;
            let _named = caps_text(
                painter,
                pos2(rect.left() + space::LG, y - band / 2.0),
                Align2::LEFT_CENTER,
                &tier_name(tier),
                Face::Display.at(size::MICRO),
                tier_ink(tier),
            );
        }
    }
}

/// The rating as a line over the tiers it crossed, each tier a band named
/// down the left, and the game under the pointer read out beside its point.
fn rating_chart(ui: &mut Ui, run: &[&CareerMatch], points: &[f32]) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), RATING_CHART), Sense::hover());
    if !ui.is_rect_visible(rect) || points.len() < 2 {
        return;
    }
    let inner = Rect::from_min_max(
        pos2(
            rect.left() + space::LG + TIER_LABELS,
            rect.top() + space::SM + BREATHE,
        ),
        pos2(
            rect.right() - space::LG,
            rect.bottom() - space::SM - BREATHE,
        ),
    );
    // Out to whole tiers, so every band is a full tier and has a name.
    let (low, high) = points
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &p| {
            (lo.min(p), hi.max(p))
        });
    let low = (low / 100.0).floor() * 100.0;
    let high = ((high / 100.0).ceil() * 100.0).max(low + 100.0);
    let y_of = |v: f32| inner.bottom() - inner.height() * (v - low) / (high - low);
    let painter = ui.painter().clone();
    scale(&painter, (rect, inner), (low, high));
    let n = points.len() as f32;
    let spots: Vec<egui::Pos2> = points
        .iter()
        .enumerate()
        .map(|(i, &v)| pos2(inner.left() + inner.width() * i as f32 / (n - 1.0), y_of(v)))
        .collect();
    painter.add(egui::Shape::line(
        spots.clone(),
        egui::Stroke::new(2.0, colour::TEXT_STRONG),
    ));
    for spot in [spots.first(), spots.last()].into_iter().flatten() {
        painter.circle_filled(*spot, 3.0, colour::TEXT_STRONG);
    }
    if let Some(pointer) = response.hover_pos() {
        let i = nearest(inner, points.len(), pointer.x);
        if let (Some(spot), Some(m)) = (spots.get(i), run.get(i)) {
            readout(&painter, inner, *spot, m);
        }
    }
}

/// The game under the pointer: a guide down to it, a ring on it, and its
/// rank, rating and what that game did, on a plate beside it.
fn readout(painter: &egui::Painter, inner: Rect, spot: egui::Pos2, m: &CareerMatch) {
    painter.vline(spot.x, inner.y_range(), (1.0, colour::TEXT_FAINT));
    painter.circle_filled(spot, 3.5, colour::TEXT_STRONG);
    painter.circle_stroke(spot, 5.5, egui::Stroke::new(1.5, colour::TEXT_STRONG));
    let said = [
        m.rank_after.clone(),
        m.rr_after.map(|rr| format!("{rr} RR")),
        m.rr_delta.map(|d| format!("{d:+}")),
        m.map.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("  \u{b7}  ");
    let galley = painter.layout_no_wrap(said, Face::Number.at(size::LABEL), colour::TEXT_STRONG);
    let size = galley.size() + vec2(2.0 * space::MD, space::SM);
    // Beside the point on whichever side has room, and above it unless that
    // leaves the chart.
    let x = if spot.x + space::MD + size.x <= inner.right() {
        spot.x + space::MD
    } else {
        (spot.x - space::MD - size.x).max(inner.left() - TIER_LABELS)
    };
    let y = if spot.y - space::MD - size.y >= inner.top() {
        spot.y - space::MD - size.y
    } else {
        spot.y + space::MD
    };
    let plate = Rect::from_min_size(pos2(x, y), size);
    painter.rect_filled(plate, 0, colour::BG_INSET);
    painter.rect_stroke(plate, 0, (1.0, colour::LINE), egui::StrokeKind::Inside);
    painter.galley(
        pos2(
            plate.left() + space::MD,
            plate.center().y - galley.size().y / 2.0,
        ),
        galley,
        colour::TEXT_STRONG,
    );
}

/// A tier's name the way the game writes it: "Gold 2", "Radiant".
fn tier_name(tier: u32) -> String {
    if tier < 3 {
        return "Unranked".to_owned();
    }
    let group = overseer_ui::rank_group(tier);
    let mut name: String = group.chars().take(1).flat_map(char::to_uppercase).collect();
    name.push_str(group.get(1..).unwrap_or_default());
    if tier >= 27 {
        name
    } else {
        format!("{name} {}", tier.rem_euclid(3) + 1)
    }
}

/// A tier's own colour for its name, dimmed so the line stays the brightest
/// thing on the chart.
fn tier_ink(tier: u32) -> egui::Color32 {
    overseer_ui::shape::blend(overseer_ui::rank(Some(tier)), colour::TEXT_FAINT, 0.35)
}

/// A ladder position that does not restart every tier.
fn ladder(m: &CareerMatch) -> f32 {
    let tier = f64::from(m.tier_after.unwrap_or(0)) * 100.0;
    let rr = f64::from(m.rr_after.unwrap_or(0));
    (tier + rr) as f32
}

/// How often they were the problem, as bars against an even ratio.
fn per_match_kd(ui: &mut Ui, profile: &Profile, side: Side) {
    let mut values: Vec<f32> = profile
        .matches
        .iter()
        .filter_map(|m| m.kd)
        .map(|v| v as f32)
        .collect();
    if values.len() < 2 {
        return;
    }
    values.reverse();
    heading(ui, "K/D per Match");
    let _read = plot(ui, &values, Some(1.0), board::paint::win(side))
        .on_hover_text("Oldest first. The line is a K/D of one.");
}

/// Which guns did the killing, with a bar for the share.
fn guns(ui: &mut Ui, profile: &Profile) {
    if profile.top_guns.is_empty() {
        return;
    }
    heading(ui, "Guns");
    for gun in profile.top_guns.iter().take(3) {
        let Some(name) = gun.name.as_deref() else {
            continue;
        };
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::ROW_TIGHT), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter().clone();
        let share = f32::from(u16::try_from(gun.share.unwrap_or(0).min(100)).unwrap_or(0));
        // Set like the board's K/D: the numeral first, then a bar for
        // comparing it with the others.
        let numeral = caps_text(
            &painter,
            pos2(rect.left() + space::LG + 44.0, rect.center().y),
            Align2::RIGHT_CENTER,
            &format!("{}%", share as u32),
            Face::Heavy.at(size::TITLE + 2.0),
            colour::TEXT_STRONG,
        );
        let start = numeral.right() + space::MD;
        let room = rect.right() - space::LG - start;
        let track = Rect::from_min_size(pos2(start, rect.center().y - 9.0), vec2(room, 18.0));
        painter.add(board::paint::slant(track, false, true, colour::BG_RAISED));
        let bar = Rect::from_min_size(track.min, vec2((room * share / 100.0).max(space::SM), 18.0));
        painter.add(board::paint::slant(
            bar,
            false,
            true,
            overseer_ui::shape::blend(colour::LINE, colour::TEXT_DIM, 0.22),
        ));
        let _name = caps_text(
            &painter,
            pos2(start + space::SM, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            Face::Display.at(size::LABEL + 1.0),
            colour::TEXT,
        );
        let _kills = caps_text(
            &painter,
            pos2(rect.right() - space::LG, rect.center().y),
            Align2::RIGHT_CENTER,
            &format!("{} kills", gun.kills.unwrap_or(0)),
            Face::Number.at(size::LABEL),
            colour::TEXT_FAINT,
        );
    }
}

/// The last few matches, one line each.
fn matches(ui: &mut Ui, profile: &Profile, side: Side) {
    heading(ui, "Last Matches");
    for m in profile.matches.iter().take(LISTED) {
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::ROW_TIGHT), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter().clone();
        let won = m.won();
        let tint = if won {
            board::paint::win(side)
        } else {
            colour::TEXT_FAINT
        };
        // A thin rail for the result. It is the first thing the eye wants
        // and not worth a word's width.
        painter.rect_filled(
            Rect::from_min_size(
                pos2(rect.left() + space::LG, rect.top() + 3.0),
                vec2(2.0, rect.height() - 6.0),
            ),
            0,
            tint,
        );
        let left = rect.left() + space::LG + space::MD;
        let after = painter.text(
            pos2(left, rect.center().y),
            Align2::LEFT_CENTER,
            m.map.as_deref().unwrap_or("-"),
            Face::Body.at(size::MICRO),
            colour::TEXT,
        );
        painter.text(
            pos2(after.right() + space::MD, rect.center().y),
            Align2::LEFT_CENTER,
            played(m),
            Face::Body.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        let mut right = rect.right() - space::LG;
        if let Some(delta) = m.rr_delta {
            let drawn = painter.text(
                pos2(right, rect.center().y),
                Align2::RIGHT_CENTER,
                format!("{delta:+}"),
                Face::Number.at(size::MICRO),
                if delta >= 0 {
                    board::paint::win(side)
                } else {
                    colour::TEXT_DIM
                },
            );
            right = drawn.left() - space::MD;
        }
        painter.text(
            pos2(right, rect.center().y),
            Align2::RIGHT_CENTER,
            format!(
                "{}/{}/{}",
                m.kills.unwrap_or(0),
                m.deaths.unwrap_or(0),
                m.assists.unwrap_or(0)
            ),
            Face::Number.at(size::MICRO),
            colour::TEXT_DIM,
        );
    }
}

/// Who they keep turning up with, which is how a stack shows itself.
fn together(ui: &mut Ui, profile: &Profile) {
    // Once together is a random teammate, not somebody they play with.
    let named: Vec<&overseer_core::CoPlayer> = profile
        .co_players
        .iter()
        .filter(|c| c.name.is_some() && c.shared_matches.unwrap_or(0) >= 2)
        .take(5)
        .collect();
    if named.is_empty() {
        return;
    }
    // Your own career counts every lobby on record, and says so.
    if profile.co_players_from.as_deref() == Some("log") {
        heading(ui, "Played With Most");
    } else {
        heading(ui, &format!("Played With, Last {}", profile.matches.len()));
    }
    for mate in named {
        stat(
            ui,
            &format!("{} games", mate.shared_matches.unwrap_or(0)),
            mate.name.as_deref().unwrap_or("-"),
            colour::TEXT,
            "",
        );
    }
}

/// The two habits worth planning around.
fn habits(ui: &mut Ui, profile: &Profile) {
    let force = &profile.force_habit;
    let bonus = profile.bonus_buys.first();
    if force.pct.is_none() && bonus.is_none() {
        return;
    }
    heading(ui, "Habits");
    if let (Some(pct), Some(chances)) = (force.pct, force.chances) {
        stat(
            ui,
            "Force",
            &format!("{pct}%"),
            if pct >= 60 {
                colour::WARN
            } else {
                colour::TEXT
            },
            &format!("of {chances} lost pistols"),
        );
    }
    if let Some(buy) = bonus {
        stat(
            ui,
            "Bonus",
            buy.name.as_deref().unwrap_or("-"),
            colour::TEXT,
            &format!(
                "{}% of {} bonus rounds",
                buy.share.unwrap_or(0),
                profile.bonus_rounds.unwrap_or(0)
            ),
        );
    }
}

/// Draws a series as a line, or as bars measured from `baseline` when there
/// is one.
fn plot(ui: &mut Ui, values: &[f32], baseline: Option<f32>, hot: egui::Color32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), CHART), Sense::hover());
    if !ui.is_rect_visible(rect) || values.len() < 2 {
        return response;
    }
    let inner = inside(rect);
    let (low, high) = bounds(values, baseline);
    let n = values.len() as f32;
    let height_of = |v: f32| inner.bottom() - inner.height() * (v - low) / (high - low);
    let painter = ui.painter().clone();

    if let Some(rule) = baseline {
        // Bars sit centred in equal slots. On the line's endpoints the first
        // and last bars would hang half outside the box.
        let slot = inner.width() / n;
        let y = height_of(rule);
        painter.hline(inner.x_range(), y, (1.0, colour::LINE));
        for (i, &v) in values.iter().enumerate() {
            let middle = slot.mul_add(i as f32 + 0.5, inner.left());
            let top = height_of(v);
            let half = slot * 0.275;
            let bar = Rect::from_min_max(
                pos2(middle - half, top.min(y)),
                pos2(middle + half, top.max(y)),
            );
            let tint = if v >= rule { hot } else { colour::TEXT_FAINT };
            painter.rect_filled(bar, 0, tint);
        }
        return response;
    }
    let at = |i: usize, v: f32| {
        pos2(
            inner.left() + inner.width() * (i as f32) / (n - 1.0),
            height_of(v),
        )
    };

    let points: Vec<egui::Pos2> = values.iter().enumerate().map(|(i, &v)| at(i, v)).collect();
    painter.add(egui::Shape::line(
        points.clone(),
        egui::Stroke::new(2.0, hot),
    ));
    // Dots only where it started and where it got to. A dot on every point
    // turns the line into a row of beads.
    for point in [points.first(), points.last()].into_iter().flatten() {
        painter.circle_filled(*point, 3.0, hot);
    }
    // Ring the point under the pointer so the hover text has something to
    // refer to.
    if let Some(pointer) = response.hover_pos() {
        let i = nearest(inner, values.len(), pointer.x);
        if let Some(point) = points.get(i) {
            painter.circle_stroke(*point, 4.5, egui::Stroke::new(1.5, colour::TEXT_STRONG));
        }
    }
    response
}

/// The box the points are drawn in.
fn inside(rect: Rect) -> Rect {
    Rect::from_min_max(
        pos2(rect.left() + space::LG, rect.top() + space::SM),
        pos2(rect.right() - space::LG, rect.bottom() - space::SM),
    )
}

/// Which of `n` points evenly across `inner` is nearest to `x`.
fn nearest(inner: Rect, n: usize, x: f32) -> usize {
    let last = n.saturating_sub(1);
    let t = ((x - inner.left()) / inner.width().max(1.0)).clamp(0.0, 1.0);
    ((t * last as f32).round() as usize).min(last)
}

/// The range to draw over, padded so it is never zero wide and a flat series
/// sits through the middle.
fn bounds(values: &[f32], baseline: Option<f32>) -> (f32, f32) {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for &v in values.iter().chain(baseline.iter()) {
        low = low.min(v);
        high = high.max(v);
    }
    if !low.is_finite() || !high.is_finite() {
        return (0.0, 1.0);
    }
    let pad = ((high - low) * 0.15).max(0.05);
    (low - pad, high + pad)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{Career, bounds, ladder};
    use overseer_core::CareerMatch;

    /// Rating restarts at zero on promotion, so a raw plot draws a rank up as
    /// a crash.
    #[test]
    fn a_promotion_reads_as_a_rise() {
        let before = CareerMatch {
            tier_after: Some(17),
            rr_after: Some(96),
            ..CareerMatch::default()
        };
        let after = CareerMatch {
            tier_after: Some(18),
            rr_after: Some(12),
            ..CareerMatch::default()
        };
        assert!(
            ladder(&after) > ladder(&before),
            "ranking up drew as a fall"
        );
    }

    #[test]
    fn a_flat_series_does_not_divide_by_nothing() {
        let (low, high) = bounds(&[1.0, 1.0, 1.0], None);
        assert!(high > low, "a flat line had nowhere to be drawn");
        let (low, high) = bounds(&[0.9, 1.4], Some(1.0));
        assert!(low < 0.9 && high > 1.4, "the baseline fell outside the box");
        assert!(bounds(&[], None).1 > bounds(&[], None).0);
    }

    /// Ids are never reused, so a late answer belongs to a selection that has
    /// moved on.
    #[test]
    fn a_late_answer_to_an_abandoned_question_is_dropped() {
        let mut career = Career::Asking {
            id: 4,
            puuid: "b".to_owned(),
        };
        let mut seen = HashMap::new();
        career.answered(3, Ok(serde_json::json!({ "puuid": "a" })), &mut seen);
        assert!(
            matches!(career, Career::Asking { id: 4, .. }),
            "an old answer landed on the current question"
        );
        career.answered(4, Ok(serde_json::json!({ "puuid": "b" })), &mut seen);
        assert!(matches!(career, Career::Have { .. }));
        assert!(seen.contains_key("b"), "an answer was not remembered");
        assert!(
            !seen.contains_key("a"),
            "an abandoned answer was remembered"
        );

        // Once there is an answer, nothing replaces it.
        career.answered(4, Err("no".to_owned()), &mut seen);
        assert!(matches!(career, Career::Have { .. }));
    }

    /// A question in flight when the socket dropped will never be answered.
    #[test]
    fn a_reconnect_forgets_what_it_was_waiting_for() {
        let mut career = Career::Asking {
            id: 1,
            puuid: "a".to_owned(),
        };
        career.reconnected();
        assert!(matches!(career, Career::Nothing));

        // An answer already in hand survives, so a history somebody is
        // reading does not go blank.
        let mut career = Career::Have {
            puuid: "a".to_owned(),
            profile: Box::default(),
        };
        career.reconnected();
        assert!(matches!(career, Career::Have { .. }));
    }
}
