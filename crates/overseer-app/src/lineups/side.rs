//! The panel beside the map: the lineups on it and the one picked, and
//! the frame the lineup form and the drawing tools sit in.

use egui::{Align2, Color32, Panel, Rect, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_core::{Atlas, Lineup};
use overseer_ui::{Face, caps_text, colour, motion, size, space};

use super::{
    Draft, Export, Job, Mode, Request, View, face, form, length, pictures, player, sketch,
};
use crate::controls::{self, Tone};

/// How tall a lineup's line in the list is.
const ROW: f32 = 46.0;

/// Draws the panel for whatever it is doing, and says what it asked for.
pub(super) fn show(ui: &mut Ui, atlas: &Atlas, view: &mut View) -> Option<Request> {
    let mut asked = None;
    if matches!(view.mode, Mode::Edit(_)) {
        Panel::bottom("lineup-footer")
            .frame(
                egui::Frame::NONE
                    .fill(colour::BG_RAISED)
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ui, |ui| {
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    ui.max_rect().top(),
                    (1.0, colour::LINE),
                );
                ui.spacing_mut().item_spacing = vec2(space::SM, space::SM);
                asked = form::footer(ui, view);
            });
    }
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(16, 16))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(space::SM, space::SM);
                    // Comes in again for each map and each thing the panel
                    // does, rather than swapping in place.
                    let doing = match view.mode {
                        Mode::Browse => "browse",
                        Mode::Edit(_) => "edit",
                        Mode::Draw(_) => "draw",
                    };
                    let id = egui::Id::new(("lineups-side", doing, view.map.clone()));
                    asked = motion::arrive(ui, id, 0.0, |ui| match view.mode {
                        Mode::Browse => browse(ui, atlas, view),
                        Mode::Edit(_) => form::body(ui, atlas, view).or(asked),
                        Mode::Draw(_) => {
                            sketch::show(ui, view);
                            None
                        }
                    });
                    said(ui, view);
                });
        });
    asked
}

/// A heading in the heavy italic and a line under it.
pub(super) fn title(ui: &mut Ui, text: &str, under: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let _title = caps_text(
        ui.painter(),
        pos2(rect.left(), rect.top() + 14.0),
        Align2::LEFT_CENTER,
        text,
        Face::Heavy.at(22.0),
        colour::TEXT_STRONG,
    );
    let _under = caps_text(
        ui.painter(),
        pos2(rect.left() + 1.0, rect.top() + 36.0),
        Align2::LEFT_CENTER,
        under,
        Face::Display.at(size::MICRO),
        colour::TEXT_DIM,
    );
}

/// A sentence wrapped to the panel.
pub(super) fn words(ui: &mut Ui, text: &str, tint: Color32) {
    let galley = ui.painter().layout(
        text.to_owned(),
        Face::Body.at(size::MICRO + 1.0),
        tint,
        ui.available_width(),
    );
    let (rect, _response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, tint);
}

/// A status line: a short rail in `tint`, whose colour says what kind it
/// is, beside the words in the panel's own colour so they stay easy to read.
pub(super) fn notice(ui: &mut Ui, text: &str, tint: Color32) {
    let indent = space::MD + 3.0;
    let galley = ui.painter().layout(
        text.to_owned(),
        Face::Body.at(size::MICRO + 1.0),
        colour::TEXT,
        ui.available_width() - indent,
    );
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), galley.size().y + 4.0),
        Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
        0,
        tint,
    );
    painter.galley(rect.min + vec2(indent, 2.0), galley, colour::TEXT);
}

/// What the last request came to.
fn said(ui: &mut Ui, view: &View) {
    let Some((text, failed)) = &view.said else {
        return;
    };
    ui.add_space(space::MD);
    notice(ui, text, if *failed { colour::BAD } else { colour::ALLY });
}

/// The lineups on the map, a filter by agent, and the one picked.
fn browse(ui: &mut Ui, atlas: &Atlas, view: &mut View) -> Option<Request> {
    let map = view.map.clone().unwrap_or_default();
    let here: Vec<&Lineup> = atlas.lineups.iter().filter(|l| l.map == map).collect();
    let count = match here.len() {
        0 => "No lineups yet".to_owned(),
        1 => "1 lineup".to_owned(),
        n => format!("{n} lineups"),
    };
    title(ui, &map, &count);
    // One row of three, the width split evenly.
    let shared = ui.horizontal(|ui| {
        let gap = ui.spacing().item_spacing.x;
        let wide = gap.mul_add(-2.0, ui.available_width()) / 3.0;
        if controls::button_wide(ui, "Add Lineup", (Tone::Primary, true), wide).clicked() {
            view.mode = Mode::Edit(Box::new(view.draft()));
            view.said = None;
        }
        if controls::button_wide(ui, "Draw", (Tone::Plain, true), wide).clicked() {
            view.mode = Mode::Draw(Box::new(view.sketch(atlas)));
            view.said = None;
        }
        exports(ui, view, wide)
    });
    let shared = shared.inner;
    if here.is_empty() {
        ui.add_space(space::MD);
        for step in [
            "1. Press Add Lineup.",
            "2. Drag on the map from where you stand to where it lands.",
            "3. Pick the agent and the ability, like Brimstone's Incendiary.",
            "4. Add a clip of the throw if you like, then save.",
        ] {
            words(ui, step, colour::TEXT_DIM);
        }
        return shared;
    }
    // Nothing picked lights every lineup on the map. One the filter has
    // hidden isn't picked any more.
    let shown = view.shown(atlas);
    if !shown
        .iter()
        .any(|l| l.id.is_some() && l.id == view.selected)
    {
        view.selected = None;
    }
    let hint = if shown.is_empty() {
        "No lineup matches the filter"
    } else {
        "Pick a lineup, here or on the map"
    };
    picker(ui, atlas, (&here, hint), view);
    let picked = atlas
        .lineups
        .iter()
        .find(|l| l.id.is_some() && l.id == view.selected && l.map == map);
    let Some(picked) = picked else {
        return shared;
    };
    let id = egui::Id::new(("lineup-details", picked.id.clone()));
    motion::arrive(ui, id, 0.0, |ui| details(ui, picked, view)).or(shared)
}

/// Saving the map as a picture or copying it to paste into a chat, and
/// sharing the map's lineups as a code.
fn exports(ui: &mut Ui, view: &mut View, wide: f32) -> Option<Request> {
    let busy = view.export.is_some();
    let mut asked = None;
    let button = controls::button_wide(ui, "Share", (Tone::Plain, !busy), wide);
    let _menu = egui::Popup::menu(&button)
        .align(egui::RectAlign::BOTTOM_END)
        .gap(4.0)
        .frame(controls::menu_card())
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if controls::menu_row(ui, "Save Picture", false, 150.0).clicked() {
                view.export = Some(Export::Save);
                view.said = None;
                ui.close();
            }
            if controls::menu_row(ui, "Copy Picture", false, 150.0).clicked() {
                view.export = Some(Export::Copy);
                view.said = None;
                ui.close();
            }
            if controls::menu_row(ui, "Copy Code", false, 150.0).clicked() {
                asked = Some(Request::ShareMap);
                ui.close();
            }
            if controls::menu_row(ui, "Paste Code", false, 150.0).clicked() {
                // The clipboard's text comes back as a paste a frame later.
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                view.pasting = Some(ui.ctx().cumulative_pass_nr());
                view.said = None;
                ui.close();
            }
            if let Some(folder) = view.saved_to.clone()
                && controls::menu_row(ui, "Open Folder", false, 150.0).clicked()
            {
                // Explorer opens the folder by itself, and nothing waits on it.
                drop(std::process::Command::new("explorer").arg(folder).spawn());
                ui.close();
            }
        });
    asked
}

/// A box that narrows the list to lineups with those words, and a chip that
/// goes round the orders, on one quiet line under the heading.
fn ordering(ui: &mut Ui, view: &mut View) {
    ui.horizontal(|ui| {
        let label = view.order.label();
        let wide = ui.available_width() - controls::chip_width(ui, label) - space::SM;
        let _typed = controls::field(ui, &mut view.looking_for, "Filter", wide);
        if controls::chip(ui, label, false).clicked() {
            view.order = view.order.next();
        }
    });
}

/// The picked lineup as a row with an arrow, which drops the whole list
/// down under it to pick another from, with the filter, the order and the
/// agent chips at the top of it.
fn picker(ui: &mut Ui, atlas: &Atlas, (here, hint): (&[&Lineup], &str), view: &mut View) {
    ui.add_space(space::SM);
    let main = view.main.clone();
    let id = ui.id().with("lineup-picker");
    let open = egui::Popup::is_id_open(ui.ctx(), id);
    let picked = atlas
        .lineups
        .iter()
        .find(|l| l.id.is_some() && l.id == view.selected);
    let response = if let Some(lineup) = picked {
        row(ui, atlas, lineup, (open, main.as_deref()))
    } else {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
        ui.painter().rect_filled(rect, 0, colour::BG_INSET);
        ui.painter().text(
            rect.left_center() + vec2(space::LG, 0.0),
            Align2::LEFT_CENTER,
            hint,
            Face::Body.at(size::BODY),
            colour::TEXT_DIM,
        );
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    };
    arrow(ui.painter(), response.rect, open || response.hovered());
    let wide = response.rect.width();
    let _list = egui::Popup::menu(&response)
        .id(id)
        .width(wide)
        .align(egui::RectAlign::BOTTOM_START)
        .gap(2.0)
        .frame(controls::menu_card())
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(wide - 8.0);
            filter(ui, here, view);
            if here.len() > 1 {
                ordering(ui, view);
            }
            let tallest = ui.ctx().content_rect().height() * 0.6;
            let list = view.shown(atlas);
            ScrollArea::vertical().max_height(tallest).show(ui, |ui| {
                for lineup in list {
                    let on = lineup.id.is_some() && lineup.id == view.selected;
                    if row(ui, atlas, lineup, (on, main.as_deref())).clicked() {
                        view.selected.clone_from(&lineup.id);
                        view.confirm = false;
                        ui.close();
                    }
                }
            });
        });
}

/// The arrow at the right of the picker, pointing down to say it opens.
fn arrow(painter: &egui::Painter, rect: Rect, hot: bool) {
    let c = pos2(rect.right() - space::XL, rect.center().y);
    let ink = if hot {
        colour::TEXT_STRONG
    } else {
        colour::TEXT_DIM
    };
    let stroke = egui::Stroke::new(2.0, ink);
    painter.line_segment([c + vec2(-6.0, -3.0), c + vec2(0.0, 3.0)], stroke);
    painter.line_segment([c + vec2(0.0, 3.0), c + vec2(6.0, -3.0)], stroke);
}

/// A chip per agent with lineups here, when there is more than one.
fn filter(ui: &mut Ui, here: &[&Lineup], view: &mut View) {
    let mut agents: Vec<&str> = here.iter().map(|l| l.agent.as_str()).collect();
    agents.sort_unstable();
    agents.dedup();
    if view.agent.as_deref().is_some_and(|a| !agents.contains(&a)) {
        view.agent = None;
    }
    if agents.len() < 2 {
        return;
    }
    controls::heading(ui, "Agent");
    ui.horizontal_wrapped(|ui| {
        if controls::chip(ui, "All", view.agent.is_none()).clicked() {
            view.agent = None;
        }
        for agent in agents {
            if controls::chip(ui, agent, view.agent.as_deref() == Some(agent)).clicked() {
                view.agent = Some(agent.to_owned());
            }
        }
    });
}

/// One lineup in the list: the agent, the title, and what it is for.
pub(super) fn row(
    ui: &mut Ui,
    atlas: &Atlas,
    lineup: &Lineup,
    (picked, main): (bool, Option<&str>),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    if picked {
        painter.rect_filled(rect, 0, colour::BG_SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0, colour::BG_HOVER);
    }
    let rail = if lineup.side.as_deref() == Some("defense") {
        colour::INFO
    } else {
        colour::ENEMY
    };
    painter.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, ROW)), 0, rail);
    let portrait = Rect::from_min_size(rect.min + vec2(8.0, 5.0), vec2(36.0, 36.0));
    face(painter, portrait, &lineup.agent, 1.0, main);
    let text_at = portrait.right() + space::LG;
    let font = Face::Display.at(size::TITLE);
    let _title = caps_text(
        painter,
        pos2(text_at, rect.top() + 16.0),
        Align2::LEFT_CENTER,
        &crate::board::fit(
            painter,
            &lineup.title,
            &font,
            rect.right() - text_at - space::MD,
        ),
        font.clone(),
        colour::TEXT_STRONG,
    );
    let _about = caps_text(
        painter,
        pos2(text_at, rect.top() + 33.0),
        Align2::LEFT_CENTER,
        &about(atlas, lineup),
        Face::Display.at(size::MICRO),
        colour::TEXT_DIM,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Asks for a wider panel when the picked lineup's clip, `clip` times as
/// wide as it is tall or 0 with none, or one of its pictures is wider than it
/// is tall and the panel is too narrow to show it at its tallest. Once a pick.
fn make_room(ui: &Ui, lineup: &Lineup, view: &mut View, clip: f32) {
    if view.widened == lineup.id {
        return;
    }
    view.widened.clone_from(&lineup.id);
    let tall = ui.ctx().content_rect().height();
    let pictures = lineup
        .images
        .iter()
        .filter_map(|p| pictures::aspect(ui.ctx(), p))
        .fold(0.0, f32::max)
        * tall
        * pictures::TALLEST;
    let clip = clip * tall * view.prefs.size;
    let more = pictures.max(clip) - ui.available_width();
    view.widen = (more > 1.0).then_some(more);
}

/// What a lineup is for, in a line: the ability and its key, the side and
/// the site.
pub(super) fn about(atlas: &Atlas, lineup: &Lineup) -> String {
    let mut parts = Vec::new();
    if let Some(name) = lineup.ability.as_deref() {
        let key = super::ability(atlas, &lineup.agent, name).map(|(_, a)| a.key.clone());
        parts.push(key.map_or_else(|| name.to_owned(), |k| format!("{k} {name}")));
    }
    match lineup.side.as_deref() {
        Some("attack") => parts.push("Attacking".to_owned()),
        Some("defense") => parts.push("Defending".to_owned()),
        _ => {}
    }
    if let Some(site) = lineup.site.as_deref() {
        parts.push(format!("{site} Site"));
    }
    parts.join("  \u{b7}  ")
}

/// The picked lineup: what can be done with it, then its clip, its pictures
/// and its words, in that order, so the clip is always on screen whole.
fn details(ui: &mut Ui, lineup: &Lineup, view: &mut View) -> Option<Request> {
    let busy = view.pending.is_some();
    let mut asked = None;
    ui.add_space(space::SM);
    ui.horizontal_wrapped(|ui| {
        if controls::button(ui, "Edit", Tone::Plain, !busy).clicked() {
            view.mode = Mode::Edit(Box::new(Draft::of(lineup)));
            view.said = None;
        }
        if controls::button(ui, "Copy Code", Tone::Plain, !busy).clicked() {
            asked = Some(Request::Share);
        }
        let delete = if view.confirm {
            "Click Again to Delete"
        } else {
            "Delete"
        };
        if controls::button(ui, delete, Tone::Danger, !busy).clicked() {
            if view.confirm {
                asked = Some(Request::Delete);
            }
            view.confirm = !view.confirm;
        }
    });
    ui.add_space(space::SM);
    let clip = lineup.clip.as_ref();
    // The cut clip already has its volume in it, so it plays as it is.
    // Windows won't let the backend replace or delete a file a player has
    // open, so the clip stays off while it is cut again or thrown away.
    let cutting = view
        .saving
        .as_ref()
        .is_some_and(|d| d.lineup.id == lineup.id);
    let deleting = matches!(view.pending, Some((_, Job::Delete)));
    let file = clip
        .and_then(|c| c.file.as_deref())
        .filter(|_| !cutting && !deleting);
    let runs = clip.and_then(length).unwrap_or(form::LONGEST);
    let mut shape = Some(0.0);
    if let Some(file) = file {
        let player = player::of(&mut view.player, file, 0.0, view.prefs);
        player.show(ui, (0.0, runs), 100.0, 0.0);
        shape = player.shape();
        ui.add_space(space::SM);
    }
    // Not before the clip's shape is known.
    if let Some(shape) = shape {
        make_room(ui, lineup, view, shape);
    }
    if let Some(at) = pictures::column(ui, &lineup.images) {
        view.viewing = Some((lineup.images.clone(), at));
    }
    if let Some(notes) = lineup.notes.as_deref() {
        words(ui, &notes.to_uppercase(), colour::TEXT);
    }
    if let Some(description) = lineup.description.as_deref() {
        words(ui, description, colour::TEXT_DIM);
    }
    if file.is_none() {
        // A pasted lineup whose clip couldn't be cut still has its link.
        let uncut = clip.is_some_and(|c| c.source.is_some() && c.file.is_none());
        let queued = lineup
            .id
            .as_ref()
            .is_some_and(|id| view.cutting.contains(id));
        let hint = if cutting {
            "Cutting the new clip."
        } else if deleting {
            "Deleting the lineup."
        } else if queued {
            "Its clip is being cut, and shows here once it's ready."
        } else if uncut {
            "The clip isn't cut yet. Edit the lineup and save it to cut it."
        } else {
            "No clip yet. Edit it to add one."
        };
        words(ui, hint, colour::TEXT_DIM);
    }
    asked
}
