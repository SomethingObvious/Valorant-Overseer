use egui::{Align2, Color32, Rect, Response, Sense, Ui, pos2, vec2};
use overseer_core::Player;
use overseer_ui::{Face, caps_text, caps_width, colour, motion, shape, size, space};

use super::Side;
use super::grid::{Grid, Placed};
use super::paint;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Metrics {
    pub(crate) height: f32,
    pub(crate) name: f32,
    pub(crate) emblem: f32,
    pub(crate) tier: f32,
    pub(crate) kd: f32,
    pub(crate) stat: f32,
    /// One result pip's width.
    pub(crate) pip: f32,
    pub(crate) tags_first: bool,
}

impl Metrics {
    /// The killfeed crop's width. Riot cuts it two by one, so it is as tall
    /// as the slab and twice as wide.
    pub(crate) fn crop(self) -> f32 {
        self.height * 2.0
    }
}

pub(crate) const ENEMY: Metrics = Metrics {
    height: 56.0,
    name: 21.0,
    emblem: 36.0,
    tier: 16.0,
    kd: 32.0,
    stat: 17.0,
    pip: 9.0,
    tags_first: false,
};

pub(crate) const ALLY: Metrics = Metrics {
    height: 42.0,
    name: 16.0,
    emblem: 24.0,
    tier: 13.0,
    kd: 21.0,
    stat: 14.0,
    pip: 7.0,
    tags_first: false,
};

pub(crate) const SMALL: Metrics = Metrics {
    height: 44.0,
    name: 18.0,
    emblem: 28.0,
    tier: 14.0,
    kd: 26.0,
    stat: 15.0,
    pip: 8.0,
    tags_first: false,
};

pub(crate) const GAP: f32 = space::SM;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Look<'a> {
    pub(crate) side: Side,
    pub(crate) metrics: Metrics,
    pub(crate) selected: bool,
    pub(crate) noted: bool,
    pub(crate) tag: Option<&'a str>,
    pub(crate) note: Option<&'a str>,
    pub(crate) counted: f32,
    pub(crate) still: bool,
    pub(crate) nothing: &'static str,
}

pub(crate) const fn nothing_yet(board: &overseer_core::Board) -> &'static str {
    if board.reading.is_some() {
        "Reading their games."
    } else {
        "Nothing on them yet."
    }
}

pub(crate) fn spoken(player: &Player) -> String {
    let mut said: Vec<String> = [&player.name, &player.agent, &player.rank]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    if let Some(kd) = player.kd {
        let n = player.form.len();
        said.push(if few_games(player) {
            format!("K/D {kd:.2} from {n} game{}", if n == 1 { "" } else { "s" })
        } else {
            format!("K/D {kd:.2}")
        });
    }
    if let Some(size) = player
        .party
        .as_ref()
        .and_then(|p| p.size)
        .filter(|&n| n > 1)
    {
        said.push(format!("in a party of {size}"));
    } else if let Some(size) = player
        .stack_guess
        .as_ref()
        .and_then(|g| g.size)
        .filter(|&n| n > 1)
    {
        said.push(format!("maybe in a party of {size}"));
    }
    if player.smurf {
        said.push("worth a look".to_owned());
        said.extend(player.smurf_reasons.iter().take(3).cloned());
    }
    if said.is_empty() {
        "Unknown player".to_owned()
    } else {
        said.join(", ")
    }
}

pub(crate) fn row(ui: &mut Ui, player: &Player, grid: &Grid, look: &Look<'_>) -> Response {
    let enemy = look.side == Side::Enemy;
    let flagged = player.smurf;
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), look.metrics.height),
        Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            look.selected,
            spoken(player),
        )
    });
    ui.add_space(GAP);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter_at(rect.expand2(vec2(space::XL, space::MD)));
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
    // The fill alone is too close to a plain row's to say which one is picked.
    if look.selected {
        brackets(&painter, rect);
    }
    if flagged && enemy {
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(4.0, rect.height())),
            0,
            colour::WARN,
        );
    }
    stats(&inside, player, (rect, grid), look);
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
    explain_at(&response, &hovers, player, look);
    response
}

pub(super) fn explain_at(
    response: &Response,
    hovers: &[(Rect, Hover<'_>)],
    player: &Player,
    look: &Look<'_>,
) {
    let on = response
        .hover_pos()
        .and_then(|pointer| hovers.iter().position(|(rect, _)| rect.contains(pointer)));
    if let Some((rect, _)) = on.and_then(|i| hovers.get(i)) {
        super::tip::mark_hovered(&response.ctx, response.layer_id, *rect);
    }
    let group = response.id.with("card");
    if let Some((rect, what)) =
        super::tip::lingering(&response.ctx, group, on).and_then(|i| hovers.get(i))
    {
        super::tip::under(&response.ctx, group, *rect, |ui| {
            explain(ui, what, player, look);
        });
    }
}

fn stats(inside: &egui::Painter, player: &Player, (rect, grid): (Rect, &Grid), look: &Look<'_>) {
    if known(player) {
        for placed in &grid.placed {
            cell(inside, player, rect, (*placed, grid), look);
        }
    } else if let Some(first) = grid.placed.first() {
        let why = if player.name.as_deref().is_none_or(str::is_empty) && !player.hidden.name {
            "Riot didn't say who this is."
        } else {
            look.nothing
        };
        inside.text(
            pos2(rect.left() + first.left, rect.center().y),
            Align2::LEFT_CENTER,
            why,
            Face::Body.at(size::LABEL + 1.0),
            colour::TEXT_FAINT,
        );
    }
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

pub(super) enum Hover<'a> {
    Auto(&'a overseer_core::AutoTag),
    Yours(&'a str),
    Flag,
}

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
                    means: "Numbers that are high for this rank and level.",
                    evidence: &reasons,
                    accent: colour::WARN,
                },
            );
        }
    }
}

pub(super) const FEW_GAMES: u32 = 10;

pub(super) const FEW_MAP_GAMES: u32 = 5;

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
/// worth-a-look words after the Riot tag, or in its place when only one fits.
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
    let riot = (!tag.is_empty()).then_some(Mark::Riot(tag));
    let mark = if flag {
        Some(Mark::Flag)
    } else if let Some(yours) = look.tag {
        Some(Mark::Yours(yours))
    } else if standing_in && player.hidden.name {
        Some(Mark::Hidden)
    } else {
        None
    };
    let middle = m.name.mul_add(-0.32, line.center().y);
    let font = Face::Display.at(m.name);
    let text = fit(painter, name, &font, limit - x);
    let mut after = x + caps_width(painter, &text, font.clone()) + space::SM;
    let riot_wide = riot.as_ref().map_or(0.0, |k| mark_width(painter, k, m));
    let mark_wide = mark.as_ref().map_or(0.0, |k| mark_width(painter, k, m));
    let both = after + riot_wide + mark.as_ref().map_or(0.0, |_| space::SM + mark_wide) <= limit;
    let (riot, moved) = if both {
        (riot, None)
    } else {
        (None, Some(tag).filter(|t| !t.is_empty()))
    };
    let fits = mark.is_none() || after + mark_wide <= limit;
    let named = caps_text(
        painter,
        pos2(x, middle),
        Align2::LEFT_CENTER,
        &text,
        font,
        name_colour(player, look.side, standing_in, flag && !fits),
    );
    if flag && !fits {
        hovers.push((named, Hover::Flag));
    }
    if let Some(tag) = riot {
        draw_mark(painter, &tag, pos2(after, middle), m);
        after += mark_width(painter, &tag, m) + space::SM;
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
    hovers.extend(
        tag_line(
            painter,
            player,
            pos2(x, m.name.mul_add(0.9, middle)),
            limit,
            standing_in || (m.tags_first && player.auto_tags.iter().any(|t| t.tag.is_some())),
            moved,
        )
        .into_iter()
        .map(|(rect, tag)| (rect, Hover::Auto(tag))),
    );
    hovers
}

fn name_colour(player: &Player, side: Side, standing_in: bool, amber: bool) -> Color32 {
    if amber {
        colour::WARN
    } else if standing_in {
        colour::TEXT_DIM
    } else if player.is_self {
        colour::YOU
    } else if side == Side::Enemy {
        colour::TEXT_STRONG
    } else {
        colour::TEXT
    }
}

fn valuable(tag: &overseer_core::AutoTag) -> bool {
    tier(tag) == 0
}

fn tier(tag: &overseer_core::AutoTag) -> u8 {
    let kind = tag.kind.as_deref().unwrap_or_default();
    let is = |name: &str| kind.eq_ignore_ascii_case(name);
    if is("weapon") || is("economy") {
        0
    } else if is("agent") || is("rank") {
        1
    } else if is("duels") {
        2
    } else {
        3
    }
}

fn line_words(player: &Player, standing_in: bool) -> (Option<&str>, Vec<Word<'_>>) {
    let agent = player.agent.as_deref().filter(|_| !standing_in);
    let unpicked = player.agent.is_none();
    let role = player.main_role().filter(|_| unpicked);
    let mains: Vec<&str> = if unpicked {
        player
            .mains()
            .iter()
            .filter_map(|a| a.agent.as_deref())
            .collect()
    } else {
        Vec::new()
    };
    let mut tags: Vec<&overseer_core::AutoTag> = player
        .auto_tags
        .iter()
        .filter(|t| t.tag.is_some())
        .collect();
    tags.sort_by_key(|t| t.order.unwrap_or_else(|| tier(t)));
    let words = role
        .into_iter()
        .chain(agent)
        .chain(mains)
        .map(|a| (a, None))
        .chain(
            tags.into_iter()
                .filter_map(|t| Some((t.tag.as_deref()?, Some(t)))),
        )
        .collect();
    (role, words)
}

type Word<'a> = (&'a str, Option<&'a overseer_core::AutoTag>);

/// The line under the name: the agent, faint, unless it already stands in
/// for the name, then the valuable tags as chips and as many of the rest as
/// fit before the first column. Before they pick, their mains take the
/// agent's place, led by the role two of them share. A Riot tag with no
/// room beside the name leads the line instead. The overlay has no panel, so
/// this is the only place it shows a habit.
fn tag_line<'a>(
    painter: &egui::Painter,
    player: &'a Player,
    at: egui::Pos2,
    limit: f32,
    standing_in: bool,
    riot: Option<&str>,
) -> Vec<(Rect, &'a overseer_core::AutoTag)> {
    let font = Face::Number.at(size::MICRO + 1.0);
    let riot = riot.map(|t| format!("#{t}"));
    let (role, mut words) = line_words(player, standing_in);
    let wide = |(word, tag): &Word<'_>| {
        let around = if tag.is_some_and(valuable) {
            space::MD * 3.0
        } else {
            space::LG
        };
        around + caps_width(painter, word, font.clone())
    };
    // A Riot tag keeps the case it was typed in, here as beside the name.
    let riot_wide = |riot: &str| overseer_ui::typed_width(painter, riot, font.clone());
    let leads = riot
        .as_deref()
        .filter(|riot| at.x + riot_wide(riot) + words.iter().map(wide).sum::<f32>() <= limit);
    if let Some(riot) = leads {
        words.insert(0, (riot, None));
    }
    let leads = leads.is_some();
    let mut x = at.x;
    let mut last_chip = false;
    let mut first = true;
    let mut drawn_tags = Vec::new();
    for (i, (word, tag)) in words.into_iter().enumerate() {
        let chip = tag.is_some_and(valuable);
        let pad = if chip { space::MD } else { 0.0 };
        let gap = if first {
            0.0
        } else if chip || last_chip {
            space::MD
        } else {
            space::LG
        };
        let typed = leads && i == 0;
        let wide = if typed {
            riot_wide(word)
        } else {
            2.0f32.mul_add(pad, caps_width(painter, word, font.clone()))
        };
        if x + gap + wide > limit {
            continue;
        }
        if !first && !chip && !last_chip {
            painter.circle_filled(pos2(x + gap / 2.0, at.y + 0.5), 1.0, colour::TEXT_FAINT);
        }
        let left = x + gap;
        let tint = match tag {
            None if Some(word) == role => colour::TEXT_DIM,
            None => colour::TEXT_FAINT,
            Some(_) if chip => colour::TEXT_STRONG,
            Some(_) => colour::TEXT,
        };
        let draw = if typed {
            overseer_ui::typed_text
        } else {
            caps_text
        };
        let drawn = draw(
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
        first = false;
        x = area.right();
    }
    drawn_tags
}

fn mark_width(painter: &egui::Painter, mark: &Mark<'_>, m: Metrics) -> f32 {
    match mark {
        Mark::Flag => caps_width(painter, FLAG, paint::label()),
        Mark::Hidden => caps_width(painter, "Hidden", paint::label()),
        Mark::Yours(tag) => 2.0f32.mul_add(
            space::MD,
            overseer_ui::typed_width(painter, &crate::notes::shown_tag(tag), paint::label()),
        ),
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

fn draw_mark(painter: &egui::Painter, mark: &Mark<'_>, at: egui::Pos2, m: Metrics) {
    let label = |text: &str, tint: Color32| {
        let _drawn = caps_text(painter, at, Align2::LEFT_CENTER, text, paint::label(), tint);
    };
    match mark {
        Mark::Flag => label(FLAG, colour::WARN),
        Mark::Hidden => label("Hidden", colour::TEXT_FAINT),
        Mark::Yours(tag) => {
            let tag = crate::notes::shown_tag(tag);
            let wide = 2.0f32.mul_add(
                space::MD,
                overseer_ui::typed_width(painter, &tag, paint::label()),
            );
            let chip = Rect::from_min_size(pos2(at.x, at.y - 8.0), vec2(wide, 16.0));
            painter.add(paint::slant(chip, false, true, colour::TEXT_STRONG));
            let _drawn = overseer_ui::typed_text(
                painter,
                pos2(chip.left() + space::MD - 1.0, chip.center().y),
                Align2::LEFT_CENTER,
                &tag,
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

pub(super) fn level_of(player: &Player) -> (String, Color32) {
    match player.level {
        Some(l) if player.hidden.level => (
            if l > 0 { l.to_string() } else { "-".to_owned() },
            colour::TEXT_FAINT,
        ),
        Some(l) if l < 60 && player.rank_tier.unwrap_or(0) >= 3 => {
            (l.to_string(), colour::TEXT_STRONG)
        }
        Some(l) => (l.to_string(), colour::TEXT_DIM),
        None => ("-".to_owned(), colour::TEXT_FAINT),
    }
}

fn cell(
    painter: &egui::Painter,
    player: &Player,
    line: Rect,
    (placed, grid): (Placed, &Grid),
    look: &Look<'_>,
) {
    let peak_under_rank = grid.peak_under_rank;
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
            Some(kd) => {
                let few = few_games(player);
                let under = under_kd(player, grid, side, few);
                kd_cell(painter, kd, pos2(right, y), look, (few, under));
            }
            None => stat("-".to_owned(), colour::TEXT_FAINT),
        },
        "games" => {
            let (text, tint) = games_of(player);
            stat(text, tint);
        }
        "hs" => {
            let (text, tint) = hs(side, player.hs_pct);
            stat(text, tint);
        }
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

pub(super) fn hs(side: Side, hs: Option<f64>) -> (String, Color32) {
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

const FEW_KD_GAMES: usize = 3;

pub(crate) fn few_games(player: &Player) -> bool {
    (1..FEW_KD_GAMES).contains(&player.form.len())
}

fn under_kd(player: &Player, grid: &Grid, side: Side, few: bool) -> Option<(String, Color32)> {
    if few && !grid.games_shown {
        let n = player.form.len();
        return Some((
            format!("{n} {}", if n == 1 { "Game" } else { "Games" }),
            colour::TEXT_FAINT,
        ));
    }
    if grid.win_under_kd && player.win_rate.is_some() {
        let (text, tint) = rate(side, player.win_rate, player.games, FEW_GAMES);
        return Some((format!("Win {text}"), tint));
    }
    None
}

fn kd_cell(
    painter: &egui::Painter,
    kd: f64,
    right: egui::Pos2,
    look: &Look<'_>,
    (few, under): (bool, Option<(String, Color32)>),
) {
    let shown = kd * f64::from(look.counted);
    let lift = if under.is_some() { 5.0 } else { 0.0 };
    let landed = paint::numeral(
        painter,
        &format!("{shown:.2}"),
        pos2(right.x, right.y - 1.5 - lift),
        &Face::Heavy.at(look.metrics.kd),
        if few {
            colour::TEXT_FAINT
        } else {
            paint::kd_heat(look.side, Some(kd))
        },
    );
    if let Some((said, tint)) = under {
        let _said = caps_text(
            painter,
            pos2(right.x, landed.bottom() + 2.0),
            Align2::RIGHT_TOP,
            &said,
            Face::Display.at(size::MICRO),
            tint,
        );
    }
}

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
    let peak = peak_line(player, tier).filter(|_| peak_under_rank);
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
    let total = name.chars().count();
    if total <= 4 || caps_width(painter, name, font.clone()) <= room {
        return name.to_owned();
    }
    let (mut low, mut high) = (4, total - 1);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if caps_width(painter, &clip(name, mid), font.clone()) <= room {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    clip(name, low)
}

fn clip(name: &str, keep: usize) -> String {
    if name.chars().count() <= keep {
        return name.to_owned();
    }
    let mut out: String = name.chars().take(keep.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// Bone brackets just outside a row's corners, the way the website marks
/// what its board key points at.
fn brackets(painter: &egui::Painter, rect: Rect) {
    let outer = rect.expand(5.0);
    let (arm, tall, thick) = (16.0, 14.0, 3.0);
    for (corner, (x, y)) in [
        (outer.left_top(), (1.0, 1.0)),
        (outer.right_top(), (-1.0, 1.0)),
        (outer.left_bottom(), (1.0, -1.0)),
        (outer.right_bottom(), (-1.0, -1.0)),
    ] {
        painter.rect_filled(
            Rect::from_two_pos(corner, corner + vec2(x * arm, y * thick)),
            0,
            colour::TEXT_STRONG,
        );
        painter.rect_filled(
            Rect::from_two_pos(corner, corner + vec2(x * thick, y * tall)),
            0,
            colour::TEXT_STRONG,
        );
    }
}

#[cfg(test)]
mod tests {
    use overseer_core::{AutoTag, Player};

    #[test]
    fn tags_come_in_tiers() {
        let tag = |word: &str, kind: &str| AutoTag {
            tag: Some(word.to_owned()),
            kind: Some(kind.to_owned()),
            ..AutoTag::default()
        };
        let player = Player {
            agent: Some("Jett".to_owned()),
            auto_tags: vec![
                tag("entry", "Duels"),
                tag("hot", "Form"),
                tag("clutch", "Duels"),
                tag("odin", "Weapon"),
                tag("new", "Something"),
                tag("deranked", "Rank"),
                tag("saves", "Economy"),
                tag("aced", "Duels"),
            ],
            ..Player::default()
        };
        let (_, words) = super::line_words(&player, false);
        let order: Vec<&str> = words.iter().map(|(word, _)| *word).collect();
        assert_eq!(
            order,
            [
                "Jett", "odin", "saves", "deranked", "entry", "clutch", "aced", "hot", "new"
            ]
        );
    }
}
