//! One player, as one slab.
//!
//! An enemy row is a broadcast lower third: the killfeed crop at its head, a
//! name you can read across the room, the emblem in its own light, and the
//! K/D as the largest numeral on the board, in a colour that says how much
//! of a problem it is. An ally row is the same row at two thirds the size and
//! half the volume, because the reason this app is open is the other five.

use egui::{Align2, Color32, Rect, Response, Sense, Ui, pos2, vec2};
use overseer_core::Player;
use overseer_ui::{Face, caps_text, colour, motion, rank, shape, size, space};

use super::Side;
use super::grid::{Grid, Placed};
use super::paint::{self, Bracket};

/// The measurements of one kind of row.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Metrics {
    /// The slab's height.
    pub(crate) height: f32,
    /// The name, in points.
    pub(crate) name: f32,
    /// The emblem's side.
    pub(crate) emblem: f32,
    /// The tier's name beside it.
    pub(crate) tier: f32,
    /// The K/D numeral.
    pub(crate) kd: f32,
    /// Every other statistic.
    pub(crate) stat: f32,
    /// One result pip's width.
    pub(crate) pip: f32,
    /// Whether there is room for a second line under the name, for the agent.
    pub(crate) agent_line: bool,
}

impl Metrics {
    /// The crop is exactly as tall as the slab and twice as wide: Riot cut
    /// it two by one, and a face stretched to fit a box is not a face.
    pub(crate) fn crop(self) -> f32 {
        self.height * 2.0
    }
}

/// An enemy, in the window.
pub(crate) const ENEMY: Metrics = Metrics {
    height: 56.0,
    name: 21.0,
    emblem: 36.0,
    tier: 16.0,
    kd: 32.0,
    stat: 17.0,
    pip: 9.0,
    agent_line: true,
};

/// An ally, in the window: the same row, quieter.
pub(crate) const ALLY: Metrics = Metrics {
    height: 36.0,
    name: 16.0,
    emblem: 24.0,
    tier: 13.0,
    kd: 21.0,
    stat: 14.0,
    pip: 7.0,
    agent_line: false,
};

/// An enemy, in the overlay: smaller, because it sits over a game.
pub(crate) const OVERLAY: Metrics = Metrics {
    height: 44.0,
    name: 18.0,
    emblem: 28.0,
    tier: 14.0,
    kd: 26.0,
    stat: 15.0,
    pip: 8.0,
    agent_line: true,
};

/// How tall the line under a flagged row is, for its reasons.
pub(crate) const REASON: f32 = 22.0;

/// How tall a row is for an account the backend could not see.
const UNSEEN: f32 = 26.0;

/// The gap between two slabs.
pub(crate) const GAP: f32 = space::SM;

/// Everything about a row that is not the player.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Look {
    /// Whose side it is on.
    pub(crate) side: Side,
    /// Its measurements.
    pub(crate) metrics: Metrics,
    /// Whether it is the one chosen.
    pub(crate) selected: bool,
    /// Whether you have written about them.
    pub(crate) noted: bool,
    /// Its share of a party bracket.
    pub(crate) bracket: Option<Bracket>,
    /// How far the numerals have counted up, from nothing to all of it.
    pub(crate) counted: f32,
    /// The efficient tier: no motion, no blur.
    pub(crate) still: bool,
    /// Whether the reasons for a flag go on the row. The overlay has no
    /// room for a second line under five rows.
    pub(crate) reasons: bool,
}

/// Draws one row, and hands back what the pointer did to it.
///
/// The whole row is reserved before anything asks whether it is on screen:
/// a row off screen still takes its room, the reasons line included, or the
/// board would change height as it scrolled and everything under a flagged
/// row would jump.
pub(crate) fn row(ui: &mut Ui, player: &Player, grid: &Grid, look: &Look) -> Response {
    let unseen = player.name.is_none() && player.agent.is_none() && player.rank.is_none();
    let reasons = look.reasons && player.smurf && !player.smurf_reasons.is_empty();
    let height = if unseen {
        UNSEEN
    } else {
        look.metrics.height + if reasons { REASON } else { 0.0 }
    };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    ui.add_space(GAP);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter_at(rect.expand2(vec2(space::XL, space::MD)));
    // Keyed by the account rather than by the slot, so the tint follows a
    // player when a sort moves them rather than staying with the place.
    let key = response.id.with(player.puuid.as_deref().unwrap_or(""));
    let lift = motion::eased(ui.ctx().animate_bool_with_time(
        key,
        response.hovered(),
        if look.still { 0.0 } else { motion::INSTANT },
    ));
    let fill = if look.selected {
        colour::BG_SELECTED
    } else if player.smurf {
        shape::blend(colour::BG_RAISED, colour::WARN, 0.07)
    } else {
        colour::BG_RAISED
    };
    let fill = shape::blend(fill, colour::BG_HOVER, lift * 0.8);
    paint::slab(&painter, rect, fill, lift, look.still);
    if player.smurf {
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(4.0, rect.height())),
            0,
            colour::WARN,
        );
    }
    if let Some(bracket) = look.bracket {
        paint::bracket(&painter, bracket, rect.left() - 10.0, rect);
    }
    if player.stack_guess.is_some() && player.party.is_none() && look.bracket.is_none() {
        let guess = Bracket {
            tint: colour::TEXT_FAINT,
            top: true,
            bottom: true,
            guessed: true,
        };
        paint::bracket(&painter, guess, rect.left() - 10.0, rect);
    }
    let line = Rect::from_min_size(rect.min, vec2(rect.width(), look.metrics.height));
    // An account the backend could not see at all. Ten columns of dashes
    // reads as the app being broken, and a full slab spent saying nothing
    // is a slab taken from somebody who is there; one short quiet line reads
    // as the truth.
    if unseen {
        painter.text(
            pos2(line.left() + space::LG, rect.center().y),
            Align2::LEFT_CENTER,
            "Riot did not say who this is",
            Face::Body.at(size::MICRO + 1.0),
            colour::TEXT_FAINT,
        );
        return response;
    }
    identity(&painter, player, line, look);
    for placed in &grid.placed {
        cell(&painter, player, line, *placed, look);
    }
    if look.noted {
        paint::dog_ear(&painter, rect);
    }
    if reasons {
        why(&painter, player, rect, look.metrics.crop() + space::LG);
    }
    response
}

/// The face and the name.
fn identity(painter: &egui::Painter, player: &Player, line: Rect, look: &Look) {
    let m = look.metrics;
    let crop = Rect::from_min_size(
        pos2(
            line.left() + if player.smurf { 4.0 } else { 0.0 },
            line.top(),
        ),
        vec2(m.crop(), m.height),
    );
    paint::crop(painter, player, crop);
    let x = crop.right() + space::LG;
    let (name, tag) = split_name(player.display_name());
    let name_colour = if player.is_self {
        colour::YOU
    } else if look.side == Side::Enemy {
        colour::TEXT_STRONG
    } else {
        colour::TEXT
    };
    let middle = if m.agent_line {
        line.center().y - m.name * 0.32
    } else {
        line.center().y
    };
    let limit = line.left() + look.metrics.crop() + 200.0;
    let drawn = caps_text(
        painter,
        pos2(x, middle),
        Align2::LEFT_CENTER,
        &clip(name, 14),
        Face::Display.at(m.name),
        name_colour,
    );
    if !tag.is_empty() && drawn.right() + 40.0 < limit {
        painter.text(
            pos2(drawn.right() + space::SM, middle + 1.0),
            Align2::LEFT_CENTER,
            format!("#{tag}"),
            Face::Number.at(m.name * 0.62),
            colour::TEXT_FAINT,
        );
    }
    if m.agent_line
        && let Some(agent) = player.agent.as_deref()
    {
        let _drawn = caps_text(
            painter,
            pos2(x, middle + m.name * 0.9),
            Align2::LEFT_CENTER,
            agent,
            Face::Number.at(size::MICRO + 1.0),
            colour::TEXT_FAINT,
        );
    }
}

/// One statistic in its column.
fn cell(painter: &egui::Painter, player: &Player, line: Rect, placed: Placed, look: &Look) {
    let m = look.metrics;
    let left = line.left() + placed.left;
    let right = line.left() + placed.right;
    let y = line.center().y;
    let side = look.side;
    let stat = |text: String, tint: Color32| {
        painter.text(
            pos2(right, y),
            Align2::RIGHT_CENTER,
            text,
            Face::Display.at(m.stat),
            tint,
        );
    };
    match placed.column.head {
        "rank" if right - left < 100.0 => {
            let tier = player.rank_tier.unwrap_or(0);
            if tier >= 3 {
                paint::emblem(
                    painter,
                    tier,
                    pos2(left + (right - left) / 2.0, y),
                    m.emblem,
                    if look.still { 0.0 } else { 1.0 },
                );
            }
        }
        "rank" => rank_cell(painter, player, pos2(left, y), look),
        "k/d" => {
            let Some(kd) = player.kd else {
                return stat("-".to_owned(), colour::TEXT_FAINT);
            };
            let shown = kd * f64::from(look.counted);
            let _landed = paint::numeral(
                painter,
                &format!("{shown:.2}"),
                pos2(right, y + 1.0),
                &Face::Heavy.at(m.kd),
                heat(side, (kd as f32 - 0.9) / 0.8),
            );
        }
        "hs" => match player.hs_pct {
            Some(v) => stat(format!("{v:.0}%"), heat(side, (v as f32 - 16.0) / 16.0)),
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "win" => match player.win_rate {
            Some(v) => stat(format!("{v:.0}%"), heat(side, (v as f32 - 46.0) / 14.0)),
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "lvl" => match player.level {
            // A low level on a ranked account is the first smurf tell, so it
            // is the one level worth a colour.
            Some(l) if player.level_hidden => stat(format!("{l}?"), colour::TEXT_FAINT),
            Some(l) if l < 60 && player.rank_tier.unwrap_or(0) >= 3 => {
                stat(l.to_string(), colour::WARN);
            }
            Some(l) => stat(l.to_string(), colour::TEXT_DIM),
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "met" => match player.met() {
            0 => {}
            n => stat(format!("{n}x"), colour::INFO),
        },
        "map" => match player.map_win_rate.as_ref().and_then(|m| m.win_rate) {
            Some(v) => stat(format!("{v:.0}%"), heat(side, (v as f32 - 46.0) / 14.0)),
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "rr" => match player.rr {
            Some(rr) => stat(rr.to_string(), colour::TEXT_DIM),
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "last 5" => {
            let win = if side == Side::Enemy {
                colour::ENEMY
            } else {
                colour::ALLY
            };
            paint::pips(painter, &player.form, pos2(left, y), m.pip, win);
        }
        _ => {}
    }
}

/// How hot a number is for the side it belongs to.
///
/// On the enemy's side a good number is bad news and runs to the enemy's
/// red; on yours it runs to your green. `t` is nothing to worry about at 0
/// and as good as it gets at 1.
fn heat(side: Side, t: f32) -> Color32 {
    match side {
        Side::Enemy => colour::threat(t),
        Side::Ally => colour::strength(t),
    }
}

/// The emblem, the tier, and the peak under it when the peak is the more
/// interesting fact.
fn rank_cell(painter: &egui::Painter, player: &Player, at: egui::Pos2, look: &Look) {
    let m = look.metrics;
    let tier = player.rank_tier.unwrap_or(0);
    let Some(name) = player.rank.as_deref().filter(|n| !n.is_empty()) else {
        painter.text(
            at,
            Align2::LEFT_CENTER,
            "-",
            Face::Display.at(m.tier),
            colour::TEXT_FAINT,
        );
        return;
    };
    let tint = rank(player.rank_tier);
    if tier >= 3 {
        paint::emblem(
            painter,
            tier,
            pos2(at.x + m.emblem / 2.0, at.y),
            m.emblem,
            if look.still { 0.0 } else { 1.0 },
        );
    }
    let x = at.x + if tier >= 3 { m.emblem + space::SM } else { 0.0 };
    let peak = peak_line(player, tier).filter(|_| m.agent_line);
    let y = if peak.is_some() {
        at.y - m.tier * 0.4
    } else {
        at.y
    };
    let _drawn = caps_text(
        painter,
        pos2(x, y),
        Align2::LEFT_CENTER,
        name,
        Face::Display.at(m.tier),
        if tier >= 3 { tint } else { colour::TEXT_DIM },
    );
    if let Some(peak) = peak {
        let _drawn = caps_text(
            painter,
            pos2(x, y + m.tier * 0.95),
            Align2::LEFT_CENTER,
            &peak,
            Face::Number.at(size::MICRO + 1.0),
            colour::TEXT_FAINT,
        );
    }
}

/// "Peak Ascendant 2", when the peak is two ranks or more above today.
///
/// Two whole ranks, because one is ordinary: half a lobby is a rank off its
/// best at any time, and saying so on every row is saying nothing.
fn peak_line(player: &Player, tier: u32) -> Option<String> {
    let peak = player.peak_rank_tier?;
    let name = player.peak_rank.as_deref()?;
    (peak >= tier + 6 || (tier < 3 && peak >= 3)).then(|| format!("peak {name}"))
}

/// Why this account is worth a look, on the line under its row.
///
/// Set in the reading face, cream with the numbers in amber, because this is
/// the one sentence on the board that has to be read rather than scanned.
fn why(painter: &egui::Painter, player: &Player, rect: Rect, indent: f32) {
    let y = rect.bottom() - REASON / 2.0 - 2.0;
    let left = rect.left() + indent;
    let mut job = egui::text::LayoutJob::default();
    for (i, reason) in player.smurf_reasons.iter().enumerate() {
        if i > 0 {
            job.append(
                "   ",
                0.0,
                egui::TextFormat::simple(Face::Body.at(size::MICRO + 2.0), colour::TEXT_FAINT),
            );
        }
        for piece in paint::split_numbers(reason) {
            let tint = if piece.1 {
                colour::WARN
            } else {
                colour::TEXT_STRONG
            };
            job.append(
                piece.0,
                0.0,
                egui::TextFormat::simple(Face::Body.at(size::MICRO + 2.0), tint),
            );
        }
    }
    job.wrap = egui::text::TextWrapping {
        max_width: rect.right() - space::LG - left,
        max_rows: 1,
        break_anywhere: false,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(left, y - galley.size().y / 2.0),
        galley,
        colour::TEXT_STRONG,
    );
}

/// A Riot id split at its tag.
fn split_name(full: &str) -> (&str, &str) {
    full.split_once('#').unwrap_or((full, ""))
}

/// A name cut to a length a caps name can hold in its column, with an
/// ellipsis if it had to be.
fn clip(name: &str, keep: usize) -> String {
    if name.chars().count() <= keep {
        return name.to_owned();
    }
    let mut out: String = name.chars().take(keep.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::{clip, peak_line};
    use crate::board::paint::split_numbers;
    use overseer_core::Player;

    /// The numbers in a reason are found, decimals and percentages whole.
    #[test]
    fn a_reason_splits_at_its_numbers() {
        let runs = split_numbers("Lvl 34, K/D 1.92, 64% win");
        let numbers: Vec<&str> = runs.iter().filter(|r| r.1).map(|r| r.0).collect();
        assert_eq!(numbers, ["34", "1.92", "64%"]);
        let whole: String = runs.iter().map(|r| r.0).collect();
        assert_eq!(whole, "Lvl 34, K/D 1.92, 64% win");
    }

    /// A long name is cut with an ellipsis, a short one left alone.
    #[test]
    fn a_long_name_is_cut_at_its_length() {
        assert_eq!(clip("Day", 14), "Day");
        assert_eq!(clip("AVeryLongNameIndeed", 8), "AVeryLo\u{2026}");
    }

    /// The peak is only mentioned when it is two ranks or more above.
    #[test]
    fn a_peak_one_rank_up_is_not_news() {
        let player = |tier, peak| Player {
            rank_tier: Some(tier),
            peak_rank_tier: Some(peak),
            peak_rank: Some("Peak".to_owned()),
            ..Player::default()
        };
        assert!(peak_line(&player(14, 17), 14).is_none());
        assert!(peak_line(&player(14, 20), 14).is_some());
    }
}
