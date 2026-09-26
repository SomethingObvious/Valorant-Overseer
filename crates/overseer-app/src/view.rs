//! The settings screen, and the one door the snapshot test draws through.
//!
//! Every switch in the app is on one screen, in the order somebody would look
//! for them, with a sentence saying what each one is for. A setting whose
//! effect you have to discover by toggling it is a setting nobody touches.

use egui::{Align2, ScrollArea, Sense, Ui, pos2, vec2};

use crate::board::COLUMNS;
use crate::overlay::Corner;
use crate::settings::{Quality, Settings};
#[cfg(test)]
use crate::sort::Sort;
use overseer_ui::{Face, colour, label_text, size, space};

/// What did not start, and why, for the screen to say out loud.
///
/// Both are things the machine can refuse rather than things the app got
/// wrong, and both change what the rest of the screen promises: without a
/// hotkey the overlay can only be switched off from here, and without a tray
/// the close button still means close.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Trouble<'a> {
    /// Why there is no global hotkey.
    pub(crate) hotkey: Option<&'a str>,
    /// Why there is no icon beside the clock.
    pub(crate) tray: Option<&'a str>,
}

/// Draws the settings screen. True when something changed and wants saving.
pub(crate) fn settings(
    ui: &mut Ui,
    settings: &mut Settings,
    quality: Quality,
    dropped: bool,
    trouble: Trouble<'_>,
) -> bool {
    let mut changed = false;
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(space::LG);
            title(
                ui,
                "settings",
                "Everything the window can be told. Press comma to go back.",
            );

            changed |= columns(ui, settings);

            section(ui, "layout", "How much of the window the board gets.");
            if switch(
                ui,
                "detail panel",
                "The selected player, in full, on the right",
                settings.panel,
            ) {
                settings.panel = !settings.panel;
                changed = true;
            }

            changed |= overlay(ui, settings, trouble);

            changed |= effort(ui, settings, quality, dropped);
            ui.add_space(space::XXL);
        });
    changed
}

/// Which columns the board shows. True when one was switched.
fn columns(ui: &mut Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    section(ui, "columns", "What the board shows about each player.");
    for column in &COLUMNS {
        let hidden = settings.hidden_columns.iter().any(|h| h == column.head);
        if switch(ui, column.head, column.about, !hidden) {
            changed = true;
            if hidden {
                settings.hidden_columns.retain(|h| h != column.head);
            } else {
                settings.hidden_columns.push(column.head.to_owned());
            }
        }
    }
    changed
}

/// The overlay and the tray, which are one thought: what the app looks like
/// while the game has the screen.
fn overlay(ui: &mut Ui, settings: &mut Settings, trouble: Trouble<'_>) -> bool {
    let mut changed = false;
    section(
        ui,
        "overlay",
        "A second window in a corner of the screen, over the game, taking no clicks.",
    );
    if switch(
        ui,
        "overlay",
        "No title bar, always on top, and the board stays where it is. [o]",
        settings.overlay,
    ) {
        settings.overlay = !settings.overlay;
        changed = true;
    }
    for corner in Corner::ALL {
        if switch(
            ui,
            corner.label(),
            corner.about(),
            settings.corner == corner,
        ) {
            settings.corner = corner;
            changed = true;
        }
    }
    note(
        ui,
        &format!(
            "{} switches it from inside the game, because nothing in the overlay takes a click. Run VALORANT borderless: nothing can draw over true fullscreen.",
            crate::hotkey::LABEL
        ),
    );
    if let Some(why) = trouble.hotkey {
        note(
            ui,
            &format!("No hotkey, so the overlay only goes away from here. {why}"),
        );
    }

    section(ui, "tray", "The icon beside the clock.");
    match trouble.tray {
        None => note(
            ui,
            "Closing the window hides it there rather than quitting. Left click brings it back, and Quit is in its menu.",
        ),
        Some(why) => note(
            ui,
            &format!("There isn't one, so closing the window quits. {why}"),
        ),
    }
    changed
}

/// How much the window may spend on looking good. True when it changed.
fn effort(ui: &mut Ui, settings: &mut Settings, quality: Quality, dropped: bool) -> bool {
    let mut changed = false;
    section(
        ui,
        "effort",
        "How much the window is allowed to spend on looking good.",
    );
    for option in [Quality::Auto, Quality::Efficient, Quality::Rich] {
        let about = match option {
            Quality::Auto => "Rich, until three frames in a row miss their budget",
            Quality::Efficient => "No movement at all. Same information, calmer",
            Quality::Rich => "Every animation the design allows",
        };
        if switch(ui, option.label(), about, settings.quality == option) {
            settings.quality = option;
            changed = true;
        }
    }
    if dropped && settings.quality == Quality::Auto {
        note(
            ui,
            "Dropped to efficient: three frames in a row missed their budget. Choosing a tier by hand ends this.",
        );
    } else {
        note(ui, &format!("Running {}.", quality.label()));
    }
    changed
}

/// The screen's own title, and a sentence under it.
fn title(ui: &mut Ui, text: &str, about: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XXL), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            pos2(rect.left() + space::XL, rect.center().y),
            Align2::LEFT_CENTER,
            label_text(text),
            Face::Display.at(size::DISPLAY),
            colour::TEXT_STRONG,
        );
    }
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            pos2(rect.left() + space::XL, rect.center().y),
            Align2::LEFT_CENTER,
            about,
            Face::Body.at(size::BODY),
            colour::TEXT_FAINT,
        );
    }
}

/// A group of switches, with a rule over it.
fn section(ui: &mut Ui, text: &str, about: &str) {
    ui.add_space(space::XL);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    painter.hline(rect.x_range(), rect.top(), (1.0, colour::LINE));
    let drawn = painter.text(
        pos2(rect.left() + space::XL, rect.center().y + space::SM),
        Align2::LEFT_CENTER,
        label_text(text),
        Face::Display.at(size::LABEL),
        colour::TEXT_DIM,
    );
    painter.text(
        pos2(drawn.right() + space::LG, rect.center().y + space::SM),
        Align2::LEFT_CENTER,
        about,
        Face::Body.at(size::MICRO),
        colour::TEXT_FAINT,
    );
}

/// One switch: a mark, a name, and what it is for. True when clicked.
fn switch(ui: &mut Ui, name: &str, about: &str, on: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    let painter = ui.painter().clone();
    if response.hovered() {
        painter.rect_filled(rect, 0, colour::BG_HOVER);
    }
    // A filled square is on and an outlined one is off, which reads at a
    // glance and needs no label of its own.
    let box_rect = egui::Rect::from_min_size(
        pos2(rect.left() + space::XL, rect.center().y - 5.0),
        vec2(10.0, 10.0),
    );
    if on {
        painter.rect_filled(box_rect, 0, colour::INFO);
    } else {
        painter.rect_stroke(
            box_rect,
            0,
            egui::Stroke::new(1.0, colour::LINE),
            egui::StrokeKind::Inside,
        );
    }
    let drawn = painter.text(
        pos2(box_rect.right() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        Face::Body.at(size::BODY),
        if on {
            colour::TEXT_STRONG
        } else {
            colour::TEXT_DIM
        },
    );
    painter.text(
        pos2(
            drawn.right().max(rect.left() + 180.0) + space::LG,
            rect.center().y,
        ),
        Align2::LEFT_CENTER,
        about,
        Face::Body.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    response.clicked()
}

/// A line of explanation under a group.
fn note(ui: &mut Ui, text: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().text(
        pos2(rect.left() + space::XL, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        Face::Body.at(size::MICRO),
        colour::TEXT_DIM,
    );
}

/// The board and the panel, drawn the way the window draws them, for the
/// snapshot test. One door, so the test cannot drift away from the app.
#[cfg(test)]
use crate::board::{self, Pace, RowStyle};
#[cfg(test)]
use crate::{app, panel};
#[cfg(test)]
use egui::{CentralPanel, Panel};
#[cfg(test)]
use overseer_core::Board;
#[cfg(test)]
use overseer_ui::motion;

/// One frame of the app, for the snapshot and budget tests to draw.
///
/// A struct rather than a parameter list because the list is now everything
/// the window holds, and seven positional arguments is a call nobody can
/// read at the site.
#[cfg(test)]
pub(crate) struct Shown<'a> {
    /// What the bridge last sent.
    pub(crate) board: &'a Board,
    /// Who the panel is about, by name rather than by id, because a fixture
    /// is written by a person.
    pub(crate) selected: Option<&'a str>,
    /// Every switch.
    pub(crate) settings: &'a Settings,
    /// How the board is ordered.
    pub(crate) sort: &'a Sort,
    /// What has been written about them.
    pub(crate) notes: &'a mut crate::notes::Notes,
    /// The selected player's history.
    pub(crate) career: &'a crate::career::Career,
}

#[cfg(test)]
pub(crate) fn snapshot(ui: &mut Ui, shown: Shown<'_>) {
    let Shown {
        board,
        selected,
        settings,
        sort,
        notes,
        career,
    } = shown;
    let width = ui.available_width();
    app::snapshot_header(ui, board);
    if width >= app::COMPACT && settings.panel {
        let panel_width = if width >= app::WIDE { 340.0 } else { 280.0 };
        Panel::right("detail")
            .exact_size(panel_width)
            .frame(
                egui::Frame::NONE
                    .fill(colour::BG_RAISED)
                    .stroke(egui::Stroke::new(1.0, colour::LINE)),
            )
            .show(ui, |ui| {
                ui.add_space(space::MD);
                let player = selected
                    .and_then(|id| board.players.iter().find(|p| p.name.as_deref() == Some(id)));
                panel::show(ui, player, notes, career);
            });
    }
    CentralPanel::default()
        .frame(egui::Frame::NONE.fill(colour::BG))
        .show(ui, |ui| {
            ui.add_space(space::MD);
            let height = if width < app::COMPACT {
                space::ROW_TIGHT
            } else {
                space::ROW
            };
            for (label, tint, team) in board::teams(board) {
                let mut players = board.team(&team);
                crate::sort::apply(&mut players, sort);
                if players.is_empty() {
                    continue;
                }
                board::team_heading(ui, label, tint, board, &team);
                // Same id scope as the window uses, for the same reason.
                ui.push_id(&team, |ui| {
                    board::headings(ui, ui.available_width(), &settings.hidden_columns, sort);
                });
                for player in players {
                    let style = RowStyle {
                        team: tint,
                        selected: player.name.as_deref() == selected,
                        height,
                        noted: player.puuid.as_deref().is_some_and(|id| notes.has(id)),
                        pace: Pace {
                            hover: motion::EFFICIENT,
                            select: motion::EFFICIENT,
                        },
                    };
                    board::row(ui, player, &style, &settings.hidden_columns);
                }
                ui.add_space(space::XL);
            }
        });
}
