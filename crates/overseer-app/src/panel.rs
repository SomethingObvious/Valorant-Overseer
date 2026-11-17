use egui::{Align2, Color32, Rect, RichText, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_core::Player;

use crate::board::{self, Side};
use crate::career::{self, Career};
use crate::notes::{self, Notes};
use overseer_ui::{
    Face, art, caps, caps_at, caps_text, caps_width, colour, hex, motion, rank, shape, size, space,
};

const VALUE_X: f32 = 60.0;

pub(crate) fn show(
    ui: &mut Ui,
    player: Option<&Player>,
    side: Side,
    store: &mut Notes,
    career: &Career,
    still: bool,
) -> bool {
    let Some(player) = player else {
        heading(ui, "No One Selected");
        line(
            ui,
            "Click a row, or use the arrow keys.",
            colour::TEXT_DIM,
            size::BODY,
        );
        return false;
    };
    let mut save = false;
    let two = ui.available_width() >= TWO_COLUMNS;
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(space::LG);
            if two {
                ui.columns(2, |columns| {
                    if let [left, right] = columns {
                        save = about(left, player, side, store, still);
                        lately(right, player, side, career);
                    }
                });
            } else {
                save = about(ui, player, side, store, still);
                lately(ui, player, side, career);
            }
            ui.add_space(space::XL);
        });
    save
}

const TWO_COLUMNS: f32 = 560.0;

fn about(ui: &mut Ui, player: &Player, side: Side, store: &mut Notes, still: bool) -> bool {
    card(ui, player);
    if !player.smurf_reasons.is_empty() {
        verdict(ui, player);
    }
    reads(ui, player, side);
    let save = notes(ui, player, store, side);
    ranks(ui, player, side, still);
    form(ui, player, side);
    mains(ui, player);
    numbers(ui, player, side);
    brief(ui, player);
    group(ui, player);
    met(ui, player);
    save
}

fn lately(ui: &mut Ui, player: &Player, side: Side, career: &Career) {
    if let Some(puuid) = player.puuid.as_deref() {
        career::show(ui, career, puuid, side);
    }
    loadout(ui, player);
}

fn notes(ui: &mut Ui, player: &Player, store: &mut Notes, side: Side) -> bool {
    let Some(id) = player.puuid.as_deref() else {
        return false;
    };
    let opened = egui::Id::new(("note-open", id));
    let mut note = store.get(id);
    if note.is_empty() && !ui.data(|d| d.get_temp::<bool>(opened)).unwrap_or(false) {
        if invite(ui).clicked() {
            ui.data_mut(|d| d.insert_temp(opened, true));
        }
        return false;
    }
    subheading(ui, "Your Notes", colour::TEXT_DIM);
    let mut text = note.text.clone();
    // The tag line keeps its raw text in egui's scratch space while it has
    // focus, because tidying "toxic," into "toxic" under the cursor eats the
    // comma that was just typed.
    let key = egui::Id::new(("tags", id));
    let mut tags = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| notes::tags_line(&note.tags));
    let (mut touched, mut done) = (false, false);
    ui.horizontal(|ui| {
        ui.add_space(space::LG);
        // To the same edge as the heading's rule, measured from where the
        // box starts rather than from what is left after the spacing.
        let width = (ui.max_rect().right() - space::LG - ui.cursor().left()).max(120.0);
        ui.vertical(|ui| {
            let prose = inset(
                ui,
                egui::TextEdit::multiline(&mut text)
                    .id(egui::Id::new(("note-text", id)))
                    .desired_width(width)
                    .desired_rows(2)
                    .font(Face::Body.at(size::BODY))
                    .hint_text(hint("What you want to remember about them")),
            );
            ui.add_space(space::SM);
            let labels = inset(
                ui,
                egui::TextEdit::singleline(&mut tags)
                    .id(egui::Id::new(("note-tags", id)))
                    .desired_width(width)
                    .font(Face::Body.at(size::MICRO))
                    .hint_text(hint("Tags, with commas. The first shows on their row")),
            );
            let have = notes::parse_tags(&tags);
            let picked = offers(ui, player, store, (&have, side));
            // A pick is saved at once: the box it lands in may never have
            // had the keyboard, so it would never lose it and never save.
            let picked_one = picked.is_some();
            if let Some(tag) = picked {
                let mut all = have;
                all.push(tag);
                tags = notes::tags_line(&all);
            }
            touched = prose.changed() || labels.changed() || picked_one;
            done = prose.lost_focus() || labels.lost_focus() || picked_one;
        });
    });
    if touched || done {
        note.text = text;
        note.tags = notes::parse_tags(&tags);
        player.display_name().clone_into(&mut note.name);
        store.set(id, note);
    }
    if done {
        ui.data_mut(|d| d.remove::<String>(key));
    } else if touched {
        ui.data_mut(|d| d.insert_temp(key, tags));
    }
    done
}

fn inset(ui: &mut Ui, edit: egui::TextEdit<'_>) -> egui::Response {
    let ground = ui.painter().add(egui::Shape::Noop);
    let response =
        ui.add(edit.frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6))));
    let rect = response.rect;
    ui.painter()
        .set(ground, egui::Shape::rect_filled(rect, 0, colour::BG_INSET));
    if response.has_focus() {
        ui.painter().rect_filled(
            Rect::from_min_size(
                pos2(rect.left(), rect.bottom() - 2.0),
                vec2(rect.width(), 2.0),
            ),
            0,
            colour::TEXT_STRONG,
        );
    }
    response
}

fn offers(
    ui: &mut Ui,
    player: &Player,
    store: &Notes,
    (have, side): (&[String], Side),
) -> Option<String> {
    let offers: Vec<String> = player
        .auto_tags
        .iter()
        .filter_map(|t| t.tag.clone())
        .chain(store.common_tags(6))
        .filter(|t| !have.contains(t))
        .fold(Vec::new(), |mut out, t| {
            if !out.contains(&t) && out.len() < 8 {
                out.push(t);
            }
            out
        });
    let mut picked = None;
    if !offers.is_empty() {
        ui.add_space(space::SM);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(space::SM, space::SM);
            for offer in &offers {
                let read = player
                    .auto_tags
                    .iter()
                    .find(|t| t.tag.as_ref() == Some(offer));
                if suggestion(ui, offer, read, side).clicked() {
                    picked = Some(offer.clone());
                }
            }
        });
    }
    picked
}

fn suggestion(
    ui: &mut Ui,
    tag: &str,
    read: Option<&overseer_core::AutoTag>,
    side: Side,
) -> egui::Response {
    let font = Face::Display.at(size::MICRO);
    let label = format!("+ {tag}");
    let wide = 2.0f32.mul_add(space::MD, caps_width(ui.painter(), &label, font.clone()));
    let (rect, response) = ui.allocate_exact_size(vec2(wide, 18.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Add the tag {tag}"))
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() {
            painter.add(board::paint::slant(rect, false, true, colour::BG_HOVER));
        }
        board::paint::chip_outline(painter, rect, colour::TEXT_FAINT);
        let _drawn = caps_text(
            painter,
            pos2(rect.left() + space::MD - 1.0, rect.center().y),
            Align2::LEFT_CENTER,
            &label,
            font,
            if response.hovered() {
                colour::TEXT_STRONG
            } else {
                colour::TEXT_DIM
            },
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    match read {
        Some(read) => {
            response.on_hover_ui(|ui| board::tip::auto(ui, read, board::paint::win(side)))
        }
        None => response.on_hover_text("One you have used before. A click adds it."),
    }
}

pub(crate) fn write_note(ctx: &egui::Context, id: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(("note-open", id)), true));
    ctx.memory_mut(|m| m.request_focus(egui::Id::new(("note-text", id))));
}

fn invite(ui: &mut Ui) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Write a note"));
    if ui.is_rect_visible(rect) {
        let tint = if response.hovered() {
            colour::TEXT_STRONG
        } else {
            colour::TEXT_DIM
        };
        ui.painter().text(
            pos2(rect.left() + space::LG, rect.center().y),
            Align2::LEFT_CENTER,
            "+ Write a Note",
            Face::Body.at(size::MICRO),
            tint,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn figures_toggle(ui: &mut Ui, id: egui::Id, count: usize) -> bool {
    let open = ui.data(|store| store.get_temp::<bool>(id)).unwrap_or(false);
    ui.add_space(space::MD);
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, open, "All Figures")
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let slab = rect.shrink2(vec2(space::LG, 0.0));
        let ground = if response.hovered() {
            colour::BG_HOVER
        } else {
            colour::BG_RAISED
        };
        painter.rect_filled(slab, 0, ground);
        let tint = if response.hovered() || open {
            colour::TEXT_STRONG
        } else {
            colour::TEXT
        };
        let drawn = caps_text(
            painter,
            pos2(slab.left() + space::LG, slab.center().y),
            Align2::LEFT_CENTER,
            "All Figures",
            Face::Display.at(size::LABEL),
            tint,
        );
        let _count = caps_text(
            painter,
            pos2(drawn.right() + space::MD, slab.center().y),
            Align2::LEFT_CENTER,
            &count.to_string(),
            Face::Heavy.at(size::LABEL),
            colour::TEXT_FAINT,
        );
        crate::history::chevron(
            painter,
            pos2(slab.right() - space::LG - 5.0, slab.center().y),
            open,
            tint,
        );
    }
    let open = open
        ^ response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked();
    ui.data_mut(|store| store.insert_temp(id, open));
    if open {
        ui.add_space(space::SM);
    }
    open
}

fn hint(text: &str) -> RichText {
    RichText::new(text)
        .color(colour::HINT)
        .font(Face::Body.at(size::MICRO))
}

const CARD: f32 = 120.0;

/// A click on the card copies the Riot ID, the same as Ctrl+C.
fn copies(ui: &Ui, response: egui::Response, player: &Player) {
    let Some(name) = player.name.as_deref() else {
        return;
    };
    crate::controls::named(&response, &format!("Copy {name}"));
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Copy their Riot ID");
    if response.clicked() {
        copy(ui.ctx(), name.to_owned());
    }
}

fn card(ui: &mut Ui, player: &Player) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), CARD), Sense::click());
    copies(ui, response, player);
    if !ui.is_rect_visible(rect) {
        return;
    }
    let card = Rect::from_min_max(
        pos2(rect.left() + space::LG, rect.top()),
        pos2(rect.right() - space::LG, rect.bottom()),
    );
    let painter = ui.painter().clone();
    let tint = hex(player.agent_color.as_deref()).unwrap_or(colour::BG_INSET);
    painter.rect_filled(card, 0, shape::blend(tint, colour::BG, 0.55));
    let named = player.agent.as_deref().unwrap_or("");
    if let Some(face) = art::card(ui.ctx(), named).or_else(|| art::killfeed(ui.ctx(), named)) {
        let texture = face.size_vec2();
        let shown = (card.height() / card.width()) * (texture.x / texture.y.max(1.0));
        let mut mesh = egui::Mesh::with_texture(face.id());
        mesh.add_rect_with_uv(
            card,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, shown.clamp(0.0, 1.0))),
            Color32::WHITE,
        );
        painter.add(egui::Shape::mesh(mesh));
    } else if let Some(banner) = board::paint::banner(ui.ctx(), player) {
        let texture = banner.size_vec2();
        let shown = (card.width() / card.height()) / (texture.x / texture.y.max(1.0));
        let (left, right) = if shown < 1.0 {
            (0.5 - shown / 2.0, 0.5 + shown / 2.0)
        } else {
            (0.0, 1.0)
        };
        let mut mesh = egui::Mesh::with_texture(banner.id());
        mesh.add_rect_with_uv(
            card,
            Rect::from_min_max(pos2(left, 0.0), pos2(right, 1.0)),
            Color32::from_gray(170),
        );
        painter.add(egui::Shape::mesh(mesh));
    }
    let mut fade = egui::Mesh::default();
    let dark = colour::BG.gamma_multiply_u8(235);
    for (x, y, colour) in [
        (
            card.left(),
            card.height().mul_add(0.35, card.top()),
            Color32::TRANSPARENT,
        ),
        (
            card.right(),
            card.height().mul_add(0.35, card.top()),
            Color32::TRANSPARENT,
        ),
        (card.right(), card.bottom(), dark),
        (card.left(), card.bottom(), dark),
    ] {
        fade.colored_vertex(pos2(x, y), colour);
    }
    fade.add_triangle(0, 1, 2);
    fade.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(fade));
    if player.smurf {
        painter.rect_filled(
            Rect::from_min_size(card.min, vec2(4.0, card.height())),
            0,
            colour::WARN,
        );
    }
    card_words(&painter, card, player);
}

fn card_words(painter: &egui::Painter, card: Rect, player: &Player) {
    let (name, tag, _standing_in) = board::shown_name(player);
    let room = 2.0f32.mul_add(-space::LG, card.width());
    let font = Face::Heavy.at(size::HERO);
    let drawn = caps_text(
        painter,
        pos2(card.left() + space::LG, card.bottom() - 34.0),
        Align2::LEFT_CENTER,
        &board::fit(painter, name, &font, room),
        font,
        colour::TEXT_STRONG,
    );
    if player.hidden.name {
        let _hidden = caps_text(
            painter,
            pos2(drawn.right() + space::MD, card.bottom() - 32.0),
            Align2::LEFT_CENTER,
            "Hidden",
            Face::Display.at(size::LABEL),
            colour::TEXT_FAINT,
        );
    }
    let mut about: Vec<String> = Vec::new();
    if !tag.is_empty() {
        about.push(format!("#{tag}"));
    }
    if let Some(level) = player.level {
        about.push(if player.hidden.level && level > 0 {
            format!("Hidden level {level}")
        } else if player.hidden.level {
            "Level hidden".to_owned()
        } else {
            format!("Level {level}")
        });
    }
    about.extend(player.agent.iter().cloned());
    about.extend(player.role.iter().cloned());
    let font = Face::Number.at(size::BODY);
    let mut said = about.join("  \u{b7}  ");
    while about.len() > 1 && caps_width(painter, &said, font.clone()) > room {
        about.pop();
        said = about.join("  \u{b7}  ");
    }
    let _line = caps_text(
        painter,
        pos2(card.left() + space::LG, card.bottom() - 13.0),
        Align2::LEFT_CENTER,
        &board::fit(painter, &said, &font, room),
        font,
        colour::TEXT,
    );
    if let Some(title) = player.title.as_deref() {
        let width = caps_width(painter, title, Face::Display.at(size::LABEL));
        let chip = Rect::from_min_size(
            pos2(
                card.right() - space::MD - width - space::XL,
                card.top() + space::MD,
            ),
            vec2(width + space::XL, 20.0),
        );
        painter.add(board::paint::slant(
            chip,
            true,
            false,
            colour::BG.gamma_multiply(0.85),
        ));
        let _title = caps_text(
            painter,
            pos2(chip.center().x + 2.0, chip.center().y),
            Align2::CENTER_CENTER,
            title,
            Face::Display.at(size::LABEL),
            colour::TEXT_STRONG,
        );
    }
}

fn verdict(ui: &mut Ui, player: &Player) {
    ui.add_space(space::MD);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        let band = Rect::from_min_max(
            pos2(rect.left() + space::LG, rect.top()),
            pos2(rect.right() - space::LG, rect.bottom()),
        );
        let solid = player.smurf;
        let painter = ui.painter();
        painter.add(board::paint::slant(
            band,
            false,
            true,
            if solid {
                colour::WARN
            } else {
                colour::WARN.gamma_multiply(0.18)
            },
        ));
        let _drawn = caps_text(
            painter,
            pos2(band.left() + space::LG, band.center().y),
            Align2::LEFT_CENTER,
            if solid {
                "Worth a Look"
            } else {
                "Not Enough to Flag"
            },
            Face::Heavy.at(size::TITLE),
            if solid { colour::BG } else { colour::WARN },
        );
    }
    ui.add_space(space::SM);
    if player.smurf_evidence.is_empty() {
        for reason in &player.smurf_reasons {
            reason_line(ui, reason);
        }
    } else {
        evidence(ui, &player.smurf_evidence);
    }
}

fn evidence(ui: &mut Ui, rows: &[overseer_core::Evidence]) {
    let label_font = Face::Display.at(size::LABEL);
    let spine = rows
        .iter()
        .map(|row| caps_width(ui.painter(), &row.label, label_font.clone()))
        .fold(VALUE_X, |widest, w| widest.max(w + space::LG));
    for row in rows {
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter();
        caps_at(
            painter,
            pos2(rect.left() + space::LG, rect.center().y),
            Align2::LEFT_CENTER,
            &row.label,
            label_font.clone(),
            colour::TEXT_DIM,
        );
        let value = caps_text(
            painter,
            pos2(rect.left() + space::LG + spine, rect.center().y),
            Align2::LEFT_CENTER,
            &row.value,
            Face::Heavy.at(size::TITLE),
            colour::WARN,
        );
        // In the card's own caps, so "over 18 games" sits with the number
        // the way "Level 28" sits on the card, not as a sentence beside it.
        if let Some(note) = row.note.as_deref() {
            caps_at(
                painter,
                pos2(value.right() + space::MD, rect.center().y + 1.0),
                Align2::LEFT_CENTER,
                note,
                Face::Display.at(size::LABEL),
                colour::TEXT_DIM,
            );
        }
    }
}

fn reason_line(ui: &mut Ui, reason: &str) {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = 2.0f32.mul_add(-space::LG, ui.available_width());
    for (run, numeric) in board::paint::split_numbers(reason) {
        job.append(
            run,
            0.0,
            egui::TextFormat::simple(
                Face::Body.at(size::BODY),
                if numeric {
                    colour::WARN
                } else {
                    colour::TEXT_STRONG
                },
            ),
        );
    }
    let galley = ui.painter().layout_job(job);
    let (rect, _response) = ui.allocate_exact_size(
        vec2(
            ui.available_width(),
            (space::XL + space::SM).max(galley.size().y + space::SM),
        ),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().galley(
        pos2(
            rect.left() + space::LG,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        colour::TEXT_STRONG,
    );
}

fn ranks(ui: &mut Ui, player: &Player, side: Side, still: bool) {
    ui.add_space(space::MD);
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), EMBLEM + space::MD),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        rank_line(ui, player, rect, side, still);
    }
    rank_bar(ui, player, still);
    // Zero is not a place. The backend sends it for anybody who is not
    // on the leaderboard at all, and "Leaderboard #0" reads as a rank.
    if let Some(place) = player.leaderboard.filter(|p| *p > 0) {
        line(
            ui,
            &format!("Leaderboard #{place}"),
            colour::WARN,
            size::BODY,
        );
    }
    let peak = player.peak_rank.as_deref().map(|peak| {
        player
            .peak_act
            .as_deref()
            .filter(|a| !a.is_empty())
            .map_or_else(
                || format!("Peak {peak}"),
                |act| format!("Peak {peak}, {act}"),
            )
    });
    let last = player
        .previous_rank
        .as_deref()
        .map(|previous| format!("Last act {previous}"));
    let said: Vec<String> = peak.into_iter().chain(last).collect();
    if !said.is_empty() {
        line(ui, &said.join("  \u{b7}  "), colour::TEXT_DIM, size::MICRO);
    }
}

fn rank_line(ui: &Ui, player: &Player, rect: Rect, side: Side, still: bool) {
    let painter = ui.painter().clone();
    let tier = player.rank_tier.unwrap_or(0);
    let emblem = pos2(rect.left() + space::LG + EMBLEM / 2.0, rect.center().y);
    if tier >= 3 {
        board::paint::emblem(
            &painter,
            tier,
            emblem,
            EMBLEM,
            if still { 0.0 } else { 1.0 },
        );
    }
    let x = rect.left() + space::LG + if tier >= 3 { EMBLEM + space::LG } else { 0.0 };
    let name = player.rank.as_deref().unwrap_or("Unranked");
    let _tier = caps_text(
        &painter,
        pos2(x, rect.center().y - 12.0),
        Align2::LEFT_CENTER,
        name,
        Face::Heavy.at(24.0),
        if tier >= 3 {
            colour::TEXT_STRONG
        } else {
            colour::TEXT_DIM
        },
    );
    let Some(rr) = player.rr else { return };
    let after = caps_text(
        &painter,
        pos2(x, rect.center().y + 14.0),
        Align2::LEFT_CENTER,
        &rr.to_string(),
        Face::Heavy.at(18.0),
        colour::TEXT_STRONG,
    );
    let after = caps_text(
        &painter,
        pos2(after.right() + space::SM, rect.center().y + 15.0),
        Align2::LEFT_CENTER,
        "RR",
        Face::Display.at(size::LABEL),
        colour::TEXT_FAINT,
    );
    if let Some(delta) = player.rr_earned.filter(|d| *d != 0) {
        let tint = if delta > 0 {
            board::paint::win(side)
        } else {
            colour::TEXT_DIM
        };
        let shown = caps_text(
            &painter,
            pos2(after.right() + space::LG, rect.center().y + 14.0),
            Align2::LEFT_CENTER,
            &format!("{delta:+}"),
            Face::Heavy.at(16.0),
            tint,
        );
        if let Some(actual) = player.rr_actual.filter(|a| *a != delta) {
            let _actual = caps_text(
                &painter,
                pos2(shown.right() + space::MD, rect.center().y + 15.0),
                Align2::LEFT_CENTER,
                &format!("{actual:+} actual"),
                Face::Display.at(size::LABEL),
                colour::TEXT_DIM,
            );
        }
    }
}

/// Puts a Riot ID on the clipboard and remembers it for a moment, so the
/// footer can say it worked.
pub(crate) fn copy(ctx: &egui::Context, name: String) {
    let now = ctx.input(|i| i.time);
    ctx.copy_text(name.clone());
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(COPIED), (name, now)));
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAID));
}

pub(crate) fn copied(ctx: &egui::Context) -> Option<String> {
    let now = ctx.input(|i| i.time);
    ctx.data(|d| d.get_temp::<(String, f64)>(egui::Id::new(COPIED)))
        .filter(|(_, at)| now - at < SAID)
        .map(|(name, _)| name)
}

/// The egui data key for the last copy.
const COPIED: &str = "overseer-copied";

const SAID: f64 = 1.8;

fn reads(ui: &mut Ui, player: &Player, side: Side) {
    let tags: Vec<(&str, &overseer_core::AutoTag)> = player
        .auto_tags
        .iter()
        .filter_map(|t| Some((t.tag.as_deref()?, t)))
        .collect();
    if tags.is_empty() {
        return;
    }
    heading(ui, "Reads");
    let laid = rows(
        ui.painter(),
        &kinds(&tags),
        ui.available_width() - space::LG,
    );
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), laid.height), Sense::hover());
    let shift = vec2(rect.left() + space::LG, rect.top());
    let painter = ui.painter().clone();
    for y in &laid.rules {
        painter.hline(
            rect.left() + space::LG..=rect.right() - space::LG,
            rect.top() + y,
            egui::Stroke::new(1.0, colour::LINE),
        );
    }
    for (at, kind) in &laid.labels {
        let _label = caps_text(
            &painter,
            *at + shift,
            Align2::LEFT_CENTER,
            kind,
            Face::Display.at(size::MICRO),
            colour::HINT,
        );
    }
    let font = Face::Display.at(size::MICRO + 1.0);
    let mut chips = Vec::with_capacity(laid.chips.len());
    let mut on = None;
    for (chip, tag, whole) in laid.chips {
        let chip = chip.translate(shift);
        if ui
            .interact(chip, ui.id().with(("read", tag)), Sense::hover())
            .hovered()
        {
            board::tip::mark_hovered(ui.ctx(), ui.layer_id(), chip);
            on = Some(chips.len());
        }
        chips.push((chip, whole));
        board::paint::chip_outline(&painter, chip, colour::TEXT_FAINT);
        let _word = caps_text(
            &painter,
            pos2(chip.left() + space::MD - 1.0, chip.center().y),
            Align2::LEFT_CENTER,
            tag,
            font.clone(),
            colour::TEXT,
        );
    }
    let group = ui.id().with("read-card");
    if let Some((chip, whole)) =
        board::tip::lingering(ui.ctx(), group, on).and_then(|i| chips.get(i))
    {
        board::tip::under(ui.ctx(), group, *chip, |ui| {
            board::tip::auto(ui, whole, board::paint::win(side));
        });
    }
}

type Group<'a> = (&'a str, Vec<(&'a str, &'a overseer_core::AutoTag)>);

fn kinds<'a>(tags: &[(&'a str, &'a overseer_core::AutoTag)]) -> Vec<Group<'a>> {
    let mut out: Vec<Group<'a>> = Vec::new();
    for &(tag, whole) in tags {
        let kind = whole.kind.as_deref().unwrap_or("Other");
        match out.last_mut() {
            Some((last, words)) if last.eq_ignore_ascii_case(kind) => words.push((tag, whole)),
            _ => out.push((kind, vec![(tag, whole)])),
        }
    }
    out
}

/// Where the Reads rows go, measured from their top left corner.
#[derive(Default)]
struct Laid<'a> {
    chips: Vec<(Rect, &'a str, &'a overseer_core::AutoTag)>,
    labels: Vec<(egui::Pos2, &'a str)>,
    rules: Vec<f32>,
    height: f32,
}

const KIND_COLUMN: f32 = 72.0;

fn rows<'a>(painter: &egui::Painter, groups: &[Group<'a>], right: f32) -> Laid<'a> {
    let font = Face::Display.at(size::MICRO + 1.0);
    let mut laid = Laid::default();
    let mut y = 0.0;
    for (at, (kind, words)) in groups.iter().enumerate() {
        if at > 0 {
            laid.rules.push(y + 3.0);
            y += 6.0;
        }
        laid.labels.push((pos2(0.0, y + 11.0), kind));
        let (mut x, mut row) = (KIND_COLUMN, 0.0_f32);
        for &(tag, whole) in words {
            let wide = 2.0f32.mul_add(space::MD, caps_width(painter, tag, font.clone()));
            if x + wide > right && x > KIND_COLUMN {
                x = KIND_COLUMN;
                row += 1.0;
            }
            let top = row.mul_add(CHIP_ROW, y) + 2.0;
            laid.chips.push((
                Rect::from_min_size(pos2(x, top), vec2(wide, 18.0)),
                tag,
                whole,
            ));
            x += wide + space::SM;
        }
        y = (row + 1.0).mul_add(CHIP_ROW, y);
    }
    laid.height = y;
    laid
}

fn brief(ui: &mut Ui, player: &Player) {
    let Some(read) = player.tendencies.as_ref() else {
        return;
    };
    heading(ui, read.title.as_deref().unwrap_or("Tendencies"));
    if let Some(tag) = &read.tag {
        if let Some(word) = tag.headline.as_deref() {
            caps_line(ui, word, Face::Display.at(size::TITLE), colour::TEXT_STRONG);
        }
        placed(ui, tag);
        ui.add_space(space::MD);
    }
    for habit in &read.habits {
        if let Some(words) = habit.headline.as_deref() {
            caps_line(ui, words, Face::Display.at(size::BODY), colour::TEXT_STRONG);
        }
        placed(ui, habit);
        if let Some(suggest) = habit.suggest.as_deref().filter(|s| !s.is_empty()) {
            line(ui, suggest, colour::TEXT, size::LABEL);
        }
        ui.add_space(space::MD);
    }
    if let Some(main) = &read.main {
        finding(ui, main, colour::TEXT_STRONG, size::BODY);
    }
    if let Some(buy) = &read.buy {
        if let Some(label) = buy.label.as_deref() {
            subheading(ui, label, colour::TEXT_DIM);
        }
        if buy.items.is_empty() {
            if let Some(guns) = buy.guns.as_deref() {
                line(ui, guns, colour::TEXT_STRONG, size::BODY);
            }
        } else {
            gun_row(ui, &buy.items);
        }
        if let Some(armor) = buy.armor.as_deref() {
            line(ui, armor, colour::TEXT, size::LABEL);
        }
        ui.add_space(space::SM);
    }
    if let Some(style) = &read.style {
        finding(ui, style, colour::TEXT, size::LABEL);
    }
    if let Some(empty) = read.empty.as_deref().filter(|s| !s.is_empty()) {
        line(ui, empty, colour::TEXT_DIM, size::LABEL);
    }
    if !player.brief.is_empty() {
        let id = ui
            .id()
            .with(("all-figures", player.puuid.as_deref().unwrap_or("")));
        if figures_toggle(ui, id, player.brief.len()) {
            all_figures(ui, &player.brief);
        }
    }
    if let Some(footer) = read.footer.as_deref() {
        line(ui, footer, colour::TEXT_FAINT, size::MICRO);
    }
}

/// Evidence with each short figure kept on one row, so "18 games" never
/// splits and the dot stays with what it follows. A long phrase can still
/// wrap between its words.
fn held(text: &str) -> String {
    text.split(" · ")
        .map(|part| {
            if part.chars().count() <= 20 {
                part.replace(' ', "\u{a0}")
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\u{a0}· ")
}

fn all_figures(ui: &mut Ui, lines: &[overseer_core::BriefLine]) {
    const GROUPS: [&str; 3] = ["Against Peers", "Buys", "On Record"];
    let group = |line: &overseer_core::BriefLine| match line.kind.as_deref() {
        Some("tendency" | "positional" | "style") => 0,
        Some("buy" | "forecast") => 1,
        _ => 2,
    };
    for (at, name) in GROUPS.iter().enumerate() {
        let rows: Vec<&overseer_core::BriefLine> =
            lines.iter().filter(|l| group(l) == at).collect();
        if rows.is_empty() {
            continue;
        }
        subheading(ui, name, colour::TEXT_FAINT);
        if at == 0 {
            figure_ladder(ui, &rows);
            continue;
        }
        for row in rows {
            let tint = if row.flag {
                colour::TEXT_STRONG
            } else {
                colour::TEXT
            };
            caps_line(ui, figure_words(row), Face::Display.at(size::LABEL), tint);
            if let Some(evidence) = row.evidence.as_deref().filter(|s| !s.is_empty()) {
                caps_line(
                    ui,
                    &held(evidence),
                    Face::Display.at(size::MICRO),
                    colour::TEXT_DIM,
                );
            }
        }
    }
}

fn figure_words(row: &overseer_core::BriefLine) -> &str {
    row.headline
        .as_deref()
        .or(row.text.as_deref())
        .unwrap_or("")
}

fn along(track: Rect, pct: f32) -> f32 {
    pct.clamp(0.0, 1.0).mul_add(track.width(), track.left())
}

fn rail(painter: &egui::Painter, track: Rect) {
    painter.rect_filled(track, 0, colour::BG_INSET);
    for (quarter, tint) in [
        (0.25, colour::LINE),
        (0.5, colour::EDGE),
        (0.75, colour::LINE),
    ] {
        let x = along(track, quarter);
        painter.vline(x, track.y_range().expand(2.0), (1.0, tint));
    }
}

/// Every figure measured against peers on one shared scale, a row each,
/// so the ones far from the middle stand out at a glance. The figures
/// behind each are on hover.
fn figure_ladder(ui: &mut Ui, rows: &[&overseer_core::BriefLine]) {
    let width = ui.available_width();
    let left = ui.max_rect().left() + space::LG;
    let right = ui.max_rect().right() - space::LG;
    let words_end = (right - left).mul_add(0.56, left);
    let number = 36.0;
    let track_x = (words_end + space::MD)..=(right - number - space::SM);
    let (head, _response) = ui.allocate_exact_size(vec2(width, space::LG), Sense::hover());
    if ui.is_rect_visible(head) {
        let painter = ui.painter();
        let font = Face::Display.at(size::MICRO);
        let ends = [
            (*track_x.start(), Align2::LEFT_CENTER, "Lower"),
            (*track_x.end(), Align2::RIGHT_CENTER, "Higher"),
        ];
        for (x, align, word) in ends {
            let _drawn = caps_text(
                painter,
                pos2(x, head.center().y),
                align,
                word,
                font.clone(),
                colour::TEXT_FAINT,
            );
        }
    }
    for row in rows {
        let (rect, response) = ui.allocate_exact_size(vec2(width, space::XL), Sense::hover());
        let tint = if row.flag {
            colour::TEXT_STRONG
        } else {
            colour::TEXT
        };
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            if response.hovered() {
                painter.rect_filled(rect.shrink2(vec2(space::SM, 0.0)), 0, colour::BG_HOVER);
            }
            let job = one_line(
                figure_words(row),
                Face::Display.at(size::LABEL),
                tint,
                words_end - left,
            );
            let galley = painter.layout_job(job);
            painter.galley(
                pos2(left, rect.center().y - galley.size().y / 2.0),
                galley,
                tint,
            );
            if let Some(pct) = row.pct {
                let track = Rect::from_x_y_ranges(
                    track_x.clone(),
                    (rect.center().y - 1.0)..=(rect.center().y + 1.0),
                );
                rail(painter, track);
                let at = pos2(along(track, pct), rect.center().y);
                if row.flag {
                    painter.circle_filled(at, 4.5, colour::TEXT_STRONG);
                } else {
                    painter.circle_stroke(at, 3.5, (1.5, colour::TEXT_DIM));
                }
                let _place = painter.text(
                    pos2(right, rect.center().y),
                    Align2::RIGHT_CENTER,
                    ordinal(pct),
                    Face::Heavy.at(size::MICRO + 1.0),
                    tint,
                );
            }
        }
        if let Some(evidence) = row.evidence.as_deref().filter(|s| !s.is_empty()) {
            let _shown = response.on_hover_text(evidence);
        }
    }
}

fn one_line(text: &str, font: egui::FontId, tint: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = caps(text, font, tint);
    job.wrap.max_width = width.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job
}

/// 86th, 81st, 12th: the share of peers below them as an ordinal.
pub(crate) fn ordinal(pct: f32) -> String {
    let n = format!("{:.0}", pct.clamp(0.0, 1.0) * 100.0);
    let suffix = if ["11", "12", "13"].iter().any(|teen| n.ends_with(teen)) {
        "th"
    } else if n.ends_with('1') {
        "st"
    } else if n.ends_with('2') {
        "nd"
    } else if n.ends_with('3') {
        "rd"
    } else {
        "th"
    };
    format!("{n}{suffix}")
}

fn placed(ui: &mut Ui, found: &overseer_core::TendencyFinding) {
    let evidence = found.evidence.as_deref().unwrap_or("");
    match found.pos {
        Some(pos) => {
            let (shape, lo, hi): (&[f32], f32, f32) = if found.curve.is_empty() {
                (NORMAL_SHAPE.as_slice(), -BELL_SPAN, BELL_SPAN)
            } else {
                (
                    found.curve.as_slice(),
                    found.lo.unwrap_or(-BELL_SPAN),
                    found.hi.unwrap_or(BELL_SPAN),
                )
            };
            let own = found.own.as_deref().unwrap_or("");
            let pct_word = found.share.as_deref().unwrap_or("");
            let at_mark = match (own.is_empty(), pct_word.is_empty()) {
                (false, false) => format!("{own} · {pct_word}"),
                (true, false) => pct_word.to_owned(),
                (false, true) => own.to_owned(),
                (true, true) => String::new(),
            };
            bell(
                ui,
                &Bell {
                    shape,
                    lo,
                    hi,
                    pos,
                    at_mark: &at_mark,
                    at_middle: found.peers.as_deref().unwrap_or(""),
                    left_word: found.left.as_deref().unwrap_or(""),
                    right_word: found.right.as_deref().unwrap_or(""),
                    named: found.headline.as_deref().unwrap_or(""),
                },
            );
            if !evidence.is_empty() {
                line(ui, &held(evidence), colour::TEXT_DIM, size::MICRO);
            }
        }
        None if !evidence.is_empty() => line(ui, &held(evidence), colour::TEXT_DIM, size::MICRO),
        None => {}
    }
}

const BELL_MIN_W: f32 = 48.0;
const BELL_H: f32 = 24.0;
const BELL_ENDS: f32 = 16.0;
const BELL_TOP: f32 = 16.0;
const BELL_SPAN: f32 = 2.6;

struct Bell<'a> {
    shape: &'a [f32],
    lo: f32,
    hi: f32,
    pos: f32,
    at_mark: &'a str,
    at_middle: &'a str,
    left_word: &'a str,
    right_word: &'a str,
    named: &'a str,
}

struct Frame {
    left: f32,
    width: f32,
    base: f32,
    top: f32,
}

impl Frame {
    const fn x_at(&self, t: f32) -> f32 {
        t.mul_add(self.width, self.left)
    }
}

/// A normal curve, for a read without a measured shape.
static NORMAL_SHAPE: std::sync::LazyLock<Vec<f32>> = std::sync::LazyLock::new(|| {
    (0..=BELL_SAMPLES)
        .map(|i| {
            let z = (f32::from(i) / f32::from(BELL_SAMPLES)).mul_add(2.0 * BELL_SPAN, -BELL_SPAN);
            (-(z * z) / 2.0).exp()
        })
        .collect()
});

const BELL_SAMPLES: u8 = 48;

fn shape_at(shape: &[f32], peak: f32, t: f32) -> f32 {
    let last = shape.len().saturating_sub(1);
    if last == 0 {
        return 0.0;
    }
    let scaled = t.clamp(0.0, 1.0) * last as f32;
    let i = (scaled.floor() as usize).min(last - 1);
    let frac = scaled - i as f32;
    let (Some(&from), Some(&to)) = (shape.get(i), shape.get(i + 1)) else {
        return 0.0;
    };
    (to - from).mul_add(frac, from) / peak
}

fn bell(ui: &mut Ui, bell: &Bell<'_>) {
    let width = ui.available_width();
    let frame_width = 2.0f32.mul_add(-space::LG, width).max(BELL_MIN_W);
    let height = BELL_TOP + BELL_H + BELL_ENDS;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Label,
            true,
            format!(
                "{}: {}, between {} and {}",
                bell.named, bell.at_mark, bell.left_word, bell.right_word
            ),
        )
    });
    if !ui.is_rect_visible(rect) {
        return;
    }
    let frame = Frame {
        left: rect.left() + space::LG,
        width: frame_width,
        base: rect.top() + BELL_TOP + BELL_H,
        top: rect.top(),
    };
    let painter = ui.painter();
    let span = (bell.hi - bell.lo).max(f32::EPSILON);
    let peak = bell
        .shape
        .iter()
        .copied()
        .fold(0.0f32, f32::max)
        .max(f32::EPSILON);
    let height_at = |t: f32| shape_at(bell.shape, peak, t) * (BELL_H - 2.0);
    let mark = ((bell.pos - bell.lo) / span).clamp(0.0, 1.0);
    let tint = colour::EDGE.gamma_multiply(0.45);
    let mut fill = egui::Mesh::default();
    let samples = (0..=BELL_SAMPLES)
        .map(|i| f32::from(i) / f32::from(BELL_SAMPLES))
        .take_while(|t| *t < mark)
        .chain(std::iter::once(mark));
    for t in samples {
        let at = u32::try_from(fill.vertices.len()).unwrap_or(u32::MAX);
        fill.colored_vertex(pos2(frame.x_at(t), frame.base), tint);
        fill.colored_vertex(pos2(frame.x_at(t), frame.base - height_at(t)), tint);
        if at >= 2 {
            fill.add_triangle(at - 2, at - 1, at);
            fill.add_triangle(at - 1, at, at + 1);
        }
    }
    painter.add(fill);
    let curve: Vec<egui::Pos2> = (0..=BELL_SAMPLES)
        .map(|i| {
            let t = f32::from(i) / f32::from(BELL_SAMPLES);
            pos2(frame.x_at(t), frame.base - height_at(t))
        })
        .collect();
    painter.add(egui::Shape::line(curve, (1.0, colour::EDGE)));
    painter.line_segment(
        [
            pos2(frame.left, frame.base),
            pos2(frame.left + frame.width, frame.base),
        ],
        (1.0, colour::LINE),
    );
    painter.line_segment(
        [
            pos2(frame.x_at(0.5), frame.base),
            pos2(frame.x_at(0.5), frame.base + 3.0),
        ],
        (1.0, colour::EDGE),
    );
    painter.line_segment(
        [
            pos2(frame.x_at(mark), frame.base),
            pos2(frame.x_at(mark), frame.base - BELL_H),
        ],
        (1.5, colour::TEXT_STRONG),
    );
    bell_words(painter, bell, &frame, mark);
}

fn bell_words(painter: &egui::Painter, bell: &Bell<'_>, frame: &Frame, mark: f32) {
    if !bell.at_mark.is_empty() {
        let font = Face::Heavy.at(size::BODY);
        let galley = painter.layout_no_wrap(bell.at_mark.to_owned(), font, colour::TEXT_STRONG);
        let half = galley.size().x / 2.0;
        let x = frame
            .x_at(mark)
            .min(frame.left + frame.width - half)
            .max(frame.left + half);
        painter.galley(
            pos2(
                x - half,
                frame.top + (BELL_TOP - galley.size().y).max(0.0) / 2.0,
            ),
            galley,
            colour::TEXT_STRONG,
        );
    }
    let ends = Face::Display.at(size::MICRO);
    let _left = caps_text(
        painter,
        pos2(frame.left, frame.base + 2.0),
        Align2::LEFT_TOP,
        bell.left_word,
        ends.clone(),
        colour::TEXT_FAINT,
    );
    let _right = caps_text(
        painter,
        pos2(frame.left + frame.width, frame.base + 2.0),
        Align2::RIGHT_TOP,
        bell.right_word,
        ends.clone(),
        colour::TEXT_FAINT,
    );
    if !bell.at_middle.is_empty() {
        painter.text(
            pos2(frame.x_at(0.5), frame.base + 1.0),
            Align2::CENTER_TOP,
            format!("peers {}", bell.at_middle),
            ends,
            colour::TEXT_FAINT,
        );
    }
}

/// How tall a gun's picture is drawn in a buy block's row, and the room
/// under it for its figure and name.
const GUN_H: f32 = 32.0;
const GUN_WORDS: f32 = space::SM + size::TITLE + space::SM + size::MICRO;

/// The guns of a buy block side by side: each one's picture, its odds or
/// count in the panel's numerals under it, and its name in caps.
fn gun_row(ui: &mut Ui, items: &[overseer_core::TendencyGun]) {
    let width = ui.available_width();
    let (rect, _response) =
        ui.allocate_exact_size(vec2(width, GUN_H + GUN_WORDS + space::SM), Sense::hover());
    if !ui.is_rect_visible(rect) || items.is_empty() {
        return;
    }
    let inner = 2.0f32.mul_add(-space::LG, width);
    let cell = inner / items.len() as f32;
    let painter = ui.painter();
    for (i, gun) in items.iter().enumerate() {
        let name = gun.name.as_deref().unwrap_or("");
        let left = (i as f32).mul_add(cell, rect.left() + space::LG);
        let slot = Rect::from_min_size(
            pos2(left + space::SM, rect.top()),
            vec2(2.0f32.mul_add(-space::SM, cell), GUN_H),
        );
        if let Some(texture) = art::weapon(ui.ctx(), name) {
            let size = texture.size_vec2();
            let scale = (slot.width() / size.x.max(1.0)).min(slot.height() / size.y.max(1.0));
            let drawn = Rect::from_center_size(slot.center(), size * scale);
            painter.image(
                texture.id(),
                drawn,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let centre = left + cell / 2.0;
        painter.text(
            pos2(centre, rect.top() + GUN_H + space::SM),
            Align2::CENTER_TOP,
            gun.value.as_deref().unwrap_or(""),
            Face::Heavy.at(size::TITLE),
            colour::TEXT_STRONG,
        );
        let _name = caps_text(
            painter,
            pos2(
                centre,
                rect.top() + GUN_H + space::SM + size::TITLE + space::SM,
            ),
            Align2::CENTER_TOP,
            name,
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

fn finding(ui: &mut Ui, found: &overseer_core::TendencyFinding, tint: Color32, points: f32) {
    if let Some(words) = found.headline.as_deref() {
        caps_line(ui, words, Face::Display.at(points), tint);
    }
    if let Some(evidence) = found.evidence.as_deref().filter(|s| !s.is_empty()) {
        line(ui, &held(evidence), colour::TEXT_DIM, size::MICRO);
    }
    if let Some(suggest) = found.suggest.as_deref().filter(|s| !s.is_empty()) {
        line(ui, suggest, colour::TEXT, size::LABEL);
    }
    ui.add_space(space::SM);
}

const CHIP_ROW: f32 = 22.0;

const EMBLEM: f32 = 52.0;

fn rank_bar(ui: &mut Ui, player: &Player, still: bool) {
    let Some(rr) = player.rr else { return };
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::MD), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let share = ui.ctx().animate_value_with_time(
        egui::Id::new("panel-rr"),
        (rr as f32 / 100.0).clamp(0.0, 1.0),
        if still {
            motion::EFFICIENT
        } else {
            motion::MEASURE
        },
    );
    let track = Rect::from_min_max(
        pos2(rect.left() + space::LG, rect.center().y - 3.0),
        pos2(rect.right() - space::LG, rect.center().y + 3.0),
    );
    let painter = ui.painter();
    painter.rect_filled(track, 0, colour::BG_INSET);
    let mut filled = track;
    filled.set_width(track.width() * share);
    painter.add(shape::pip(filled, rank(player.rank_tier)));
}

fn form(ui: &mut Ui, player: &Player, side: Side) {
    if player.form.is_empty() {
        return;
    }
    subheading(ui, "Form", colour::TEXT_DIM);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if ui.is_rect_visible(rect) {
        pips(ui, player, rect, side);
    }
}

fn mains(ui: &mut Ui, player: &Player) {
    let agents = player.mains();
    if agents.is_empty() {
        return;
    }
    let heading = player
        .main_role()
        .map_or_else(|| "Mains".to_owned(), |role| format!("Mains: {role}"));
    subheading(ui, &heading, colour::TEXT_DIM);
    let most = agents.first().and_then(|a| a.games).unwrap_or(0).max(1);
    for agent in agents {
        main_row(ui, agent, most);
    }
}

const MAIN_FACE: f32 = 26.0;

fn main_row(ui: &mut Ui, agent: &overseer_core::TopAgent, most: u32) {
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::SM.mul_add(2.0, MAIN_FACE)),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let name = agent.agent.as_deref().unwrap_or_default();
    let face = Rect::from_min_size(
        pos2(rect.left() + space::LG, rect.center().y - MAIN_FACE / 2.0),
        vec2(MAIN_FACE, MAIN_FACE),
    );
    painter.rect_filled(face, 0, colour::BG_INSET);
    if let Some(portrait) = art::agent(painter.ctx(), name) {
        let mut mesh = egui::Mesh::with_texture(portrait.id());
        mesh.add_rect_with_uv(
            face,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.add(egui::Shape::mesh(mesh));
    } else {
        let _initial = caps_text(
            painter,
            face.center(),
            Align2::CENTER_CENTER,
            &name.chars().take(1).collect::<String>(),
            Face::Display.at(size::BODY),
            colour::TEXT_DIM,
        );
    }
    let x = face.right() + space::MD;
    let named = painter.text(
        pos2(x, rect.center().y - 1.0),
        Align2::LEFT_BOTTOM,
        name,
        Face::Body.at(size::BODY),
        colour::TEXT,
    );
    let role = agent.role.as_deref().unwrap_or("Agent");
    let roled = caps_text(
        painter,
        pos2(x, rect.center().y + 2.0),
        Align2::LEFT_TOP,
        role,
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    let games = agent.games.unwrap_or(0);
    let count = painter.text(
        pos2(rect.right() - space::LG, rect.center().y),
        Align2::RIGHT_CENTER,
        games.to_string(),
        Face::Number.at(size::BODY),
        colour::TEXT,
    );
    let from = (x + MAIN_TEXT).max(named.right().max(roled.right()) + space::MD);
    let to = count.left() - space::MD;
    if to - from < space::XL {
        return;
    }
    let track = Rect::from_min_max(
        pos2(from, rect.center().y - 2.0),
        pos2(to, rect.center().y + 2.0),
    );
    painter.rect_filled(track, 0, colour::BG_INSET);
    let mut filled = track;
    filled.set_width(track.width() * (games as f32 / most as f32).clamp(0.0, 1.0));
    painter.rect_filled(filled, 0, colour::TEXT_DIM);
}

const MAIN_TEXT: f32 = 84.0;

fn pips(ui: &Ui, player: &Player, rect: Rect, side: Side) {
    let painter = ui.painter().clone();
    board::paint::pips(
        &painter,
        &player.form,
        pos2(rect.left() + space::LG, rect.center().y),
        11.0,
        board::paint::win(side),
    );
    let x = 5.0f32.mul_add(13.0, rect.left() + space::LG);
    let Some(streak) = player.streak.as_ref() else {
        return;
    };
    let count = streak.count.unwrap_or(0);
    if count < 2 {
        return;
    }
    let won = streak.kind.as_deref() == Some("W");
    painter.text(
        pos2(x + space::MD, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{count} {} in a row", if won { "won" } else { "lost" }),
        Face::Body.at(size::LABEL + 1.0),
        if won {
            board::paint::win(side)
        } else {
            colour::TEXT_DIM
        },
    );
}

fn numbers(ui: &mut Ui, player: &Player, side: Side) {
    let out_of = |games: Option<u32>| games.map_or_else(String::new, |g| format!("\nOut of {g}"));
    let behind = player.form.len();
    let recent = out_of(u32::try_from(behind).ok().filter(|n| *n > 0));
    let last = match behind {
        1 => "their last game".to_owned(),
        n => format!("their last {n} games"),
    };
    let map = player
        .map_win_rate
        .as_ref()
        .filter(|m| m.games.unwrap_or(0) > 0);
    let owned: Vec<(String, Color32, String, String)> = [
        Some((
            player.kd.map_or_else(dash, |v| format!("{v:.2}")),
            if board::few_games(player) {
                colour::TEXT_FAINT
            } else {
                board::paint::kd_heat(side, player.kd)
            },
            format!("K/D{recent}"),
            format!("Kills per death over {last} of competitive, unrated, swiftplay and premier."),
        )),
        Some((
            player.hs_pct.map_or_else(dash, |v| format!("{v:.0}%")),
            colour::TEXT,
            format!("HS{recent}"),
            format!("Share of their hits that landed on the head over {last}."),
        )),
        Some((
            player
                .win_rate
                .map_or_else(dash, |v| format!("{}%", v.round())),
            sample(player.games, 10),
            format!("Win{}", out_of(player.games)),
            format!(
                "Share of their {} ranked games this act that they won.",
                player.games.unwrap_or(0)
            ),
        )),
        map.map(|m| {
            (
                format!("{}%", m.win_rate.unwrap_or(0.0).round()),
                sample(m.games, 5),
                format!("Map{}", out_of(m.games)),
                format!(
                    "Share they won of the {} games on this map in {last}.",
                    m.games.unwrap_or(0)
                ),
            )
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    heading(ui, "Stats");
    figures(ui, &figures_of(&owned));
}

fn sample(games: Option<u32>, few: u32) -> Color32 {
    if games.is_some_and(|g| g < few) {
        colour::TEXT_FAINT
    } else {
        colour::TEXT
    }
}

fn figures_of(owned: &[(String, Color32, String, String)]) -> Vec<Figure<'_>> {
    owned
        .iter()
        .map(|(value, tint, under, about)| (value.as_str(), *tint, under.as_str(), about.as_str()))
        .collect()
}

pub(crate) type Figure<'a> = (&'a str, Color32, &'a str, &'a str);

pub(crate) fn figures(ui: &mut Ui, row: &[Figure<'_>]) {
    let value_font = Face::Heavy.at(size::DISPLAY);
    let under_font = Face::Display.at(size::MICRO);
    let widest = row
        .iter()
        .map(|&(value, _, under, _)| {
            under
                .lines()
                .map(|l| caps_width(ui.painter(), l.trim(), under_font.clone()))
                .fold(
                    caps_width(ui.painter(), value, value_font.clone()),
                    f32::max,
                )
        })
        .fold(0.0, f32::max);
    let inner = 2.0f32.mul_add(-space::LG, ui.available_width());
    let fits = ((inner / (widest + space::LG)).floor() as usize).clamp(1, row.len().max(1));
    let rows = row.len().div_ceil(fits).max(1);
    let per_row = row.len().div_ceil(rows).max(1);
    for chunk in row.chunks(per_row) {
        let lines = chunk
            .iter()
            .map(|&(_, _, under, _)| under.lines().count())
            .max()
            .unwrap_or(1);
        let (rect, _response) = ui.allocate_exact_size(
            vec2(ui.available_width(), 13.0f32.mul_add(lines as f32, 27.0)),
            Sense::hover(),
        );
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter().clone();
        let inner = rect.shrink2(vec2(space::LG, 0.0));
        let each = inner.width() / per_row as f32;
        for (i, &(value, tint, under, about)) in chunk.iter().enumerate() {
            let x = inner.left() + each * i as f32;
            let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(each, rect.height()));
            let id = ui.id().with(("figure", under));
            let hit = ui.interact(cell, id, Sense::hover());
            hit.widget_info(|| {
                let under = under.lines().collect::<Vec<_>>().join(" ");
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Label,
                    true,
                    format!("{value} {under}. {about}"),
                )
            });
            if !about.is_empty() && hit.hovered() {
                let line = Rect::from_min_size(
                    pos2(x, rect.top() + 2.0),
                    vec2(caps_width(&painter, value, value_font.clone()), 20.0),
                );
                board::tip::mark_hovered(ui.ctx(), ui.layer_id(), line);
                board::tip::say_under(ui.ctx(), ui.id().with("figure-card"), cell, about);
            }
            let _value = caps_text(
                &painter,
                pos2(x, rect.top() + 12.0),
                Align2::LEFT_CENTER,
                value,
                value_font.clone(),
                tint,
            );
            for (k, text) in under.lines().enumerate() {
                let _under = caps_text(
                    &painter,
                    pos2(x, 13.0f32.mul_add(k as f32, rect.top() + 31.0)),
                    Align2::LEFT_CENTER,
                    text.trim(),
                    under_font.clone(),
                    if k == 0 {
                        colour::TEXT_DIM
                    } else {
                        colour::TEXT_FAINT
                    },
                );
            }
        }
    }
}

fn group(ui: &mut Ui, player: &Player) {
    if let Some(size) = player
        .party
        .as_ref()
        .and_then(|p| p.size)
        .filter(|s| *s > 1)
    {
        subheading(ui, "Party", colour::TEXT_DIM);
        line(
            ui,
            &format!("{}, confirmed by Riot", stack_word(size)),
            colour::TEXT,
            size::BODY,
        );
        return;
    }
    let Some(guess) = player.stack_guess.as_ref() else {
        return;
    };
    subheading(ui, "Probably Together", colour::TEXT_DIM);
    line(
        ui,
        &stack_word(guess.size.unwrap_or(0)),
        colour::TEXT_STRONG,
        size::BODY,
    );
    line(
        ui,
        &format!(
            "{} of {} lobbies on the same side, {}% sure",
            guess.same.unwrap_or(0),
            guess.shared.unwrap_or(0),
            guess.confidence.unwrap_or(0)
        ),
        colour::TEXT_FAINT,
        size::MICRO,
    );
}

fn stack_word(size: u32) -> String {
    match size {
        0 | 1 => "On their own".to_owned(),
        2 => "A duo".to_owned(),
        3 => "A trio".to_owned(),
        n => format!("A {n} stack"),
    }
}

fn met(ui: &mut Ui, player: &Player) {
    let Some(encounter) = player.encounter.as_ref().filter(|e| e.total() > 0) else {
        return;
    };
    subheading(
        ui,
        &format!("Met {} Times Before", encounter.total()),
        colour::TEXT_DIM,
    );
    let with = encounter.with_count.unwrap_or(0);
    if with > 0 {
        stat(
            ui,
            "With",
            &record(
                encounter.wins_with,
                encounter.losses_with,
                encounter.draws_with,
            ),
            colour::TEXT,
            "",
        );
    }
    let against = encounter.against_count.unwrap_or(0);
    if against > 0 {
        stat(
            ui,
            "Against",
            &record(
                encounter.wins_against,
                encounter.losses_against,
                encounter.draws_against,
            ),
            colour::TEXT,
            "",
        );
    }
}

fn record(wins: Option<u32>, losses: Option<u32>, draws: Option<u32>) -> String {
    let draws = draws.unwrap_or(0);
    let base = format!("{}W-{}L", wins.unwrap_or(0), losses.unwrap_or(0));
    if draws > 0 {
        format!("{base}-{draws}D")
    } else {
        base
    }
}

fn loadout(ui: &mut Ui, player: &Player) {
    let skins: Vec<(String, String)> = player
        .weapons
        .iter()
        .filter_map(|w| Some((w.weapon.clone()?, w.skin.as_ref()?.name.clone()?)))
        .take(4)
        .collect();
    if skins.is_empty() {
        return;
    }
    subheading(ui, "Carrying", colour::TEXT_DIM);
    for (weapon, skin) in skins {
        named(ui, &weapon, &skin);
    }
}

fn named(ui: &mut Ui, label: &str, value: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    caps_at(
        &painter,
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    let at = rect.left() + space::LG + VALUE_X;
    let mut job = egui::text::LayoutJob::simple_singleline(
        value.to_owned(),
        Face::Body.at(size::MICRO),
        colour::TEXT_DIM,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: rect.right() - space::LG - at,
        max_rows: 1,
        break_anywhere: false,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(at, rect.center().y - galley.size().y / 2.0),
        galley,
        colour::TEXT_DIM,
    );
}

const HEADING: f32 = 22.0;

pub(crate) fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(space::LG);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEADING), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let drawn = caps_text(
        painter,
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        Face::Heavy.at(size::TITLE),
        colour::TEXT,
    );
    board::paint::double_rule(
        painter,
        drawn.right() + space::MD..=rect.right() - space::LG,
        rect.center().y + 1.0,
    );
}

pub(crate) fn subheading(ui: &mut Ui, text: &str, tint: Color32) {
    ui.add_space(space::MD);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let _drawn = caps_text(
        ui.painter(),
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        Face::Display.at(size::LABEL),
        tint,
    );
}

pub(crate) fn line(ui: &mut Ui, text: &str, tint: Color32, points: f32) {
    let job = egui::text::LayoutJob::simple(text.to_owned(), Face::Body.at(points), tint, 0.0);
    laid(ui, job, tint);
}

fn caps_line(ui: &mut Ui, text: &str, font: egui::FontId, tint: Color32) {
    laid(ui, caps(text, font, tint), tint);
}

fn laid(ui: &mut Ui, mut job: egui::text::LayoutJob, tint: Color32) {
    let width = ui.available_width();
    job.wrap.max_width = 2.0f32.mul_add(-space::LG, width);
    let galley = ui.painter().layout_job(job);
    let first = galley.rows.first().map_or(0.0, |row| row.rect().height());
    let (rect, _response) = ui.allocate_exact_size(
        vec2(width, space::XL + galley.size().y - first),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().galley(
        pos2(
            rect.left() + space::LG,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        tint,
    );
}

pub(crate) fn stat(ui: &mut Ui, label: &str, value: &str, tint: Color32, note: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    caps_at(
        &painter,
        pos2(rect.left() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    let after = painter.text(
        pos2(rect.left() + space::LG + VALUE_X, rect.center().y),
        Align2::LEFT_CENTER,
        value,
        Face::Number.at(size::BODY),
        tint,
    );
    if !note.is_empty() {
        painter.text(
            pos2(after.right() + space::MD, rect.center().y),
            Align2::LEFT_CENTER,
            note,
            Face::Body.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

fn dash() -> String {
    "-".to_owned()
}

#[cfg(test)]
mod tests {
    use egui::Ui;
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use overseer_core::Player;

    use super::{Notes, held};
    use crate::notes::Note;

    #[test]
    fn reads_take_a_row_per_kind_they_have() {
        let tag = |kind: &str| overseer_core::AutoTag {
            kind: Some(kind.to_owned()),
            ..overseer_core::AutoTag::default()
        };
        let (op, marshal, hot) = (tag("weapon"), tag("Weapon"), tag("Form"));
        let tags = [("op", &op), ("marshal", &marshal), ("hot", &hot)];
        let rows: Vec<(&str, usize)> = super::kinds(&tags)
            .iter()
            .map(|(kind, words)| (*kind, words.len()))
            .collect();
        assert_eq!(rows, [("weapon", 2), ("Form", 1)]);
    }

    #[test]
    fn short_figures_stay_on_one_row() {
        assert_eq!(
            held("In the opening fight more than 89% of peers · 24% vs 15% · 18 games"),
            "In the opening fight more than 89% of peers\u{a0}· 24%\u{a0}vs\u{a0}15%\u{a0}· 18\u{a0}games",
        );
    }

    #[test]
    fn what_you_type_about_somebody_is_kept() {
        let player = Player {
            puuid: Some("p1".to_owned()),
            name: Some("Day#9932".to_owned()),
            ..Player::default()
        };
        let mut store = Notes::default();
        store.set(
            "p1",
            Note {
                tags: vec!["duo".to_owned()],
                ..Note::default()
            },
        );

        let mut harness = Harness::builder()
            .with_size(egui::vec2(320.0, 300.0))
            .build_ui_state(
                move |ui: &mut Ui, state: &mut (bool, Notes)| {
                    if state.0 {
                        super::notes(ui, &player, &mut state.1, super::Side::Enemy);
                    }
                },
                (false, store),
            );
        overseer_ui::install_fonts(&harness.ctx);
        harness
            .ctx
            .set_style_of(egui::Theme::Dark, overseer_ui::style());
        harness.run();
        harness.state_mut().0 = true;
        harness.run();

        harness.get_by_role(Role::MultilineTextInput).focus();
        harness.run();
        harness
            .get_by_role(Role::MultilineTextInput)
            .type_text("instalocks");
        harness.run();
        assert_eq!(harness.state().1.get("p1").text, "instalocks");
        assert!(harness.state().1.has("p1"));
    }
}
