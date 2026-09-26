//! The bottom half of the panel: what one account has been doing lately.
//!
//! The board is pushed once a second and has to stay small, so none of this
//! is in it. It is asked for when somebody is selected and it answers the
//! questions the board raises rather than repeating them: the rank column
//! says Diamond 3, and this says whether they arrived there last week on the
//! way up or last month on the way down.
//!
//! Two charts, because two questions here are about shape rather than value.
//! A rating is a line, since what matters is which way it is going. A K/D per
//! match is bars against even, since what matters is how often they are the
//! problem. Everything else is a number, and a number that would be a chart
//! of three points is a number.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use overseer_core::{Bridge, CareerMatch, Profile};

use crate::panel::{heading, line, stat};
use overseer_ui::{Face, colour, kd, size, space};

/// How tall a chart is. Enough for a shape to be a shape, not so much that
/// the numbers under it fall off the bottom of the panel.
const CHART: f32 = 56.0;
/// How many matches to list. The history is eight or ten; a list of all of
/// them pushes everything under it out of reach for the two at the bottom
/// nobody scrolls to.
const LISTED: usize = 6;

/// Where the request for one account's history has got to.
///
/// A state rather than an `Option<Profile>`, because "asked and waiting" and
/// "asked and told no" both need saying out loud. A panel that is blank while
/// it waits and blank when it failed is a panel nobody trusts.
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
        /// Boxed because this is much larger than the other variants and the
        /// whole enum would otherwise be that size everywhere it is stored.
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

    /// Asks for whoever is selected now, if that is not who this is about.
    ///
    /// Only while the socket is up. A question asked into a dead socket is
    /// queued and sent on reconnect, by which time nobody wants the answer.
    pub(crate) fn follow(&mut self, bridge: &Bridge, selected: Option<&str>, live: bool) {
        let Some(puuid) = selected else {
            *self = Self::Nothing;
            return;
        };
        if !live || self.about() == Some(puuid) {
            return;
        }
        let id = bridge.ask("profile", serde_json::json!({ "puuid": puuid }));
        *self = Self::Asking {
            id,
            puuid: puuid.to_owned(),
        };
    }

    /// Forgets a question that was in flight when the socket went.
    ///
    /// Nothing will ever answer it, and without this the panel would wait for
    /// that answer for as long as the same player stayed selected.
    pub(crate) fn reconnected(&mut self) {
        if matches!(self, Self::Asking { .. }) {
            *self = Self::Nothing;
        }
    }

    /// Takes an answer, if it is the one being waited on.
    ///
    /// Anything else is an answer to a question abandoned when the selection
    /// moved on, and acting on it would put one player's history under
    /// another player's name.
    pub(crate) fn answered(&mut self, id: u64, result: Result<serde_json::Value, String>) {
        let Self::Asking { id: waiting, puuid } = self else {
            return;
        };
        if *waiting != id {
            return;
        }
        let puuid = puuid.clone();
        *self = match result {
            Err(why) => Self::Refused { puuid, why },
            Ok(value) => match serde_json::from_value::<Profile>(value) {
                Ok(profile) => Self::Have {
                    puuid,
                    profile: Box::new(profile),
                },
                Err(e) => Self::Refused {
                    puuid,
                    why: format!("the history did not read: {e}"),
                },
            },
        };
    }
}

/// Draws whatever there is for this account.
pub(crate) fn show(ui: &mut Ui, career: &Career, puuid: &str) {
    match career {
        Career::Asking { puuid: who, .. } if who == puuid => {
            heading(ui, "career");
            line(
                ui,
                "Reading their last matches.",
                colour::TEXT_FAINT,
                size::MICRO,
            );
        }
        Career::Refused { puuid: who, why } if who == puuid => {
            heading(ui, "career");
            line(ui, why, colour::TEXT_DIM, size::MICRO);
        }
        Career::Have {
            puuid: who,
            profile,
        } if who == puuid => history(ui, profile),
        _ => {}
    }
}

/// Everything a full history has to say, in the order it is worth reading.
fn history(ui: &mut Ui, profile: &Profile) {
    if profile.matches.is_empty() {
        heading(ui, "career");
        line(ui, "No matches on record.", colour::TEXT_DIM, size::MICRO);
        return;
    }
    averages(ui, profile);
    rating(ui, profile);
    per_match_kd(ui, profile);
    guns(ui, profile);
    matches(ui, profile);
    together(ui, profile);
    habits(ui, profile);
}

/// The whole history in four numbers, with the sample size beside them.
fn averages(ui: &mut Ui, profile: &Profile) {
    let a = &profile.averages;
    let games = a.games.unwrap_or(profile.matches.len() as u32);
    heading(ui, "career");
    let over = format!("over {games}");
    stat(
        ui,
        "k/d",
        &a.kd.map_or_else(dash, |v| format!("{v:.2}")),
        kd(a.kd),
        &over,
    );
    stat(
        ui,
        "win",
        &a.win_rate.map_or_else(dash, |v| format!("{v:.0}%")),
        colour::TEXT,
        &a.wins.map_or_else(String::new, |w| format!("{w} won")),
    );
    stat(
        ui,
        "hs",
        &a.hs_pct.map_or_else(dash, |v| format!("{v:.0}%")),
        colour::TEXT,
        "",
    );
    stat(
        ui,
        "avg",
        &format!(
            "{:.1} / {:.1} / {:.1}",
            a.kills.unwrap_or(0.0),
            a.deaths.unwrap_or(0.0),
            a.assists.unwrap_or(0.0)
        ),
        colour::TEXT_DIM,
        "k d a",
    );
}

/// Where their rating has been going, as a line.
///
/// Plotted against a ladder position rather than the rating itself, because
/// rating restarts at zero in every tier and a raw line drops off a cliff on
/// the promotion it should be celebrating.
fn rating(ui: &mut Ui, profile: &Profile) {
    let run = profile.rating_run();
    if run.len() < 2 {
        return;
    }
    heading(ui, "rating");
    let points: Vec<f32> = run.iter().map(|m| ladder(m)).collect();
    plot(ui, &points, None);

    let first = run.first().and_then(|m| m.rank_after.clone());
    let last = run.last().and_then(|m| m.rank_after.clone());
    let moved = points
        .last()
        .zip(points.first())
        .map_or(0.0, |(end, start)| end - start);
    let tint = if moved > 0.0 {
        colour::GOOD
    } else if moved < 0.0 {
        colour::BAD
    } else {
        colour::TEXT_DIM
    };
    let words = match (first, last) {
        (Some(a), Some(b)) if a != b => format!("{a} to {b}, {moved:+.0} over {}", run.len()),
        (_, Some(b)) => format!("{b}, {moved:+.0} over {}", run.len()),
        _ => format!("{moved:+.0} over {}", run.len()),
    };
    line(ui, &words, tint, size::MICRO);
}

/// A ladder position that does not restart every tier.
fn ladder(m: &CareerMatch) -> f32 {
    let tier = f64::from(m.tier_after.unwrap_or(0)) * 100.0;
    let rr = f64::from(m.rr_after.unwrap_or(0));
    (tier + rr) as f32
}

/// How often they were the problem, as bars against an even ratio.
fn per_match_kd(ui: &mut Ui, profile: &Profile) {
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
    heading(ui, "k/d per match");
    plot(ui, &values, Some(1.0));
    line(
        ui,
        "oldest left, even is the rule",
        colour::TEXT_FAINT,
        size::MICRO,
    );
}

/// Which guns did the killing, with a bar for the share.
fn guns(ui: &mut Ui, profile: &Profile) {
    if profile.top_guns.is_empty() {
        return;
    }
    heading(ui, "guns");
    for gun in profile.top_guns.iter().take(5) {
        let Some(name) = gun.name.as_deref() else {
            continue;
        };
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter().clone();
        let share = f32::from(u16::try_from(gun.share.unwrap_or(0).min(100)).unwrap_or(0));
        // The bar sits behind the name rather than beside it, so five guns
        // read as one shape rather than as five rows of furniture.
        let full = rect.width() - space::LG * 2.0;
        let bar = Rect::from_min_size(
            pos2(rect.left() + space::LG, rect.center().y - 7.0),
            vec2(full * share / 100.0, 14.0),
        );
        painter.rect_filled(bar, 0, colour::BG_HOVER);
        painter.text(
            pos2(rect.left() + space::LG + space::SM, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            Face::Body.at(size::MICRO),
            colour::TEXT,
        );
        painter.text(
            pos2(rect.right() - space::LG, rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{}%  {} kills", share as u32, gun.kills.unwrap_or(0)),
            Face::Number.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

/// The last few matches, one line each.
fn matches(ui: &mut Ui, profile: &Profile) {
    heading(ui, "last matches");
    for m in profile.matches.iter().take(LISTED) {
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::ROW_TIGHT), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter().clone();
        let won = m.won();
        let tint = if won { colour::ALLY } else { colour::ENEMY };
        // A rail rather than a word: the result is the first thing the eye
        // needs and the last thing worth spending space on.
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
            m.agent.as_deref().unwrap_or(""),
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
                    colour::GOOD
                } else {
                    colour::BAD
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
    let named: Vec<&overseer_core::CoPlayer> = profile
        .co_players
        .iter()
        .filter(|c| c.name.is_some())
        .take(4)
        .collect();
    if named.is_empty() {
        return;
    }
    heading(ui, "played with");
    for mate in named {
        let shared = mate.shared_matches.unwrap_or(0);
        stat(
            ui,
            if mate.is_party { "duo" } else { "also" },
            mate.name.as_deref().unwrap_or("-"),
            if mate.is_party {
                colour::WARN
            } else {
                colour::TEXT
            },
            &format!("{shared} together"),
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
    heading(ui, "habits");
    if let (Some(pct), Some(chances)) = (force.pct, force.chances) {
        stat(
            ui,
            "force",
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
            "bonus",
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

/// Draws a series, as a line or as bars around a rule.
///
/// One function for both because they are the same picture: points across a
/// box, scaled to fit. Passing a baseline turns it into bars measured from
/// that value, which is what a ratio wants and what a rating does not.
fn plot(ui: &mut Ui, values: &[f32], baseline: Option<f32>) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), CHART), Sense::hover());
    if !ui.is_rect_visible(rect) || values.len() < 2 {
        return;
    }
    let inner = Rect::from_min_max(
        pos2(rect.left() + space::LG, rect.top() + space::SM),
        pos2(rect.right() - space::LG, rect.bottom() - space::SM),
    );
    let (low, high) = bounds(values, baseline);
    let n = values.len() as f32;
    let height_of = |v: f32| inner.bottom() - inner.height() * (v - low) / (high - low);
    let painter = ui.painter().clone();

    if let Some(rule) = baseline {
        // Bars stand in slots, on the centre of each. Sharing the line's
        // endpoints would put half of the first and half of the last outside
        // the box, which is exactly what it looks like: clipped.
        let slot = inner.width() / n;
        let y = height_of(rule);
        painter.hline(inner.x_range(), y, (1.0, colour::LINE));
        for (i, &v) in values.iter().enumerate() {
            let middle = inner.left() + slot * (i as f32 + 0.5);
            let top = height_of(v);
            let half = slot * 0.275;
            let bar = Rect::from_min_max(
                pos2(middle - half, top.min(y)),
                pos2(middle + half, top.max(y)),
            );
            let tint = if v >= rule { colour::GOOD } else { colour::BAD };
            painter.rect_filled(bar, 0, tint);
        }
        return;
    }
    let at = |i: usize, v: f32| {
        pos2(
            inner.left() + inner.width() * (i as f32) / (n - 1.0),
            height_of(v),
        )
    };

    let points: Vec<egui::Pos2> = values.iter().enumerate().map(|(i, &v)| at(i, v)).collect();
    // The area under the line, fading to nothing at the floor. A bare
    // polyline is a diagram; the same line with weight under it is a
    // quantity, and which of the two this is happens to be the question.
    let mut area = egui::Mesh::default();
    for point in &points {
        area.colored_vertex(*point, colour::INFO.gamma_multiply(0.22));
        area.colored_vertex(pos2(point.x, inner.bottom()), egui::Color32::TRANSPARENT);
    }
    for i in 0..points.len().saturating_sub(1) {
        let a = (i * 2) as u32;
        area.add_triangle(a, a + 1, a + 2);
        area.add_triangle(a + 1, a + 3, a + 2);
    }
    painter.add(egui::Shape::mesh(area));
    painter.add(egui::Shape::line(
        points.clone(),
        egui::Stroke::new(2.0, colour::INFO),
    ));
    // Only the ends get a dot. A dot on every point turns a shape into a
    // row of beads, and the two that matter are where it started and where
    // it got to.
    for point in [points.first(), points.last()].into_iter().flatten() {
        painter.circle_filled(*point, 2.5, colour::INFO);
    }
}

/// The range to draw over, never zero wide.
///
/// A flat series would otherwise divide by nothing and put every point on the
/// same pixel row, or off the top of the box. Padding it puts the flat line
/// through the middle, which is what a flat line means.
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

/// What a missing value looks like.
fn dash() -> String {
    "-".to_owned()
}

#[cfg(test)]
mod tests {
    use super::{Career, bounds, ladder};
    use overseer_core::CareerMatch;

    /// A rating that crosses a tier has to keep going up.
    ///
    /// Rating restarts at zero on promotion, so plotting it raw draws a
    /// crash at the moment somebody ranked up. The ladder position is what
    /// the chart is actually about.
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

    /// A flat series still has a box to draw in.
    #[test]
    fn a_flat_series_does_not_divide_by_nothing() {
        let (low, high) = bounds(&[1.0, 1.0, 1.0], None);
        assert!(high > low, "a flat line had nowhere to be drawn");
        let (low, high) = bounds(&[0.9, 1.4], Some(1.0));
        assert!(low < 0.9 && high > 1.4, "the baseline fell outside the box");
        assert!(bounds(&[], None).1 > bounds(&[], None).0);
    }

    /// An answer to a question nobody is waiting for is thrown away.
    ///
    /// Ids are never reused, so a late answer belongs to a selection that has
    /// moved on. Taking it would put one player's history under another
    /// player's name, which is the worst thing this panel could do.
    #[test]
    fn a_late_answer_to_an_abandoned_question_is_dropped() {
        let mut career = Career::Asking {
            id: 4,
            puuid: "b".to_owned(),
        };
        career.answered(3, Ok(serde_json::json!({ "puuid": "a" })));
        assert!(
            matches!(career, Career::Asking { id: 4, .. }),
            "an old answer landed on the current question"
        );
        career.answered(4, Ok(serde_json::json!({ "puuid": "b" })));
        assert!(matches!(career, Career::Have { .. }));

        // And nothing lands at all once there is an answer.
        career.answered(4, Err("no".to_owned()));
        assert!(matches!(career, Career::Have { .. }));
    }

    /// A question in flight when the socket went will never be answered.
    #[test]
    fn a_reconnect_forgets_what_it_was_waiting_for() {
        let mut career = Career::Asking {
            id: 1,
            puuid: "a".to_owned(),
        };
        career.reconnected();
        assert!(matches!(career, Career::Nothing));

        // An answer already in hand is still good: the socket coming back
        // does not make a history somebody is reading go blank.
        let mut career = Career::Have {
            puuid: "a".to_owned(),
            profile: Box::default(),
        };
        career.reconnected();
        assert!(matches!(career, Career::Have { .. }));
    }
}
