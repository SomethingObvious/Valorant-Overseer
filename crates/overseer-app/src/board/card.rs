//! The overlay: the match in a strip, the enemy as cards and your team as
//! lines. Every card has the same zones at the same x, the face and the
//! name, then the rank, the K/D and the rest, so a number sits under the one
//! above it and reads without a heading. Your team's lines use the same
//! zones, one line high.

use egui::{Align2, Color32, Rect, Sense, Ui, pos2, vec2};
use overseer_core::{Board, Player};
use overseer_ui::{Face, caps_text, colour, motion, shape, size, space};

use super::rows::{self, Look, Metrics};
use super::{GUTTER, ITEM, Scene, Side, Touched, heads, paint};

/// An enemy's card: two lines, the face as tall as both.
const CARD: Metrics = Metrics {
    height: 48.0,
    name: 17.0,
    emblem: 28.0,
    tier: 14.0,
    kd: 25.0,
    stat: 13.0,
    pip: 8.0,
    agent_line: true,
    tags_first: true,
};

/// One of your team: one line, quieter.
const LINE: Metrics = Metrics {
    height: 30.0,
    name: 14.0,
    emblem: 20.0,
    tier: 12.0,
    kd: 17.0,
    stat: 12.0,
    pip: 6.0,
    agent_line: false,
    tags_first: false,
};

/// The rank's zone: the emblem, the tier and what sits under it.
const RANK: f32 = 128.0;
/// The K/D's zone, its numeral set right.
const KD: f32 = 60.0;
/// The last zone: the results and the level, the rates under them.
const MORE: f32 = 172.0;
/// The space between two zones.
const SPACE: f32 = 14.0;
/// The room at a card's right edge.
const TAIL: f32 = 12.0;
/// The match strip's height.
const STRIP: f32 = 26.0;
/// Where the middle of the match strip is, down from the overlay's gutter,
/// for the hide button that sits at its right end.
pub(crate) const STRIP_MIDDLE: f32 = STRIP / 2.0;
/// The room the hide button takes at the strip's right end.
const HIDE_ROOM: f32 = 32.0;
/// How far a block sits under the one above it.
const BETWEEN: f32 = 14.0;

/// How tall the overlay is with a full match: the strip, both plates, five
/// cards and five lines, and the gutter round them. egui's spacing under
/// every widget counts too, or a bottom corner is placed for a shorter
/// window than it draws.
pub(crate) const HEIGHT: f32 = GUTTER
    + STRIP
    + ITEM
    + BETWEEN
    + heads::PLATE
    + ITEM
    + space::SM
    + (CARD.height + ITEM + rows::GAP) * 5.0
    + GUTTER;

/// Where the zones of a card or a line sit across it.
#[derive(Debug, Clone, Copy)]
struct Zones {
    /// Where the name has to stop.
    name: f32,
    /// The rank zone's left edge.
    rank: f32,
    /// The K/D's right edge.
    kd: f32,
    /// The last zone's left and right edges.
    more: (f32, f32),
}

impl Zones {
    /// The zones of a row spanning `rect`, measured from its right edge so
    /// the name gets whatever is left.
    fn of(rect: Rect) -> Self {
        let right = rect.right() - TAIL;
        let more = (right - MORE, right);
        let kd = more.0 - SPACE;
        let rank = kd - KD - SPACE - RANK;
        Self {
            name: rank - SPACE,
            rank,
            kd,
            more,
        }
    }
}

/// Draws the overlay inside its gutters, and says how tall it came out.
pub(crate) fn overlay(ui: &mut Ui, scene: &Scene<'_>, order: &[(Side, String); 2]) -> Touched {
    let top = ui.cursor().top();
    ui.add_space(GUTTER);
    strip(ui, scene.board);
    let enemies = scene
        .board
        .players
        .iter()
        .any(|p| super::side_of(scene.board, p) == Side::Enemy);
    for (side, team) in order {
        let players = super::roster(scene.board, team, scene.sort, "");
        // Your own team can be seen in the game, so once the other one is
        // known the overlay is just them, and half the height.
        if players.is_empty() || (enemies && *side != Side::Enemy) {
            continue;
        }
        ui.add_space(BETWEEN);
        let _clicked = heads::team(ui, scene.board, *side, team);
        // In agent select only your team is known, and it gets the cards
        // the enemy would, tags and all.
        let metrics = if *side == Side::Enemy || !enemies {
            CARD
        } else {
            LINE
        };
        let mut placed = Vec::with_capacity(players.len());
        for player in &players {
            let look = look(scene, player, *side, metrics);
            placed.push(row(ui, player, &look));
        }
        let members: Vec<&Player> = scene
            .board
            .players
            .iter()
            .filter(|p| super::on(p, team))
            .collect();
        let tints = paint::party_tints(&members, *side);
        for party in paint::spines(&players, &tints) {
            if let (Some(first), Some(last)) = (placed.get(party.rows.0), placed.get(party.rows.1))
            {
                paint::spine(ui.painter(), &party, first.union(*last));
            }
        }
    }
    ui.add_space(GUTTER);
    Touched {
        drew: ui.cursor().top() - top,
        ..Touched::default()
    }
}

/// How one row is drawn, from the scene and the player.
fn look<'a>(scene: &Scene<'a>, player: &'a Player, side: Side, metrics: Metrics) -> Look<'a> {
    let id = player.puuid.as_deref();
    Look {
        side,
        metrics,
        selected: false,
        noted: id.is_some_and(|id| scene.notes.has(id)),
        tag: id.and_then(|id| scene.notes.tag(id)),
        note: id.and_then(|id| scene.notes.text(id)),
        counted: if scene.still {
            1.0
        } else {
            motion::eased(scene.since / super::COUNT)
        },
        still: scene.still,
    }
}

/// The match: the map, the side and the queue on the left, the score and
/// the round on the right, and the key that hides it between them. The
/// overlay takes no clicks, so that key is the only way it goes away.
fn strip(ui: &mut Ui, board: &Board) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), STRIP), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let bottom = rect.bottom() - 2.0;
    let mut x = rect.left();
    if let Some(map) = board.map.as_deref() {
        let drawn = caps_text(
            painter,
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            map,
            Face::Heavy.at(19.0),
            colour::TEXT_STRONG,
        );
        x = drawn.right() + space::LG;
    }
    if let Some(side) = board.side.as_deref() {
        x = crate::header::plate(painter, x, bottom, side, colour::TEXT_STRONG, true).right()
            + space::SM;
    }
    if let Some(mode) = board.mode.as_deref() {
        x = crate::header::plate(painter, x, bottom, mode, colour::TEXT_DIM, false).right();
    }
    let y = rect.center().y;
    let mut right = rect.right() - HIDE_ROOM;
    if let Some(score) = board.score.as_ref() {
        let font = Face::Heavy.at(20.0);
        for (text, tint) in [
            (score.enemy.map(|n| n.to_string()), colour::ENEMY),
            (Some("-".to_owned()), colour::TEXT_FAINT),
            (score.ally.map(|n| n.to_string()), colour::ALLY),
        ] {
            let Some(text) = text else { continue };
            right = paint::numeral(painter, &text, pos2(right, y), &font, tint).left() - space::SM;
        }
        if let Some(round) = score.round {
            right = caps_text(
                painter,
                pos2(right - space::SM, y + 1.0),
                Align2::RIGHT_CENTER,
                &format!("Round {round}"),
                paint::label(),
                colour::TEXT_DIM,
            )
            .left()
                - space::XL;
        }
    }
    if let Some(chance) = board.win_prob {
        let tint = if chance >= 50.0 {
            colour::ALLY
        } else {
            colour::ENEMY
        };
        right = pair(
            painter,
            (right, y),
            "To Win",
            &format!("{chance:.0}%"),
            tint,
        );
    }
    let hint = format!("{} Hides This", crate::hotkey::LABEL);
    let wide = overseer_ui::caps_width(painter, &hint, paint::label());
    if x + space::XL + wide + space::XL <= right {
        let _hint = caps_text(
            painter,
            pos2(x + space::XL, y + 1.0),
            Align2::LEFT_CENTER,
            &hint,
            paint::label(),
            colour::TEXT_FAINT,
        );
    }
}

/// One player: an enemy's card or one of your team's lines.
fn row(ui: &mut Ui, player: &Player, look: &Look<'_>) -> Rect {
    let m = look.metrics;
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), m.height), Sense::hover());
    ui.add_space(rows::GAP);
    if !ui.is_rect_visible(rect) {
        return rect;
    }
    let painter = ui.painter_at(rect.expand2(vec2(space::XL, space::MD)));
    let enemy = look.side == Side::Enemy;
    let flagged = player.smurf && enemy;
    let fill = if flagged {
        shape::blend(colour::BG_RAISED, colour::WARN, 0.07)
    } else {
        colour::BG_RAISED
    };
    paint::slab(&painter, rect, fill, 0.0, look.still);
    let inside = painter.with_clip_rect(rect);
    if player.agent.is_none() {
        paint::card_behind(&inside, player, rect, (fill, m.crop()));
    }
    let zones = Zones::of(rect);
    let _hovers = rows::identity(&inside, player, (rect, zones.name), look, flagged);
    if flagged {
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(4.0, rect.height())),
            0,
            colour::WARN,
        );
    }
    if look.noted {
        paint::dog_ear(&painter, rect);
    }
    if !rows::known(player) {
        return rect;
    }
    if m.agent_line {
        card(&inside, player, rect, zones, look);
    } else {
        line(&inside, player, rect, zones, look);
    }
    rect
}

/// An enemy's zones, two lines each.
fn card(painter: &egui::Painter, player: &Player, rect: Rect, zones: Zones, look: &Look<'_>) {
    let m = look.metrics;
    let upper = rect.height().mul_add(0.34, rect.top());
    let lower = rect.height().mul_add(0.72, rect.top());
    rank(painter, player, zones.rank, (upper, lower), look);
    if let Some(kd) = player.kd {
        let few = rows::few_games(player);
        let _kd = paint::numeral(
            painter,
            &format!("{:.2}", kd * f64::from(look.counted)),
            pos2(zones.kd, upper),
            &Face::Heavy.at(m.kd),
            if few {
                colour::TEXT_FAINT
            } else {
                paint::kd_heat(look.side, Some(kd))
            },
        );
    }
    let games = player.form.len();
    if games > 0 {
        let _games = caps_text(
            painter,
            pos2(zones.kd, lower + 1.0),
            Align2::RIGHT_CENTER,
            &format!("{games} {}", if games == 1 { "Game" } else { "Games" }),
            paint::label(),
            colour::TEXT_FAINT,
        );
    }
    let (left, right) = zones.more;
    paint::pips(
        painter,
        &player.form,
        pos2(left, upper),
        m.pip,
        paint::win(look.side),
    );
    let mut end = right;
    let (level, tint) = rows::level_of(player);
    end = pair(painter, (end, upper), "Lvl", &level, tint) - space::LG;
    if player.met() > 0 {
        let _met = pair(
            painter,
            (end, upper),
            "Met",
            &format!("{}×", player.met()),
            colour::INFO,
        );
    }
    let map = player.map_win_rate.as_ref();
    let mut end = right;
    for (name, (text, tint)) in [
        (
            "Map",
            rows::rate(
                look.side,
                map.and_then(|m| m.win_rate),
                map.and_then(|m| m.games),
                rows::FEW_MAP_GAMES,
            ),
        ),
        (
            "Win",
            rows::rate(look.side, player.win_rate, player.games, rows::FEW_GAMES),
        ),
        ("HS", hs(look.side, player.hs_pct)),
    ] {
        end = pair(painter, (end, lower), name, &text, tint) - space::LG;
    }
}

/// One of your team's zones, on one line: the rank, the K/D, the results,
/// the headshots and the win rate.
fn line(painter: &egui::Painter, player: &Player, rect: Rect, zones: Zones, look: &Look<'_>) {
    let m = look.metrics;
    let y = rect.center().y;
    rank(painter, player, zones.rank, (y, y), look);
    if let Some(kd) = player.kd {
        let _kd = paint::numeral(
            painter,
            &format!("{:.2}", kd * f64::from(look.counted)),
            pos2(zones.kd, y),
            &Face::Heavy.at(m.kd),
            if rows::few_games(player) {
                colour::TEXT_FAINT
            } else {
                paint::kd_heat(look.side, Some(kd))
            },
        );
    }
    let (left, right) = zones.more;
    paint::pips(
        painter,
        &player.form,
        pos2(left, y),
        m.pip,
        paint::win(look.side),
    );
    let (text, tint) = rows::rate(look.side, player.win_rate, player.games, rows::FEW_GAMES);
    let end = pair(painter, (right, y), "Win", &text, tint) - space::LG;
    let (text, tint) = hs(look.side, player.hs_pct);
    let _hs = pair(painter, (end, y), "HS", &text, tint);
}

/// The rank: its emblem, its tier, and under it on a card the peak when it
/// was higher, else the RR. On a line `upper` and `lower` are the same, and
/// only the tier is said.
fn rank(
    painter: &egui::Painter,
    player: &Player,
    left: f32,
    (upper, lower): (f32, f32),
    look: &Look<'_>,
) {
    let m = look.metrics;
    let tier = player.rank_tier.unwrap_or(0);
    let centre = f32::midpoint(upper, lower);
    if tier >= 3 {
        paint::emblem(
            painter,
            tier,
            pos2(left + m.emblem / 2.0, centre + 1.0),
            m.emblem,
            if look.still { 0.0 } else { 1.0 },
        );
    }
    let x = left + m.emblem + space::MD;
    let name = player
        .rank
        .as_deref()
        .filter(|n| !n.is_empty())
        .unwrap_or("Unranked");
    let _tier = caps_text(
        painter,
        pos2(x, upper),
        Align2::LEFT_CENTER,
        name,
        Face::Display.at(m.tier),
        if tier >= 3 {
            colour::TEXT
        } else {
            colour::TEXT_DIM
        },
    );
    if !m.agent_line {
        return;
    }
    let under = rows::peak_line(player, tier).map_or_else(
        || player.rr.map(|rr| (format!("{rr} RR"), colour::TEXT_FAINT)),
        |peak| Some((peak, colour::TEXT_DIM)),
    );
    if let Some((text, tint)) = under {
        let _under = caps_text(
            painter,
            pos2(x, lower + 1.0),
            Align2::LEFT_CENTER,
            &text,
            paint::label(),
            tint,
        );
    }
}

/// The headshot rate and its colour, faint when there is none.
fn hs(side: Side, hs: Option<f64>) -> (String, Color32) {
    hs.map_or_else(
        || ("-".to_owned(), colour::TEXT_FAINT),
        |v| {
            (
                format!("{v:.0}%"),
                paint::heat(side, (v as f32 - 16.0) / 16.0),
            )
        },
    )
}

/// A value with its name before it, set right against `right` on `y`, and
/// where the name starts.
fn pair(
    painter: &egui::Painter,
    (right, y): (f32, f32),
    name: &str,
    value: &str,
    tint: Color32,
) -> f32 {
    let drawn = caps_text(
        painter,
        pos2(right, y),
        Align2::RIGHT_CENTER,
        value,
        Face::Display.at(size::BODY),
        tint,
    );
    let named = caps_text(
        painter,
        pos2(drawn.left() - space::SM, y + 0.5),
        Align2::RIGHT_CENTER,
        name,
        paint::label(),
        colour::TEXT_FAINT,
    );
    named.left()
}

#[cfg(test)]
mod tests {
    use super::{KD, MORE, RANK, SPACE, TAIL, Zones};
    use egui::{Rect, pos2, vec2};

    /// The zones are measured from the right, so every card and every line
    /// puts each number at the same x, and the name gets what is left.
    #[test]
    fn every_row_puts_its_numbers_in_the_same_place() {
        let card = Zones::of(Rect::from_min_size(pos2(0.0, 0.0), vec2(620.0, 48.0)));
        let line = Zones::of(Rect::from_min_size(pos2(0.0, 300.0), vec2(620.0, 30.0)));
        assert_eq!(
            (card.rank, card.kd, card.more),
            (line.rank, line.kd, line.more)
        );
        assert!((card.more.1 - (620.0 - TAIL)).abs() < 1e-3);
        assert!((card.name - 3.0f32.mul_add(-SPACE, 620.0 - TAIL - MORE - KD - RANK)).abs() < 1e-3);
    }
}
