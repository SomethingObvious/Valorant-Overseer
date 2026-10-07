//! The page a lineup is written on: where it goes on the map, who throws it,
//! and the optional utility, side, site, clip, name and notes. Each part is a
//! section with a tick once it is done, the optional ones say so, and Save
//! stays at the bottom of the panel where it can't scroll away.

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use overseer_core::{Atlas, Plan, Tools};
use overseer_ui::{Face, caps_text, colour, size, space};

use super::player::{self, Player, Prefs};
use super::side::{notice, title, words};
use super::{Draft, Job, Measure, Mode, Place, Request, SPIKE, View, areas, pick, pictures, plan};
use crate::controls::{self, Tone};

/// The page above the footer. Returns a request when the clip's source
/// should be measured, or fetched to watch.
pub(super) fn body(ui: &mut Ui, atlas: &Atlas, view: &mut View) -> Option<Request> {
    let main = view.main.clone();
    let View {
        mode: Mode::Edit(draft),
        player,
        prefs,
        default_agent,
        new_default,
        ..
    } = view
    else {
        return None;
    };
    let default = &mut (default_agent.as_str(), new_default);
    let heading = if draft.lineup.id.is_some() {
        "Edit Lineup"
    } else {
        "Add a Lineup"
    };
    title(ui, heading, &draft.lineup.map.clone());
    let planted = draft.lineup.agent == SPIKE;
    let placed = (planted || draft.lineup.stand.is_some()) && draft.lineup.land.is_some();
    section(ui, "On the Map", placed, false);
    placing(ui, atlas, draft, main.as_deref());
    section(ui, "Agent", !draft.lineup.agent.is_empty(), false);
    pick::agent(ui, atlas, draft, (main.as_deref(), default));
    if planted {
        // The Spike has no utility to pick.
    } else if draft.lineup.agent.is_empty() {
        section(ui, "Utility", false, true);
        words(ui, "Pick the agent first.", colour::TEXT_FAINT);
    } else {
        section(ui, "Utility", draft.lineup.ability.is_some(), true);
        pick::utility(ui, atlas, draft);
    }
    section(ui, "Side and Site", draft.lineup.site.is_some(), true);
    whereabouts(
        ui,
        atlas.maps.iter().find(|p| p.name == draft.lineup.map),
        draft,
    );
    section(ui, "Clip", !draft.source.trim().is_empty(), true);
    let asked = clip(ui, atlas.tools, draft, (player, *prefs));
    section(
        ui,
        "Name and Notes",
        !draft.lineup.title.trim().is_empty(),
        true,
    );
    let wide = ui.available_width();
    let hint = draft.named();
    let _title = controls::field(ui, &mut draft.lineup.title, &hint, wide);
    if draft.notes_open || !draft.notes.is_empty() {
        // Notes are kept in capitals, read at a glance like a callout.
        let _notes = controls::notes(
            ui,
            &mut draft.notes,
            "How to throw it, like jump and left click",
            true,
        );
    } else if controls::button(ui, "Add Notes", Tone::Plain, true).clicked() {
        draft.notes_open = true;
    }
    extras(ui, draft);
    ui.add_space(space::XL);
    asked
}

/// Save and Cancel, and what still stops a save, kept at the panel's foot.
pub(super) fn footer(ui: &mut Ui, view: &mut View) -> Option<Request> {
    let busy = view.pending.map(|(_, job)| job);
    let Mode::Edit(draft) = &view.mode else {
        return None;
    };
    let missing = draft.missing();
    let (mut saved, mut cancelled) = (false, false);
    ui.horizontal(|ui| {
        saved = controls::button(
            ui,
            "Save Lineup",
            Tone::Primary,
            missing.is_none() && busy.is_none(),
        )
        .clicked();
        cancelled = controls::button(ui, "Cancel", Tone::Plain, busy.is_none()).clicked();
    });
    match (busy, missing) {
        (Some(Job::Save), _) => words(
            ui,
            "Saving. A clip from a link downloads first, which can take a minute.",
            colour::TEXT_DIM,
        ),
        (_, Some(why)) => notice(ui, why, colour::WARN),
        _ => notice(ui, "Ready to save.", colour::ALLY),
    }
    if cancelled {
        view.mode = Mode::Browse;
        view.said = None;
        return None;
    }
    saved.then_some(Request::Save)
}

/// A section's heading with a rule to the edge, and a tick once it is done
/// or else Optional for the parts a lineup doesn't need.
fn section(ui: &mut Ui, name: &str, done: bool, optional: bool) {
    ui.add_space(space::LG);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let said = caps_text(
        painter,
        pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        name,
        Face::Display.at(size::BODY),
        colour::TEXT_STRONG,
    );
    let mut right = rect.right();
    if done {
        right = tick(painter, pos2(right - 8.0, rect.center().y)).left();
    } else if optional {
        let drawn = caps_text(
            painter,
            pos2(right, rect.center().y + 1.0),
            Align2::RIGHT_CENTER,
            "Optional",
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT,
        );
        right = drawn.left();
    }
    if right - space::MD > said.right() + space::MD {
        painter.hline(
            said.right() + space::MD..=right - space::MD,
            rect.center().y + 1.0,
            (1.0, colour::LINE),
        );
    }
}

/// A green square with a tick in it, centred on `centre`. Returns where it went.
fn tick(painter: &egui::Painter, centre: egui::Pos2) -> Rect {
    let mark = Rect::from_center_size(centre, vec2(16.0, 16.0));
    painter.rect_filled(mark, 0, colour::ALLY);
    painter.add(egui::Shape::line(
        vec![
            mark.left_center() + vec2(3.5, 0.0),
            mark.center_bottom() + vec2(-1.0, -4.0),
            mark.right_top() + vec2(-3.5, 4.0),
        ],
        Stroke::new(2.0, colour::BG),
    ));
    mark
}

/// How tall a place's row is.
const PLACE_ROW: f32 = 44.0;

/// Where you stand and where it lands, each a row with its pin drawn the way
/// the map draws it. The one the next click on the map sets is lit like a
/// picked lineup, and a placed one has a tick.
fn placing(ui: &mut Ui, atlas: &Atlas, draft: &mut Draft, main: Option<&str>) {
    let most = areas::spots(draft.lineup.ability.as_deref()).map_or(1, |s| s.0);
    for (place, name, set) in rows(draft, most) {
        let next = draft.placing == place;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), PLACE_ROW), Sense::click());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let fill = if next {
                colour::BG_SELECTED
            } else if response.hovered() {
                colour::BG_HOVER
            } else {
                colour::BG_INSET
            };
            painter.rect_filled(rect, 0, fill);
            if next {
                let rail = Rect::from_min_size(rect.min, vec2(3.0, rect.height()));
                painter.rect_filled(rail, 0, colour::TEXT_STRONG);
            }
            let ink = if set {
                colour::TEXT_STRONG
            } else {
                colour::TEXT_FAINT.gamma_multiply(0.7)
            };
            let at = pos2(rect.left() + 26.0, rect.center().y);
            match place {
                Place::Stand => plan::stand_mark(painter, at, 13.0, &draft.lineup, (ink, main)),
                Place::Land | Place::More | Place::Point(_) => {
                    plan::land_mark(painter, at, 12.0, atlas, &draft.lineup, ink);
                }
            }
            let placed = draft.lineup.points.len() + usize::from(draft.lineup.land.is_some());
            let (status, tint) = status(place, (set, next), (placed, most));
            let text_at = rect.left() + 52.0;
            let _name = caps_text(
                painter,
                pos2(text_at, rect.top() + 15.0),
                Align2::LEFT_CENTER,
                name,
                Face::Display.at(size::TITLE),
                if next || set {
                    colour::TEXT_STRONG
                } else {
                    colour::TEXT_DIM
                },
            );
            let _status = caps_text(
                painter,
                pos2(text_at, rect.top() + 31.0),
                Align2::LEFT_CENTER,
                &status,
                Face::Display.at(size::MICRO),
                tint,
            );
            if set {
                let _tick = tick(painter, pos2(rect.right() - 18.0, rect.center().y));
            }
        }
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            draft.placing = place;
        }
    }
}

/// The rows under On the Map: where you stand and where it lands, or for
/// the Spike only where it's planted, and a row for more places it lands
/// when it can be put down in `most` of them.
fn rows(draft: &Draft, most: usize) -> Vec<(Place, &'static str, bool)> {
    let land = draft.lineup.land.is_some();
    let mut rows = if draft.lineup.agent == SPIKE {
        vec![(Place::Land, "Where It's Planted", land)]
    } else {
        vec![
            (
                Place::Stand,
                "Where You Stand",
                draft.lineup.stand.is_some(),
            ),
            (Place::Land, "Where It Lands", land),
        ]
    };
    if most > 1 {
        rows.push((
            Place::More,
            "More Places It Lands",
            !draft.lineup.points.is_empty(),
        ));
    }
    rows
}

/// What a row under On the Map says, and in what colour: whether it is
/// placed and what a click on the map does. The row for more places it lands
/// counts `placed` of the `most` it can.
fn status(
    place: Place,
    (set, next): (bool, bool),
    (placed, most): (usize, usize),
) -> (String, Color32) {
    let (words, tint) = match (place, set, next) {
        (Place::More, _, next) => {
            let words = format!(
                "{placed} of {most} placed. Click the map to add one, or one to take it off"
            );
            return (words, if next { colour::TEXT } else { colour::TEXT_DIM });
        }
        (_, true, true) => ("Click the map to move it", colour::TEXT),
        (_, true, false) => ("Placed", colour::TEXT_DIM),
        (_, false, true) => ("Click the map to place it", colour::TEXT),
        (_, false, false) => ("Not placed yet", colour::TEXT_FAINT),
    };
    (words.to_owned(), tint)
}

/// Which side and site it is for.
fn whereabouts(ui: &mut Ui, plan: Option<&Plan>, draft: &mut Draft) {
    ui.horizontal_wrapped(|ui| {
        for (side, name) in [("attack", "Attacking"), ("defense", "Defending")] {
            if controls::chip(ui, name, draft.lineup.side.as_deref() == Some(side)).clicked() {
                draft.lineup.side = Some(side.to_owned());
            }
        }
        ui.add_space(space::LG);
        for site in plan.map(|p| p.sites.as_slice()).unwrap_or_default() {
            let on = draft.lineup.site.as_ref() == Some(site);
            if controls::chip(ui, &format!("{site} Site"), on).clicked() {
                draft.lineup.site = if on { None } else { Some(site.clone()) };
            }
        }
    });
}

/// How far a trim handle reaches for the pointer, either side of it.
const GRIP: f32 = 8.0;
/// The shortest clip the trim bar keeps, in seconds.
const SHORTEST: f64 = 0.5;
/// How tall the trim bar, the From and To boxes and the volume under the
/// video are, together, in points.
const CUTTING_ROOM: f32 = 150.0;
/// The longest clip the backend cuts, in seconds.
pub(super) const LONGEST: f64 = 90.0;
/// How long the trim bar takes to zoom to a new span, in seconds.
const ZOOM: f32 = 0.25;

/// The long description and the pictures, both optional. A picture comes in
/// by its path or by dropping it onto the window.
fn extras(ui: &mut Ui, draft: &mut Draft) {
    let filled = !draft.description.trim().is_empty() || !draft.lineup.images.is_empty();
    section(ui, "Description and Pictures", filled, true);
    let _description = controls::notes(
        ui,
        &mut draft.description,
        "Anything else worth knowing about it, as long as it needs",
        false,
    );
    let (_, removed) = pictures::thumbnails(ui, &draft.lineup.images, true);
    if let Some(at) = removed {
        draft.lineup.images.remove(at);
    }
    let room = draft.lineup.images.len() < pictures::MOST;
    let mut add = false;
    ui.horizontal(|ui| {
        let wide = ui.available_width() - controls::width(ui, "Add Picture") - space::SM;
        let typed = controls::field(
            ui,
            &mut draft.picture,
            "The path of a picture on this PC",
            wide,
        );
        add = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let can = room && !draft.picture.trim().is_empty();
        add |= controls::button(ui, "Add Picture", Tone::Plain, can).clicked();
    });
    // Pasted from Explorer, a path comes in quotes.
    let path = draft.picture.trim().trim_matches('"').to_owned();
    if add && room && !path.is_empty() {
        draft.lineup.images.push(path);
        draft.picture.clear();
    }
    words(
        ui,
        &format!(
            "Or drop pictures onto the window. Up to {} each, as PNG or JPEG.",
            pictures::MOST
        ),
        colour::TEXT_FAINT,
    );
}

/// The clip: a button until it is wanted, then where it comes from, the
/// video itself, a trim bar over it, where it is cut and how loud it is.
/// Dragging a handle or typing a time shows that frame, and Play plays the
/// part kept. Asks how long the source runs once it has sat unchanged for a
/// moment, then for a file of it the window can play.
fn clip(
    ui: &mut Ui,
    tools: Tools,
    draft: &mut Draft,
    (slot, prefs): (&mut Option<Player>, Prefs),
) -> Option<Request> {
    if !draft.clip_open && draft.source.trim().is_empty() {
        if controls::button(ui, "Add a Clip", Tone::Plain, true).clicked() {
            draft.clip_open = true;
        }
        return None;
    }
    let source = draft.source.trim().to_owned();
    let loaded = !source.is_empty() && source == draft.measure.of;
    let busy = draft.measure.asking || draft.measure.fetching;
    let (label, can) = match (loaded, busy, draft.measure.file.is_some()) {
        (true, true, _) => ("Loading", false),
        (true, false, true) => ("Loaded", false),
        (true, false, false) => ("Load Again", true),
        (false, _, _) => ("Load Video", !source.is_empty()),
    };
    let mut wanted = false;
    ui.horizontal(|ui| {
        let wide = ui.available_width() - controls::width(ui, label) - space::SM;
        let typed = controls::field(
            ui,
            &mut draft.source,
            "A YouTube link, or a video file on this PC",
            wide,
        );
        if typed.changed() {
            draft.measure = Measure::default();
            *slot = None;
        }
        // Enter in the box loads it the same as the button.
        wanted = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        wanted |= controls::button(ui, label, Tone::Primary, can).clicked();
    });
    if wanted && can {
        draft.measure = Measure {
            wanted: true,
            ..Measure::default()
        };
        *slot = None;
    }
    watch(ui, draft, slot, prefs);
    scrub(ui, draft, slot);
    let (mut from, mut to) = (false, false);
    ui.horizontal(|ui| {
        caption(ui, "From");
        from = controls::field(ui, &mut draft.from, "0:00", 72.0).changed();
        ui.add_space(space::MD);
        caption(ui, "To");
        to = controls::field(ui, &mut draft.to, "9:99", 72.0).changed();
    });
    if let (Some(player), Some((start, end))) = (slot.as_mut(), kept(draft))
        && (from || to)
    {
        player.still(if to { end } else { start });
    }
    ui.horizontal(|ui| {
        caption(ui, "Volume");
        ui.add(
            egui::Slider::new(&mut draft.volume, 0.0..=200.0)
                .suffix("%")
                .step_by(5.0),
        );
    });
    let link = draft.source.trim().starts_with("http");
    missing(ui, tools, link);
    if controls::button(ui, "Remove Clip", Tone::Plain, true).clicked() {
        draft.source.clear();
        draft.from.clear();
        draft.to.clear();
        draft.clip_open = false;
        draft.measure = Measure::default();
        *slot = None;
    }
    if measure_now(draft, tools) {
        return Some(Request::Measure);
    }
    watch_now(draft, tools, link).then_some(Request::Watch)
}

/// The video, once there is a file of it to play, or else what is still
/// being done to get one, playing the part kept. Before Load Video, a
/// saved lineup shows its clip as it was cut.
fn watch(ui: &mut Ui, draft: &Draft, slot: &mut Option<Player>, prefs: Prefs) {
    let Some(file) = draft.measure.file.as_deref() else {
        if draft.measure.asking {
            words(ui, "Reading how long the video is.", colour::TEXT_FAINT);
        } else if draft.measure.fetching {
            words(
                ui,
                "Getting a copy of the video to watch here. A link downloads once, at up to 1080p.",
                colour::TEXT_FAINT,
            );
        } else if let Some(why) = draft.measure.failed.as_deref() {
            notice(ui, why, colour::WARN);
        } else if let Some((cut, runs)) = draft.cut.as_ref().filter(|_| draft.same_source()) {
            // Its volume is in it already, so it plays as it is.
            let prefs = Prefs {
                autoplay: false,
                size: prefs.size * 0.6,
                ..prefs
            };
            player::of(slot, cut, 0.0, prefs).show(ui, (0.0, *runs), 100.0, CUTTING_ROOM);
            words(
                ui,
                "The clip as it was saved. Load Video to trim it again.",
                colour::TEXT_FAINT,
            );
        }
        return;
    };
    let span = kept(draft).unwrap_or((0.0, LONGEST));
    // Nothing starts on its own while it is being cut, and it is smaller than
    // a saved lineup's, so the trim bar, the times and the volume under it
    // fit on screen with it.
    let prefs = Prefs {
        autoplay: false,
        size: prefs.size * 0.6,
        ..prefs
    };
    let player = player::of(slot, file, span.0, prefs);
    // The bar carries the clip's volume, so the clip itself plays as it is.
    player.follow(draft.volume, ui.input(|i| i.time));
    player.show(ui, span, 100.0, CUTTING_ROOM);
}

/// The trim bar, and the video following a handle while it is dragged, then
/// on the exact frame once it is let go. Windows' engine seeks as the handle
/// moves. When ffmpeg plays it instead, a strip of small frames is decoded
/// for the stretch the bar shows whenever nothing is held.
fn scrub(ui: &mut Ui, draft: &mut Draft, slot: &mut Option<Player>) {
    let held = draft.measure.held.is_some();
    let dragged = trim(ui, draft, slot.as_ref().and_then(Player::playhead));
    if let Some(player) = slot.as_mut() {
        if let Some(at) = dragged {
            draft.measure.last = Some(at);
            player.preview(ui.ctx(), at);
        } else if held
            && draft.measure.held.is_none()
            && let Some(at) = draft.measure.last.take()
        {
            // The exact frame once let go, since the strip's are only near.
            player.still(at);
        }
        // Decoded ahead for the stretch the trim bar shows, once nothing is
        // being dragged, so a drag never waits on a new strip.
        if draft.measure.held.is_none()
            && let Some(reach) = reach(draft)
        {
            player.prepare(reach);
        }
    }
}

/// The stretch the trim bar spans once it settles: what is kept, with as
/// much again either side, the same as `around` eases to.
fn reach(draft: &Draft) -> Option<(f64, f64)> {
    let total = draft.measure.length?;
    let (start, end) = kept(draft)?;
    let margin = (end - start).max(5.0);
    Some(((start - margin).max(0.0), (end + margin).min(total)))
}

/// Where the part kept starts and ends, once the source's length is known.
/// No From is the start of the video and no To is its end.
fn kept(draft: &Draft) -> Option<(f64, f64)> {
    let total = draft.measure.length?;
    let start = super::seconds(&draft.from).unwrap_or(0.0).clamp(0.0, total);
    let end = super::seconds(&draft.to)
        .unwrap_or(total)
        .clamp(start, total);
    Some((start, end))
}

/// Says which of the clip's programs aren't installed.
fn missing(ui: &mut Ui, tools: Tools, link: bool) {
    for (needed, missing) in [
        (
            link && !tools.ytdlp,
            "yt-dlp isn't installed, so a clip can't come from a link. Install it with: winget install yt-dlp.yt-dlp",
        ),
        (
            !tools.ffmpeg,
            "ffmpeg isn't installed, so a clip can't be watched or cut. Install it with: winget install Gyan.FFmpeg",
        ),
    ] {
        if needed {
            notice(ui, missing, colour::WARN);
        }
    }
}

/// Whether to ask for a file of the source to play: once its length is
/// known, if the programs that get one are there.
const fn watch_now(draft: &Draft, tools: Tools, link: bool) -> bool {
    let measure = &draft.measure;
    measure.length.is_some()
        && measure.file.is_none()
        && !measure.fetching
        && measure.failed.is_none()
        && tools.ffmpeg
        && (!link || tools.ytdlp)
}

/// Whether to ask how long the source runs now: when it is new, Load Video
/// has asked for it, and the program that can tell is installed.
fn measure_now(draft: &Draft, tools: Tools) -> bool {
    let source = draft.source.trim();
    if !draft.measure.wanted
        || source.is_empty()
        || source == draft.measure.of
        || draft.measure.asking
    {
        return false;
    }
    let link = source.starts_with("http");
    !((link && !tools.ytdlp) || (!link && !tools.ffmpeg))
}

/// The part of the video kept, as a bar with a handle at each end and the
/// playhead while it plays. Dragging a handle writes From or To in tenths
/// of a second, and typing in either moves its handle. With nothing typed
/// the whole video is kept. Returns the time of a handle being dragged.
fn trim(ui: &mut Ui, draft: &mut Draft, playhead: Option<f64>) -> Option<f64> {
    let (total, (mut start, mut end)) = draft.measure.length.zip(kept(draft))?;
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
    let track = Rect::from_min_max(
        pos2(rect.left() + GRIP, rect.top() + 16.0),
        pos2(rect.right() - GRIP, rect.top() + 32.0),
    );
    // Held still while a handle is dragged, so the scale doesn't shift under it.
    let span = draft
        .measure
        .held
        .unwrap_or_else(|| around(ui, start, end, total));
    let x_of = |t: f64| scale(track, span, t);
    let t_of = |x: f32| {
        let share = f64::from(((x - track.left()) / track.width()).clamp(0.0, 1.0));
        ((span.1 - span.0).mul_add(share, span.0) * 10.0).round() / 10.0
    };
    let mut held = false;
    let mut dragged = None;
    for left in [true, false] {
        let at = x_of(if left { start } else { end });
        // Each grip reaches outward, so two handles close together are both
        // still there to take.
        let (from, to) = if left {
            (2.0f32.mul_add(-GRIP, at), at + 3.0)
        } else {
            (at - 3.0, 2.0f32.mul_add(GRIP, at))
        };
        let grip = Rect::from_x_y_ranges(from..=to, track.top() - 7.0..=track.bottom() + 7.0);
        let response = ui
            .interact(grip, ui.id().with(("trim", left)), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        held |= response.dragged();
        if response.dragged()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let to = t_of(pointer.x);
            if left {
                start = to.min(end - SHORTEST).max(0.0);
                draft.from = super::clock(start);
                dragged = Some(start);
            } else {
                end = to.max(start + SHORTEST).min(total);
                draft.to = super::clock(end);
                dragged = Some(end);
            }
        }
    }
    draft.measure.held = held.then_some(span);
    if ui.is_rect_visible(rect) {
        bar(ui.painter(), (rect, track), total, span, (start, end));
        if let Some(at) = playhead.filter(|t| (span.0..=span.1).contains(t)) {
            let x = scale(track, span, at);
            let line =
                Rect::from_x_y_ranges(x - 1.0..=x + 1.0, track.top() - 3.0..=track.bottom() + 3.0);
            ui.painter().rect_filled(line, 0, colour::TEXT_STRONG);
        }
    }
    dragged
}

/// The part of the video the bar spans: what is kept, with as much again
/// either side, so a short clip out of a long video still gets the width to
/// be cut to a tenth of a second. Eases there, so the bar zooms rather than
/// jumps once a handle is let go.
fn around(ui: &Ui, start: f64, end: f64, total: f64) -> (f64, f64) {
    let margin = (end - start).max(5.0);
    let ease = |name: &str, to: f64| {
        f64::from(
            ui.ctx()
                .animate_value_with_time(ui.id().with(name), to as f32, ZOOM),
        )
    };
    let lo = ease("trim-lo", (start - margin).max(0.0));
    let hi = ease("trim-hi", (end + margin).min(total));
    (lo, hi.max(lo + 0.1))
}

/// Where `t` seconds falls on `track` when it spans `span`.
fn scale(track: Rect, span: (f64, f64), t: f64) -> f32 {
    let share = ((t - span.0) / (span.1 - span.0)).clamp(0.0, 1.0);
    track.width().mul_add(share as f32, track.left())
}

/// Paints the trim bar: a strip for the whole video with the part the bar
/// spans marked on it, the bar with what is kept and its two handles, and
/// the times under it.
fn bar(
    painter: &egui::Painter,
    (rect, track): (Rect, Rect),
    total: f64,
    span: (f64, f64),
    (start, end): (f64, f64),
) {
    if span.1 - span.0 < total - 0.05 {
        let strip = Rect::from_x_y_ranges(track.x_range(), rect.top() + 5.0..=rect.top() + 8.0);
        let whole = |t: f64| scale(strip, (0.0, total), t);
        painter.rect_filled(strip, 0, colour::BG_INSET);
        let shown = Rect::from_x_y_ranges(whole(span.0)..=whole(span.1), strip.y_range());
        painter.rect_filled(shown, 0, colour::TEXT_FAINT);
        let kept = Rect::from_x_y_ranges(
            whole(start)..=whole(end).max(whole(start) + 1.0),
            strip.y_range(),
        );
        painter.rect_filled(kept, 0, colour::ENEMY);
    }
    painter.rect_filled(track, 0, colour::BG_INSET);
    let kept = Rect::from_x_y_ranges(
        scale(track, span, start)..=scale(track, span, end),
        track.y_range(),
    );
    painter.rect_filled(kept, 0, colour::ENEMY.gamma_multiply(0.7));
    for at in [kept.left(), kept.right()] {
        let handle =
            Rect::from_center_size(pos2(at, track.center().y), vec2(4.0, track.height() + 10.0));
        painter.rect_filled(handle, 0, colour::TEXT_STRONG);
    }
    let length = end - start;
    let (said, tint) = if length > LONGEST {
        (
            format!(
                "Keeps {}, over the {} limit",
                super::clock(length),
                super::clock(LONGEST)
            ),
            colour::WARN,
        )
    } else {
        (format!("Keeps {length:.1} seconds"), colour::TEXT_DIM)
    };
    let y = track.bottom() + 14.0;
    let font = crate::board::paint::label();
    for (x, align, text, tint) in [
        (
            track.left(),
            Align2::LEFT_CENTER,
            super::clock(span.0),
            colour::TEXT_FAINT,
        ),
        (track.center().x, Align2::CENTER_CENTER, said, tint),
        (
            track.right(),
            Align2::RIGHT_CENTER,
            super::clock(span.1),
            colour::TEXT_FAINT,
        ),
    ] {
        let _drawn = caps_text(painter, pos2(x, y), align, &text, font.clone(), tint);
    }
}

/// A small caps label beside the box it names.
fn caption(ui: &mut Ui, text: &str) {
    let font = crate::board::paint::label();
    let wide = overseer_ui::caps_width(ui.painter(), text, font.clone());
    let (rect, _response) = ui.allocate_exact_size(vec2(wide, 24.0), Sense::hover());
    let _drawn = caps_text(
        ui.painter(),
        pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font,
        colour::TEXT_FAINT,
    );
}
