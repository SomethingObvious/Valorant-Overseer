//! One player, as one slab. An enemy row is a broadcast lower third: the
//! killfeed crop at its head, a name you can read across the room, the
//! emblem in its own light, and the K/D as the largest numeral on the board.
//! An ally row is the same row at two thirds the size, and quieter.

use egui::{Align2, Color32, Rect, Response, Sense, Ui, pos2, vec2};
use overseer_core::Player;
use overseer_ui::{Face, caps_text, caps_width, colour, motion, shape, size, space};

use super::Side;
use super::grid::{Grid, Placed};
use super::paint;

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
    /// Whether that line gives the agent's place to the tags when there are
    /// any. The overlay's cards are too narrow for both, and the face
    /// already says who they play.
    pub(crate) tags_first: bool,
}

impl Metrics {
    /// The killfeed crop's width. Riot cuts it two by one, so it is as tall
    /// as the slab and twice as wide.
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
    tags_first: false,
};

/// An ally, in the window: the same row, quieter.
pub(crate) const ALLY: Metrics = Metrics {
    height: 42.0,
    name: 16.0,
    emblem: 24.0,
    tier: 13.0,
    kd: 21.0,
    stat: 14.0,
    pip: 7.0,
    agent_line: true,
    tags_first: false,
};

/// An enemy, in a window too narrow for the full row.
pub(crate) const SMALL: Metrics = Metrics {
    height: 44.0,
    name: 18.0,
    emblem: 28.0,
    tier: 14.0,
    kd: 26.0,
    stat: 15.0,
    pip: 8.0,
    agent_line: true,
    tags_first: false,
};

/// The gap between two slabs.
pub(crate) const GAP: f32 = space::SM;

/// Everything about a row that is not the player.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Look<'a> {
    /// Whose side it is on.
    pub(crate) side: Side,
    /// Its measurements.
    pub(crate) metrics: Metrics,
    /// Whether it is the one chosen.
    pub(crate) selected: bool,
    /// Whether you have written about them.
    pub(crate) noted: bool,
    /// The first tag you gave them, drawn after the name.
    pub(crate) tag: Option<&'a str>,
    /// What you wrote about them, for the hover on your tag.
    pub(crate) note: Option<&'a str>,
    /// How far the numerals have counted up, from nothing to all of it.
    pub(crate) counted: f32,
    /// The efficient tier: no motion, no blur.
    pub(crate) still: bool,
}

/// Draws one row, and hands back what the pointer did to it.
///
/// Every row is the same height, flagged or not, so a flag's reasons go on
/// its hover and at the top of the panel rather than on a second line.
pub(crate) fn row(ui: &mut Ui, player: &Player, grid: &Grid, look: &Look<'_>) -> Response {
    let enemy = look.side == Side::Enemy;
    let flagged = player.smurf;
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), look.metrics.height),
        Sense::click(),
    );
    ui.add_space(GAP);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter_at(rect.expand2(vec2(space::XL, space::MD)));
    // Keyed by the account rather than the slot, so the hover tint follows a
    // player when a sort moves them.
    let key = response.id.with(player.puuid.as_deref().unwrap_or(""));
    let lift = motion::eased(ui.ctx().animate_bool_with_time(
        key,
        response.hovered(),
        if look.still { 0.0 } else { motion::INSTANT },
    ));
    let fill = if look.selected {
        colour::BG_SELECTED
    } else if flagged && enemy {
        shape::blend(colour::BG_RAISED, colour::WARN, 0.07)
    } else {
        colour::BG_RAISED
    };
    let fill = shape::blend(fill, colour::BG_HOVER, lift * 0.8);
    paint::slab(&painter, rect, fill, lift, look.still);
    // The emblem's glow and the crop clip to the slab itself. The painter's
    // wider clip is there for the shadow, and through it the glow leaves
    // faint dots in the gap between rows.
    let inside = painter.with_clip_rect(rect);
    // The name stops at the first column, measured. Anybody worth a look
    // says so past the last column when there is room, else in place of
    // their tag.
    let (first, last) = grid.span().unwrap_or_else(|| (rect.width(), 0.0));
    let limit = rect.left() + first - space::MD;
    let room = rect.width() - space::LG - last - space::LG;
    let at_end = flagged && caps_width(&painter, FLAG, paint::label()) <= room;
    if player.agent.is_none() {
        paint::card_behind(&inside, player, rect, (fill, look.metrics.crop()));
    }
    let mut hovers = identity(&inside, player, (rect, limit), look, flagged && !at_end);
    // The rail goes over the crop's left edge rather than pushing the crop
    // right, so a flagged row's face and name line up with every other row.
    if flagged && enemy {
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(4.0, rect.height())),
            0,
            colour::WARN,
        );
    }
    if known(player) {
        for placed in &grid.placed {
            cell(&inside, player, rect, (*placed, grid.peak_under_rank), look);
        }
    }
    if look.noted {
        paint::dog_ear(&painter, rect);
    }
    if at_end {
        let drawn = caps_text(
            &painter,
            pos2(rect.right() - space::LG, rect.center().y),
            Align2::RIGHT_CENTER,
            FLAG,
            paint::label(),
            colour::WARN,
        );
        hovers.push((drawn.expand(space::SM), Hover::Flag));
    }
    // What a tag means, on the row's own hover rather than on widgets of
    // their own: a second widget over the row would take its hover, and
    // the row's hover is what brings the panel to that player.
    if let Some(pointer) = response.hover_pos()
        && let Some((_, what)) = hovers.iter().find(|(rect, _)| rect.contains(pointer))
    {
        let _shown = response
            .clone()
            .on_hover_ui_at_pointer(|ui| explain(ui, what, player, look));
    }
    response
}

/// Whether anything is known about the player yet. Nothing is, in agent
/// select or for somebody Riot said nothing about, and an empty slab says
/// so where a row of dashes looks broken.
pub(super) const fn known(player: &Player) -> bool {
    player.rank_tier.is_some()
        || player.kd.is_some()
        || player.win_rate.is_some()
        || player.level.is_some()
        || !player.form.is_empty()
}

/// Something on the row with more to say than fits on it.
pub(super) enum Hover<'a> {
    /// A tag the backend worked out.
    Auto(&'a overseer_core::AutoTag),
    /// Your own tag.
    Yours(&'a str),
    /// An account worth a look.
    Flag,
}

/// The card behind whatever the pointer is resting on.
fn explain(ui: &mut Ui, what: &Hover<'_>, player: &Player, look: &Look<'_>) {
    let accent = paint::win(look.side);
    match what {
        Hover::Auto(tag) => super::tip::auto(ui, tag, accent),
        Hover::Yours(tag) => super::tip::show(
            ui,
            &super::tip::Card {
                kind: "Your Tag",
                word: tag,
                means: look.note.unwrap_or("A word you gave them."),
                evidence: "N opens your note to change it.",
                accent: colour::TEXT_STRONG,
            },
        ),
        Hover::Flag => {
            let reasons = player.smurf_reasons.join("\n");
            super::tip::show(
                ui,
                &super::tip::Card {
                    kind: "Flag",
                    word: FLAG,
                    means: "The signals say this account plays above its rank.",
                    evidence: &reasons,
                    accent: colour::WARN,
                },
            );
        }
    }
}

/// Under this many games a win rate is drawn faint rather than hot.
pub(super) const FEW_GAMES: u32 = 10;

/// The same for one map, where five games is already a lot of somebody's
/// history.
pub(super) const FEW_MAP_GAMES: u32 = 5;

/// What an account worth a look is tagged with.
const FLAG: &str = "Worth a Look";

/// What sits after the name, in the order it matters: the flag, the tag
/// you gave them, whether Riot hid them, and their Riot tag.
enum Mark<'a> {
    Flag,
    Yours(&'a str),
    Hidden,
    Riot(&'a str),
}

/// The name to draw, and whether it is a stand-in for one.
///
/// Riot hides a name for anybody in streamer mode, and the game's own
/// scoreboard shows their agent instead, so this does too. With neither, the
/// row says unknown.
pub(crate) fn shown_name(player: &Player) -> (&str, &str, bool) {
    match player.name.as_deref().filter(|n| !n.is_empty()) {
        Some(full) => {
            let (name, tag) = split_name(full);
            (name, tag, false)
        }
        None if player.hidden.name => (player.agent.as_deref().unwrap_or("Hidden"), "", true),
        None => ("Unknown", "", true),
    }
}

/// The face and the name, the name stopping at `limit`. `flag` puts the
/// worth-a-look words where the Riot tag would go.
pub(super) fn identity<'a>(
    painter: &egui::Painter,
    player: &'a Player,
    (line, limit): (Rect, f32),
    look: &Look<'a>,
    flag: bool,
) -> Vec<(Rect, Hover<'a>)> {
    let mut hovers = Vec::new();
    let m = look.metrics;
    let crop = Rect::from_min_size(line.min, vec2(m.crop(), m.height));
    paint::crop(painter, player, crop);
    let x = crop.right() + space::LG;
    let (name, tag, standing_in) = shown_name(player);
    let mark = if flag {
        Some(Mark::Flag)
    } else if let Some(yours) = look.tag {
        Some(Mark::Yours(yours))
    } else if standing_in && player.hidden.name {
        // Only when the agent is standing in for the name. A hidden name
        // the backend knew anyway is just a name, and HIDDEN beside it reads
        // as part of it.
        Some(Mark::Hidden)
    } else {
        (!tag.is_empty()).then_some(Mark::Riot(tag))
    };
    let middle = if m.agent_line {
        m.name.mul_add(-0.32, line.center().y)
    } else {
        line.center().y
    };
    let font = Face::Display.at(m.name);
    let text = fit(painter, name, &font, limit - x);
    let after = x + caps_width(painter, &text, font.clone()) + space::SM;
    let fits = mark
        .as_ref()
        .is_none_or(|mark| after + mark_width(painter, mark, m) <= limit);
    let name_colour = if flag && !fits {
        // No room for the words, so the name carries the amber alone. The
        // flag must not vanish at a narrow width.
        colour::WARN
    } else if standing_in {
        colour::TEXT_DIM
    } else if player.is_self {
        colour::YOU
    } else if look.side == Side::Enemy {
        colour::TEXT_STRONG
    } else {
        colour::TEXT
    };
    let named = caps_text(
        painter,
        pos2(x, middle),
        Align2::LEFT_CENTER,
        &text,
        font,
        name_colour,
    );
    if flag && !fits {
        hovers.push((named, Hover::Flag));
    }
    if let Some(mark) = mark.filter(|_| fits) {
        let wide = mark_width(painter, &mark, m);
        let rect = Rect::from_min_size(pos2(after, middle - 9.0), vec2(wide, 18.0));
        match mark {
            Mark::Flag => hovers.push((rect, Hover::Flag)),
            Mark::Yours(tag) => hovers.push((rect, Hover::Yours(tag))),
            Mark::Hidden | Mark::Riot(_) => {}
        }
        draw_mark(painter, &mark, pos2(after, middle), m);
    }
    if m.agent_line {
        hovers.extend(
            tag_line(
                painter,
                player,
                pos2(x, m.name.mul_add(0.9, middle)),
                limit,
                standing_in || (m.tags_first && player.auto_tags.iter().any(|t| t.tag.is_some())),
            )
            .into_iter()
            .map(|(rect, tag)| (rect, Hover::Auto(tag))),
        );
    }
    hovers
}

/// Whether a tag changes how you play the next round: the guns they buy and
/// whether they save. Those go first on the row, in a chip.
fn valuable(tag: &overseer_core::AutoTag) -> bool {
    tag.kind
        .as_deref()
        .is_some_and(|k| k.eq_ignore_ascii_case("weapon") || k.eq_ignore_ascii_case("economy"))
}

/// The line under the name: the agent, faint, unless it already stands in
/// for the name, then the valuable tags as chips and as many of the rest as
/// fit before the first column. The overlay has no panel, so this is the only
/// place it shows a habit.
fn tag_line<'a>(
    painter: &egui::Painter,
    player: &'a Player,
    at: egui::Pos2,
    limit: f32,
    standing_in: bool,
) -> Vec<(Rect, &'a overseer_core::AutoTag)> {
    let font = Face::Number.at(size::MICRO + 1.0);
    let agent = player.agent.as_deref().filter(|_| !standing_in);
    let mut tags: Vec<&overseer_core::AutoTag> = player
        .auto_tags
        .iter()
        .filter(|t| t.tag.is_some())
        .collect();
    tags.sort_by_key(|t| !valuable(t));
    let words = agent.map(|a| (a, None)).into_iter().chain(
        tags.into_iter()
            .filter_map(|t| Some((t.tag.as_deref()?, Some(t)))),
    );
    let mut x = at.x;
    let mut last_chip = false;
    let mut drawn_tags = Vec::new();
    for (i, (word, tag)) in words.enumerate() {
        let chip = tag.is_some_and(valuable);
        let pad = if chip { space::MD } else { 0.0 };
        // A dot sits in the gap between two plain words. A chip's outline
        // already separates it.
        let gap = match i {
            0 => 0.0,
            _ if chip || last_chip => space::MD,
            _ => space::LG,
        };
        let wide = 2.0f32.mul_add(pad, caps_width(painter, word, font.clone()));
        if x + gap + wide > limit {
            break;
        }
        if i > 0 && !chip && !last_chip {
            painter.circle_filled(pos2(x + gap / 2.0, at.y + 0.5), 1.0, colour::TEXT_FAINT);
        }
        let left = x + gap;
        let tint = match tag {
            None => colour::TEXT_FAINT,
            Some(_) if chip => colour::TEXT_STRONG,
            Some(_) => colour::TEXT,
        };
        let drawn = caps_text(
            painter,
            pos2(left + pad, at.y),
            Align2::LEFT_CENTER,
            word,
            font.clone(),
            tint,
        );
        let area = Rect::from_min_max(
            pos2(left, at.y - 7.0),
            pos2(drawn.right() + pad, at.y + 7.0),
        );
        if chip {
            paint::chip_outline(painter, area, colour::TEXT_DIM);
        }
        if let Some(tag) = tag {
            drawn_tags.push((area.expand2(vec2(space::SM - pad / 2.0, 1.0)), tag));
        }
        last_chip = chip;
        x = area.right();
    }
    drawn_tags
}

/// How wide a mark is drawn.
fn mark_width(painter: &egui::Painter, mark: &Mark<'_>, m: Metrics) -> f32 {
    match mark {
        Mark::Flag => caps_width(painter, FLAG, paint::label()),
        Mark::Hidden => caps_width(painter, "Hidden", paint::label()),
        Mark::Yours(tag) => 2.0f32.mul_add(space::MD, caps_width(painter, tag, paint::label())),
        Mark::Riot(tag) => {
            painter
                .layout_no_wrap(
                    format!("#{tag}"),
                    Face::Number.at(m.name * 0.62),
                    colour::TEXT_FAINT,
                )
                .size()
                .x
        }
    }
}

/// Draws a mark with its left edge at `at`, centred on the name's line.
fn draw_mark(painter: &egui::Painter, mark: &Mark<'_>, at: egui::Pos2, m: Metrics) {
    let label = |text: &str, tint: Color32| {
        let _drawn = caps_text(painter, at, Align2::LEFT_CENTER, text, paint::label(), tint);
    };
    match mark {
        Mark::Flag => label(FLAG, colour::WARN),
        Mark::Hidden => label("Hidden", colour::TEXT_FAINT),
        // Your word goes on a cream chip, the same cream as the side plate
        // in the masthead.
        Mark::Yours(tag) => {
            let wide = 2.0f32.mul_add(space::MD, caps_width(painter, tag, paint::label()));
            let chip = Rect::from_min_size(pos2(at.x, at.y - 8.0), vec2(wide, 16.0));
            painter.add(paint::slant(chip, false, true, colour::TEXT_STRONG));
            let _drawn = caps_text(
                painter,
                pos2(chip.left() + space::MD - 1.0, chip.center().y),
                Align2::LEFT_CENTER,
                tag,
                paint::label(),
                colour::BG,
            );
        }
        Mark::Riot(tag) => {
            let galley = painter.layout_no_wrap(
                format!("#{tag}"),
                Face::Number.at(m.name * 0.62),
                colour::TEXT_FAINT,
            );
            let top = at.y + 1.0 - galley.size().y / 2.0;
            painter.galley(pos2(at.x, top), galley, colour::TEXT_FAINT);
        }
    }
}

/// The matches behind the K/D and the headshots, so one good game reads as
/// one game.
fn games_of(player: &Player) -> (String, Color32) {
    match (player.kd, player.form.len()) {
        (Some(_), n) if n > 0 => (
            n.to_string(),
            if few_games(player) {
                colour::TEXT_FAINT
            } else {
                colour::TEXT_DIM
            },
        ),
        _ => ("-".to_owned(), colour::TEXT_FAINT),
    }
}

/// The level and its colour. A low level on a ranked account is the first
/// smurf tell, so it is the one level worth a colour. A level hidden in game
/// is still exact here, so it is drawn faint and without a question mark.
pub(super) fn level_of(player: &Player) -> (String, Color32) {
    match player.level {
        Some(l) if player.hidden.level => (
            if l > 0 { l.to_string() } else { "-".to_owned() },
            colour::TEXT_FAINT,
        ),
        Some(l) if l < 60 && player.rank_tier.unwrap_or(0) >= 3 => (l.to_string(), colour::WARN),
        Some(l) => (l.to_string(), colour::TEXT_DIM),
        None => ("-".to_owned(), colour::TEXT_FAINT),
    }
}

/// One statistic in its column. `peak_under_rank` says the rank carries the
/// peak, since it is wanted and has no column of its own.
fn cell(
    painter: &egui::Painter,
    player: &Player,
    line: Rect,
    (placed, peak_under_rank): (Placed, bool),
    look: &Look<'_>,
) {
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
            let centre = pos2(f32::midpoint(left, right), y);
            emblem_only(painter, player, centre, look, peak_under_rank);
        }
        "rank" => rank_cell(painter, player, pos2(left, y), look, peak_under_rank),
        "peak" => peak_cell(painter, player, pos2(left, y), look),
        "k/d" => match player.kd {
            Some(kd) => kd_cell(painter, kd, pos2(right, y), look, few_games(player)),
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "games" => {
            let (text, tint) = games_of(player);
            stat(text, tint);
        }
        "hs" => match player.hs_pct {
            Some(v) => stat(
                format!("{v:.0}%"),
                paint::heat(side, (v as f32 - 16.0) / 16.0),
            ),
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "win" => {
            let (text, tint) = rate(side, player.win_rate, player.games, FEW_GAMES);
            stat(text, tint);
        }
        "lvl" => {
            let (text, tint) = level_of(player);
            stat(text, tint);
        }
        "met" => match player.met() {
            0 => {}
            n => stat(format!("{n}x"), colour::INFO),
        },
        "map" => {
            let map = player.map_win_rate.as_ref();
            let (text, tint) = rate(
                side,
                map.and_then(|m| m.win_rate),
                map.and_then(|m| m.games),
                FEW_MAP_GAMES,
            );
            stat(text, tint);
        }
        "rr" => match player.rr {
            Some(rr) => stat(rr.to_string(), colour::TEXT_DIM),
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "last 5" => {
            paint::pips(
                painter,
                &player.form,
                pos2(left, y),
                m.pip,
                paint::win(side),
            );
        }
        _ => {}
    }
}

/// A win rate and its colour. Under `few` games it is mostly luck, so it is
/// drawn faint rather than hot.
pub(super) fn rate(
    side: Side,
    rate: Option<f64>,
    games: Option<u32>,
    few: u32,
) -> (String, Color32) {
    match rate {
        Some(v) if games.is_some_and(|g| g < few) => (format!("{v:.0}%"), colour::TEXT_FAINT),
        Some(v) => (
            format!("{v:.0}%"),
            paint::heat(side, (v as f32 - 46.0) / 14.0),
        ),
        None => ("-".to_owned(), colour::TEXT_FAINT),
    }
}

/// The rank as its emblem alone, for a row too narrow for its word, with
/// the peak as a small emblem on its corner when it was a full rank higher.
fn emblem_only(
    painter: &egui::Painter,
    player: &Player,
    centre: egui::Pos2,
    look: &Look<'_>,
    with_peak: bool,
) {
    let tier = player.rank_tier.unwrap_or(0);
    let spin = if look.still { 0.0 } else { 1.0 };
    let side = look.metrics.emblem;
    if tier >= 3 {
        paint::emblem(painter, tier, pos2(centre.x, centre.y + 1.5), side, spin);
    }
    if let Some(peak) = player
        .peak_rank_tier
        .filter(|p| with_peak && *p >= tier + 3 && *p >= 3)
    {
        let badge = side * 0.5;
        paint::emblem(
            painter,
            peak,
            pos2(side.mul_add(0.42, centre.x), side.mul_add(0.32, centre.y)),
            badge,
            spin,
        );
    }
}

/// Under this many matches a K/D is a game or two, not a pattern.
const FEW_KD_GAMES: usize = 3;

/// Whether the K/D rests on too few matches to be read as form. With no
/// results at all the count is unknown, which is not the same as few.
pub(crate) fn few_games(player: &Player) -> bool {
    (1..FEW_KD_GAMES).contains(&player.form.len())
}

/// The K/D as the row's largest numeral, counting up while a lobby lands,
/// sat on its cap height rather than its line box. Off one or two games it
/// is drawn faint, the way a win rate off a few games is.
fn kd_cell(painter: &egui::Painter, kd: f64, right: egui::Pos2, look: &Look<'_>, few: bool) {
    let shown = kd * f64::from(look.counted);
    let _landed = paint::numeral(
        painter,
        &format!("{shown:.2}"),
        pos2(right.x, right.y - 1.5),
        &Face::Heavy.at(look.metrics.kd),
        if few {
            colour::TEXT_FAINT
        } else {
            paint::kd_heat(look.side, Some(kd))
        },
    );
}

/// The peak: a smaller emblem, the rank said the short way players say it,
/// and the act under it, since a peak without its act reads as current.
fn peak_cell(painter: &egui::Painter, player: &Player, at: egui::Pos2, look: &Look<'_>) {
    let m = look.metrics;
    let (Some(tier), Some(name)) = (
        player.peak_rank_tier.filter(|t| *t >= 3),
        player.peak_rank.as_deref(),
    ) else {
        painter.text(
            at,
            Align2::LEFT_CENTER,
            "-",
            Face::Display.at(m.tier),
            colour::TEXT_FAINT,
        );
        return;
    };
    // The same emblem and type as the rank beside it, so the two compare at
    // a glance.
    paint::emblem(
        painter,
        tier,
        pos2(at.x + m.emblem / 2.0, at.y + 1.5),
        m.emblem,
        if look.still { 0.0 } else { 1.0 },
    );
    let x = at.x + m.emblem + space::SM;
    let act = player
        .peak_act
        .as_deref()
        .map(act_short)
        .filter(|a| !a.is_empty());
    let y = if act.is_some() {
        m.tier.mul_add(-0.4, at.y)
    } else {
        at.y
    };
    // A full rank above today is the fact worth the brighter ink.
    let above = tier >= player.rank_tier.unwrap_or(0) + 3;
    let _drawn = caps_text(
        painter,
        pos2(x, y),
        Align2::LEFT_CENTER,
        name,
        Face::Display.at(m.tier),
        if above {
            colour::TEXT_STRONG
        } else {
            colour::TEXT
        },
    );
    if let Some(act) = act {
        let _drawn = caps_text(
            painter,
            pos2(x, m.tier.mul_add(0.95, y)),
            Align2::LEFT_CENTER,
            &act,
            Face::Number.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

/// A rank the short way it is said: "Ascendant 2" as "Asc 2".
fn short_rank(name: &str) -> String {
    let (group, division) = name.split_once(' ').unwrap_or((name, ""));
    let short = match group {
        "Ascendant" => "Asc",
        "Immortal" => "Imm",
        "Diamond" => "Dia",
        "Platinum" => "Plat",
        other => other,
    };
    format!("{short} {division}").trim().to_owned()
}

/// "V25 Act 4" as "V25A4", short enough to sit under a rank on a row.
fn act_short(act: &str) -> String {
    let words: Vec<&str> = act.split_whitespace().collect();
    match words.as_slice() {
        [season, act_word, number] if act_word.eq_ignore_ascii_case("act") => {
            format!("{}A{number}", season.to_uppercase())
        }
        _ => words.concat(),
    }
}

/// The emblem, the tier, and the peak under it when `peak_under_rank` asks.
fn rank_cell(
    painter: &egui::Painter,
    player: &Player,
    at: egui::Pos2,
    look: &Look<'_>,
    peak_under_rank: bool,
) {
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
    if tier >= 3 {
        paint::emblem(
            painter,
            tier,
            pos2(at.x + m.emblem / 2.0, at.y + 1.5),
            m.emblem,
            if look.still { 0.0 } else { 1.0 },
        );
    }
    let x = at.x + if tier >= 3 { m.emblem + space::SM } else { 0.0 };
    let peak = peak_line(player, tier).filter(|_| m.agent_line && peak_under_rank);
    let y = if peak.is_some() {
        m.tier.mul_add(-0.4, at.y)
    } else {
        at.y
    };
    let _drawn = caps_text(
        painter,
        pos2(x, y),
        Align2::LEFT_CENTER,
        name,
        Face::Display.at(m.tier),
        if tier >= 3 {
            colour::TEXT
        } else {
            colour::TEXT_DIM
        },
    );
    if let Some(peak) = peak {
        let _drawn = caps_text(
            painter,
            pos2(x, m.tier.mul_add(0.95, y)),
            Align2::LEFT_CENTER,
            &peak,
            Face::Number.at(size::MICRO + 1.0),
            colour::TEXT_FAINT,
        );
    }
}

/// "Peak Asc 2 V25A4", for the rank cell when the peak has no column of its
/// own, whenever the peak is above today. With its act, because a peak on
/// its own reads as current.
pub(super) fn peak_line(player: &Player, tier: u32) -> Option<String> {
    let peak = player.peak_rank_tier?;
    let name = player.peak_rank.as_deref()?;
    let act = player
        .peak_act
        .as_deref()
        .map(act_short)
        .unwrap_or_default();
    (peak > tier && peak >= 3).then(|| {
        format!("Peak {}  {act}", short_rank(name))
            .trim()
            .to_owned()
    })
}

/// A Riot id split at its tag.
fn split_name(full: &str) -> (&str, &str) {
    full.split_once('#').unwrap_or((full, ""))
}

/// The longest cut of a name that fits in `room`, measured in the face it
/// is drawn in, since a count of letters lets a wide name run into the rank.
pub(crate) fn fit(painter: &egui::Painter, name: &str, font: &egui::FontId, room: f32) -> String {
    let mut keep = name.chars().count();
    loop {
        let cut = clip(name, keep);
        if keep <= 4 || caps_width(painter, &cut, font.clone()) <= room {
            return cut;
        }
        keep -= 1;
    }
}

/// A name cut to a length, with an ellipsis if it had to be.
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
    use super::{act_short, clip, few_games, peak_line, short_rank};
    use overseer_core::Player;

    /// Ranks and acts come out the way players write them.
    #[test]
    fn a_peak_is_said_the_short_way() {
        assert_eq!(short_rank("Ascendant 2"), "Asc 2");
        assert_eq!(short_rank("Gold 1"), "Gold 1");
        assert_eq!(short_rank("Radiant"), "Radiant");
        assert_eq!(act_short("V25 Act 4"), "V25A4");
        assert_eq!(act_short("e9 act 3"), "E9A3");
        assert_eq!(act_short("Closed Beta"), "ClosedBeta");
    }

    /// One or two games is few. None is unknown, and three is a pattern.
    #[test]
    fn a_kd_off_one_game_is_few() {
        let played = |n: usize| Player {
            form: vec!["W".to_owned(); n],
            ..Player::default()
        };
        assert!(few_games(&played(1)) && few_games(&played(2)));
        assert!(!few_games(&played(0)) && !few_games(&played(3)));
    }

    /// A long name is cut with an ellipsis, a short one left alone.
    #[test]
    fn a_long_name_is_cut_at_its_length() {
        assert_eq!(clip("Day", 14), "Day");
        assert_eq!(clip("AVeryLongNameIndeed", 8), "AVeryLo\u{2026}");
    }

    /// The peak is only mentioned when it is two ranks or more above.
    #[test]
    fn a_peak_at_todays_rank_is_not_news() {
        let player = |tier, peak| Player {
            rank_tier: Some(tier),
            peak_rank_tier: Some(peak),
            peak_rank: Some("Diamond 2".to_owned()),
            peak_act: Some("V25 Act 4".to_owned()),
            ..Player::default()
        };
        assert!(peak_line(&player(14, 14), 14).is_none());
        assert_eq!(
            peak_line(&player(14, 19), 14).as_deref(),
            Some("Peak Dia 2  V25A4")
        );
    }
}
