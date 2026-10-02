//! The detail panel: one player, in full. The board shows what fits and this
//! shows the rest of what the backend knows, in a steep order on purpose: the
//! name, then the one thing worth knowing, then the numbers, then the small
//! print.

use egui::{Align2, Color32, Rect, RichText, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_core::Player;

use crate::board::{self, Side};
use crate::career::{self, Career};
use crate::notes::{self, Notes};
use overseer_ui::{
    Face, art, caps_at, caps_text, caps_width, colour, hex, motion, rank, shape, size, space,
};

/// Where a value starts, so labels and values have a spine down the middle.
const VALUE_X: f32 = 60.0;

/// Draws the panel for a player, or says nobody is selected. True when the
/// notes want saving to disk. `still` is the efficient tier, as on the board.
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
            // Two columns when there is room, so a screen of ordinary height
            // needs no scrolling.
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

/// Past this width the panel is two columns.
pub(crate) const TWO_COLUMNS: f32 = 560.0;

/// Who they are: everything that came with the board.
fn about(ui: &mut Ui, player: &Player, side: Side, store: &mut Notes, still: bool) -> bool {
    card(ui, player);
    if !player.smurf_reasons.is_empty() {
        verdict(ui, player);
    }
    reads(ui, player, side);
    let save = notes(ui, player, store, side);
    ranks(ui, player, side, still);
    form(ui, player, side);
    numbers(ui, player, side);
    group(ui, player);
    met(ui, player);
    save
}

/// The history and then the skins. The history arrives late, so it goes under
/// everything that should not move when it lands.
fn lately(ui: &mut Ui, player: &Player, side: Side, career: &Career) {
    if let Some(puuid) = player.puuid.as_deref() {
        career::show(ui, career, puuid, side);
    }
    loadout(ui, player);
}

/// Your own notes on them, written to the store on every keystroke. True when
/// a box loses the keyboard and the notes want saving to disk.
fn notes(ui: &mut Ui, player: &Player, store: &mut Notes, side: Side) -> bool {
    let Some(id) = player.puuid.as_deref() else {
        return false;
    };
    // Shut until there is a note or somebody asks for one, so two empty boxes
    // do not push the ranks down for everybody.
    let opened = egui::Id::new(("note-open", id));
    let mut note = store.get(id);
    if note.is_empty() && !ui.data(|d| d.get_temp::<bool>(opened)).unwrap_or(false) {
        if invite(ui).clicked() {
            ui.data_mut(|d| d.insert_temp(opened, true));
        }
        return false;
    }
    heading(ui, "Your Notes");
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
            // Ids come from the account, not the position. Every player's
            // box sits in the same spot, so a positional id hands the focused
            // box to the next player when the pointer moves, and the rest of
            // the sentence gets saved under their name.
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
        // Drop the raw text so the tidied tags show from the next frame.
        ui.data_mut(|d| d.remove::<String>(key));
    } else if touched {
        ui.data_mut(|d| d.insert_temp(key, tags));
    }
    done
}

/// A text box drawn as a dark slot like the search, with a cream edge along
/// its foot while it has the keyboard.
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

/// Tags offered under the box, from what the app read off this player and
/// the ones you use most. Returns the one clicked.
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

/// One suggested tag, as a small outlined chip with a plus: a click adds it.
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

/// Opens the note for an account and puts the keyboard in it, for the N key.
pub(crate) fn write_note(ctx: &egui::Context, id: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(("note-open", id)), true));
    ctx.memory_mut(|m| m.request_focus(egui::Id::new(("note-text", id))));
}

/// The faint line that stands in for the notes section until there is a note.
fn invite(ui: &mut Ui) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::click());
    if ui.is_rect_visible(rect) {
        let tint = if response.hovered() {
            colour::TEXT_DIM
        } else {
            colour::TEXT_FAINT
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

/// Grey placeholder text, in the panel's own face.
fn hint(text: &str) -> RichText {
    RichText::new(text)
        .color(colour::HINT)
        .font(Face::Body.at(size::MICRO))
}

/// How tall the player card is.
const CARD: f32 = 120.0;

/// The player as a broadcast card: their agent's art across the panel, fading
/// dark at the foot so the name set there reads on any agent.
fn card(ui: &mut Ui, player: &Player) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), CARD), Sense::click());
    // A click copies the Riot ID, the same as Ctrl+C.
    if let Some(name) = player.name.as_deref() {
        let response = response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Copy their Riot ID");
        if response.clicked() {
            copy(ui.ctx(), name.to_owned());
        }
    }
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
        // Fit the width, keep the top of the crop, which is where the eyes
        // are, and let the chin go.
        let texture = face.size_vec2();
        let shown = (card.height() / card.width()) * (texture.x / texture.y.max(1.0));
        let mut mesh = egui::Mesh::with_texture(face.id());
        mesh.add_rect_with_uv(
            card,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, shown.clamp(0.0, 1.0))),
            Color32::WHITE,
        );
        painter.add(egui::Shape::mesh(mesh));
    } else if let Some(banner) = player
        .card_art
        .as_deref()
        .and_then(|path| art::file(ui.ctx(), path))
        .or_else(|| art::default_card(ui.ctx()))
    {
        // No agent yet: their player card, cut to the card's shape from the
        // middle of the banner, the way the row has it behind the name.
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
    let dark = Color32::from_rgba_unmultiplied(10, 11, 14, 235);
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

/// The name and what is known about them, set in the dark at the foot of
/// the card.
fn card_words(painter: &egui::Painter, card: Rect, player: &Player) {
    let (name, tag, _standing_in) = board::shown_name(player);
    let room = 2.0f32.mul_add(-space::LG, card.width());
    let font = Face::Heavy.at(30.0);
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
    // The role goes first when the card is narrow, then the agent, whose art
    // is already behind the words.
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
        // On its own chip, because over the art a title in cream is cream on
        // whatever the agent's hair happens to be.
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

/// Why this account is worth a second look, in the backend's words. It is
/// amber because amber is kept for claims about a person, not measurements.
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
                "One Signal Only"
            },
            Face::Heavy.at(15.0),
            if solid { colour::BG } else { colour::WARN },
        );
    }
    ui.add_space(space::SM);
    for reason in &player.smurf_reasons {
        reason_line(ui, reason);
    }
}

/// One reason, in the reading face, its numbers in the flag's amber. A long
/// one wraps inside the panel.
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

/// Where they are now, and the best they have ever been.
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
    // The peak and the last act share a line, since both are about the past.
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

/// The emblem, the rank, the rating and what the last match did to it.
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
    let name = player.rank.clone().unwrap_or_else(|| "Unranked".to_owned());
    let _tier = caps_text(
        &painter,
        pos2(x, rect.center().y - 12.0),
        Align2::LEFT_CENTER,
        &name,
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
        let _delta = caps_text(
            &painter,
            pos2(after.right() + space::LG, rect.center().y + 14.0),
            Align2::LEFT_CENTER,
            &format!("{delta:+}"),
            Face::Heavy.at(16.0),
            tint,
        );
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

/// What was copied, while it is still worth saying.
pub(crate) fn copied(ctx: &egui::Context) -> Option<String> {
    let now = ctx.input(|i| i.time);
    ctx.data(|d| d.get_temp::<(String, f64)>(egui::Id::new(COPIED)))
        .filter(|(_, at)| now - at < SAID)
        .map(|(name, _)| name)
}

/// The egui data key for the last copy.
const COPIED: &str = "overseer-copied";

/// How long the footer says a copy worked, in seconds.
const SAID: f64 = 1.8;

/// Every tag the backend worked out, as wrapping chips with the reason on
/// hover.
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
    let font = Face::Display.at(size::MICRO + 1.0);
    let left = ui.max_rect().left() + space::LG;
    let right = ui.max_rect().right() - space::LG;
    let mut spots = Vec::with_capacity(tags.len());
    let (mut x, mut row) = (left, 0.0_f32);
    for (tag, whole) in tags {
        let wide = 2.0f32.mul_add(space::MD, caps_width(ui.painter(), tag, font.clone()));
        if x + wide > right && x > left {
            x = left;
            row += 1.0;
        }
        spots.push((row, x, wide, tag, whole));
        x += wide + space::SM;
    }
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), (row + 1.0) * CHIP_ROW),
        Sense::hover(),
    );
    let painter = ui.painter().clone();
    for (row, x, wide, tag, whole) in spots {
        let chip = Rect::from_min_size(
            pos2(x, row.mul_add(CHIP_ROW, rect.top()) + 2.0),
            vec2(wide, 18.0),
        );
        let _shown = ui
            .interact(chip, ui.id().with(("read", tag)), Sense::hover())
            .on_hover_ui(|ui| board::tip::auto(ui, whole, board::paint::win(side)));
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
}

/// One line of chips.
const CHIP_ROW: f32 = 22.0;

/// How big the emblem is in the panel.
const EMBLEM: f32 = 52.0;

/// How far through the tier they are, as a bar.
fn rank_bar(ui: &mut Ui, player: &Player, still: bool) {
    let Some(rr) = player.rr else { return };
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::MD), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    // One id for every player, so switching players slides the bar from the
    // last one's rating.
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

/// Recent results, the run they are on, and what they play.
fn form(ui: &mut Ui, player: &Player, side: Side) {
    if player.form.is_empty() && player.top_agents.is_empty() {
        return;
    }
    heading(ui, "Form");
    if !player.form.is_empty() {
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
        if ui.is_rect_visible(rect) {
            pips(ui, player, rect, side);
        }
    }
    let mut agents: Vec<&overseer_core::TopAgent> = player.top_agents.iter().collect();
    agents.sort_by_key(|a| std::cmp::Reverse(a.games.unwrap_or(0)));
    let mut mains: Vec<String> = agents
        .iter()
        .take(3)
        .map(|a| {
            format!(
                "{} {}",
                a.agent.clone().unwrap_or_else(|| "?".to_owned()),
                a.games.unwrap_or(0)
            )
        })
        .collect();
    // As many as fit beside the label, most played first.
    let room = 2.0f32.mul_add(-space::LG, ui.available_width()) - VALUE_X;
    let font = Face::Number.at(size::BODY);
    while mains.len() > 1
        && ui
            .painter()
            .layout_no_wrap(mains.join("   "), font.clone(), colour::TEXT)
            .size()
            .x
            > room
    {
        mains.pop();
    }
    if !mains.is_empty() {
        stat(ui, "Mains", &mains.join("   "), colour::TEXT, "");
    }
}

/// The last five results as pips, and the streak when it is two or more.
fn pips(ui: &Ui, player: &Player, rect: Rect, side: Side) {
    let painter = ui.painter().clone();
    // The row's own pips, at the panel's size: a win in the colour of what
    // it means for you, a loss dim.
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

/// The stats as one row, each saying how many games it is out of, since a
/// bare K/D reads as a career average when it is only the last few matches.
fn numbers(ui: &mut Ui, player: &Player, side: Side) {
    let out_of = |games: Option<u32>| games.map_or_else(String::new, |g| format!("\nOut of {g}"));
    let recent = out_of(u32::try_from(player.form.len()).ok().filter(|n| *n > 0));
    let map = player
        .map_win_rate
        .as_ref()
        .filter(|m| m.games.unwrap_or(0) > 0);
    let owned: Vec<(String, Color32, String)> = [
        Some((
            player.kd.map_or_else(dash, |v| format!("{v:.2}")),
            if board::few_games(player) {
                colour::TEXT_FAINT
            } else {
                board::paint::kd_heat(side, player.kd)
            },
            format!("K/D{recent}"),
        )),
        Some((
            player.hs_pct.map_or_else(dash, |v| format!("{v:.0}%")),
            colour::TEXT,
            format!("HS{recent}"),
        )),
        Some((
            player
                .win_rate
                .map_or_else(dash, |v| format!("{}%", v.round())),
            sample(player.games, 10),
            format!("Win{}", out_of(player.games)),
        )),
        map.map(|m| {
            (
                format!("{}%", m.win_rate.unwrap_or(0.0).round()),
                sample(m.games, 5),
                format!("Map{}", out_of(m.games)),
            )
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    heading(ui, "Stats");
    figures(ui, &figures_of(&owned));
}

/// A rate's colour by how many games are behind it: faint under `few`, the
/// same rule the board's columns follow.
fn sample(games: Option<u32>, few: u32) -> Color32 {
    if games.is_some_and(|g| g < few) {
        colour::TEXT_FAINT
    } else {
        colour::TEXT
    }
}

/// Borrows a row of owned figures for [`figures`].
fn figures_of(owned: &[(String, Color32, String)]) -> Vec<(&str, Color32, &str)> {
    owned
        .iter()
        .map(|(value, tint, under)| (value.as_str(), *tint, under.as_str()))
        .collect()
}

/// A row of figures across the panel, each a number with a word under it,
/// and what it is out of on a line of its own after a newline. When the
/// widest of them will not fit side by side they go onto more rows, evenly,
/// so a label never runs into the figure beside it.
pub(crate) fn figures(ui: &mut Ui, row: &[(&str, Color32, &str)]) {
    let value_font = Face::Heavy.at(20.0);
    let under_font = Face::Display.at(size::MICRO);
    let widest = row
        .iter()
        .map(|&(value, _, under)| {
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
            .map(|&(_, _, under)| under.lines().count())
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
        for (i, &(value, tint, under)) in chunk.iter().enumerate() {
            let x = inner.left() + each * i as f32;
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

/// Who they came with, and how sure we are.
fn group(ui: &mut Ui, player: &Player) {
    if let Some(size) = player
        .party
        .as_ref()
        .and_then(|p| p.size)
        .filter(|s| *s > 1)
    {
        heading(ui, "Party");
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
    heading_tinted(ui, "Probably Together", colour::WARN);
    line(
        ui,
        &stack_word(guess.size.unwrap_or(0)),
        colour::WARN,
        size::BODY,
    );
    // Always with the evidence, because this guess accuses strangers of
    // queueing together.
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

/// How a group of that size is said out loud.
fn stack_word(size: u32) -> String {
    match size {
        0 | 1 => "On their own".to_owned(),
        2 => "A duo".to_owned(),
        3 => "A trio".to_owned(),
        n => format!("A {n} stack"),
    }
}

/// What history you have with them.
fn met(ui: &mut Ui, player: &Player) {
    let Some(encounter) = player.encounter.as_ref().filter(|e| e.total() > 0) else {
        return;
    };
    // Neutral: having met somebody before is history, not a warning, and
    // amber is the flag's.
    heading(ui, &format!("Met {} Times Before", encounter.total()));
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

/// A win, loss and draw record, said the way a player says it.
fn record(wins: Option<u32>, losses: Option<u32>, draws: Option<u32>) -> String {
    let draws = draws.unwrap_or(0);
    let base = format!("{}W-{}L", wins.unwrap_or(0), losses.unwrap_or(0));
    if draws > 0 {
        format!("{base}-{draws}D")
    } else {
        base
    }
}

/// What they are carrying, when the backend could see it.
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
    heading(ui, "Carrying");
    for (weapon, skin) in skins {
        named(ui, &weapon, &skin);
    }
}

/// A label and a word on the same grid as [`stat`], with the word in the
/// reading face because a word in mono looks like a file name.
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

/// How tall a heading's line is.
const HEADING: f32 = 22.0;

/// A section heading: the word in caps with a hairline to the edge.
pub(crate) fn heading(ui: &mut Ui, text: &str) {
    heading_tinted(ui, text, colour::TEXT);
}

/// A section heading in a colour, for the sections that are claims.
fn heading_tinted(ui: &mut Ui, text: &str, tint: Color32) {
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
        Face::Heavy.at(15.0),
        tint,
    );
    let from = drawn.right() + space::MD;
    if from < rect.right() - space::LG {
        painter.hline(
            from..=rect.right() - space::LG,
            rect.center().y + 1.0,
            (1.0, colour::LINE),
        );
    }
}

/// One line of prose, at the panel's left margin. Text wider than the panel
/// wraps, and every row past the first makes the line that much taller.
pub(crate) fn line(ui: &mut Ui, text: &str, tint: Color32, points: f32) {
    let width = ui.available_width();
    let galley = ui.painter().layout(
        text.to_owned(),
        Face::Body.at(points),
        tint,
        2.0f32.mul_add(-space::LG, width),
    );
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

/// A label, a value, and a quieter note after it.
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

/// What a missing value looks like, in one place.
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
    use overseer_ui::{colour, size, space};

    use super::{Notes, record, stack_word};
    use crate::notes::Note;

    /// The boxes are redrawn from the store every frame, so a keystroke not
    /// written back on the same frame vanishes.
    #[test]
    fn what_you_type_about_somebody_is_kept() {
        let player = Player {
            puuid: Some("p1".to_owned()),
            name: Some("Day#9932".to_owned()),
            ..Player::default()
        };
        // Starts with a tag so the section is open. The invite that opens it
        // is painted rather than a widget, so there is no node to click.
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
        // Same first frame rule as every other harness here: the faces are
        // bound on the frame after they are installed.
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

    /// A reason wider than the narrow panel wraps onto more rows rather than
    /// running off the edge, and a short line keeps its one row.
    #[test]
    fn a_long_line_wraps_inside_the_narrow_panel() {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(280.0, 200.0))
            .build_ui_state(
                |ui: &mut Ui, state: &mut (bool, Vec<f32>)| {
                    if !state.0 {
                        return;
                    }
                    state.1.clear();
                    for text in [
                        "No matches on record.",
                        "Their match history could not be read because the service answered 429 and asked for a minute's rest.",
                    ] {
                        let top = ui.cursor().top();
                        super::line(ui, text, colour::TEXT_DIM, size::MICRO);
                        state
                            .1
                            .push(ui.cursor().top() - top - ui.spacing().item_spacing.y);
                    }
                },
                (false, Vec::new()),
            );
        overseer_ui::install_fonts(&harness.ctx);
        harness.run();
        harness.state_mut().0 = true;
        harness.run();
        let &[short, long] = harness.state().1.as_slice() else {
            panic!("expected two lines, got {:?}", harness.state().1);
        };
        assert!(
            (short - space::XL).abs() < 0.5,
            "a one row line took {short} points, not {}",
            space::XL
        );
        assert!(
            long >= space::XL + size::MICRO,
            "the long line took {long} points, so it never wrapped"
        );
    }

    /// The efficient tier draws the emblem with no light behind it and jumps
    /// the rating bar to a new rating. The rich tier is the control: one more
    /// shape for the light, and a bar that slides and so asks for frames.
    #[test]
    fn the_efficient_tier_holds_the_rank_still() {
        let mut drawn = Vec::new();
        for still in [true, false] {
            let ctx = egui::Context::default();
            overseer_ui::install_fonts(&ctx);
            let mut shapes = 0;
            // egui asks for frames of its own at first, so the rating only
            // moves once those have passed.
            for rr in [10, 10, 10, 90] {
                let player = Player {
                    rank: Some("Diamond 2".to_owned()),
                    rank_tier: Some(20),
                    rr: Some(rr),
                    ..Player::default()
                };
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    super::ranks(ui, &player, super::Side::Enemy, still);
                });
                output.textures_delta.clear();
                shapes = output.shapes.len();
            }
            assert_eq!(
                ctx.has_requested_repaint(),
                !still,
                "still: {still}, and a new rating asked for frames it should not have, or none it should"
            );
            drawn.push(shapes);
        }
        let &[held, rich] = drawn.as_slice() else {
            panic!("expected two runs, got {drawn:?}");
        };
        assert_eq!(rich, held + 1, "the light behind the emblem is one shape");
    }

    #[test]
    fn a_record_only_mentions_draws_when_there_were_some() {
        assert_eq!(record(Some(3), Some(1), Some(0)), "3W-1L");
        assert_eq!(record(Some(3), Some(1), Some(2)), "3W-1L-2D");
        assert_eq!(record(None, None, None), "0W-0L");
    }

    #[test]
    fn a_group_is_named_the_way_it_is_said() {
        assert_eq!(stack_word(1), "On their own");
        assert_eq!(stack_word(2), "A duo");
        assert_eq!(stack_word(3), "A trio");
        assert_eq!(stack_word(5), "A 5 stack");
    }
}
