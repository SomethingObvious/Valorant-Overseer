//! The settings screen, and the one door the snapshot test draws through.
//!
//! Every switch in the app is on one screen, in the order somebody would look
//! for them, with a sentence saying what each one is for. A setting whose
//! effect you have to discover by toggling it is a setting nobody touches.

use egui::{Align2, Rect, ScrollArea, Sense, Ui, pos2, vec2};

use crate::board::{COLUMNS, Column};
use crate::overlay::Corner;
use crate::settings::{Quality, Settings};
#[cfg(test)]
use crate::sort::Sort;
use overseer_ui::{Face, caps_at, caps_text, colour, shape, size, space};

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
                "Whoever the pointer is over, in full, on the right",
                settings.panel,
                false,
            ) {
                settings.panel = !settings.panel;
                changed = true;
            }
            if switch(
                ui,
                "enemies first",
                "The other five above your own, because the game already shows you yours",
                settings.enemies_first,
                false,
            ) {
                settings.enemies_first = !settings.enemies_first;
                changed = true;
            }

            changed |= overlay(ui, settings, trouble);

            changed |= effort(ui, settings, quality, dropped);
            keys(ui);
            ui.add_space(space::XXL);
        });
    changed
}

/// Every key this window listens for.
///
/// The footer carries the four worth a keycap; this carries all of them,
/// because a key nobody can find is a key nobody presses, and the settings
/// screen is where a person goes to find out what an app can do.
fn keys(ui: &mut Ui) {
    section(ui, "keys", "Everything the window listens for.");
    for (key, what) in [
        ("/", "Find a name or an agent. Escape clears it."),
        (",", "This screen, and back again."),
        (
            "w",
            "The next account worth a look, wrapping round the lobby.",
        ),
        (
            "o",
            "The overlay, on and off. Ctrl+Alt+O does it from in game.",
        ),
        ("up, down", "Move the panel through the roster."),
        ("ctrl+c", "Copy the name of whoever the panel is about."),
        ("escape", "Clear the search, then leave the screen."),
    ] {
        let (rect, _response) =
            ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter();
        let plate =
            overseer_ui::keycap(painter, pos2(rect.left() + space::XL, rect.center().y), key);
        painter.text(
            pos2(
                plate.right().max(rect.left() + 190.0) + space::LG,
                rect.center().y,
            ),
            Align2::LEFT_CENTER,
            what,
            Face::Body.at(size::MICRO),
            colour::TEXT_FAINT,
        );
    }
}

/// Which columns the board shows. True when one was switched.
fn columns(ui: &mut Ui, settings: &mut Settings) -> bool {
    section(ui, "columns", "What the board shows about each player.");
    // Twelve in one list is a thin ribbon down the middle of a wide window
    // with nothing beside it, and twelve rows of scrolling to reach the next
    // section. Two abreast wherever two fit, which halves both.
    if ui.available_width() < TWO_ABREAST {
        return switches(ui, &COLUMNS, settings);
    }
    let (first, second) = COLUMNS.split_at(COLUMNS.len().div_ceil(2));
    let mut changed = false;
    ui.columns(2, |side| {
        if let [left, right] = side {
            changed |= switches(left, first, settings);
            changed |= switches(right, second, settings);
        }
    });
    changed
}

/// The width at which the column list stops being one list.
///
/// Two of them need a name, an explanation and the air between, which is
/// what the switch itself asks for twice over.
const TWO_ABREAST: f32 = 760.0;

/// One toggle per column, in the order the board draws them.
fn switches(ui: &mut Ui, columns: &[Column], settings: &mut Settings) -> bool {
    let mut changed = false;
    for column in columns {
        let hidden = settings.hidden_columns.iter().any(|h| h == column.head);
        if switch(ui, column.head, column.about, !hidden, false) {
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
        false,
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
            true,
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
        if switch(ui, option.label(), about, settings.quality == option, true) {
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
        caps_at(
            ui.painter(),
            pos2(rect.left() + space::XL, rect.center().y),
            Align2::LEFT_CENTER,
            text,
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

/// A group of switches, with a rule over it and a tick before it.
fn section(ui: &mut Ui, text: &str, about: &str) {
    ui.add_space(space::XL);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    // A band rather than a rule. A settings screen is a stack of groups, and
    // a hairline says where one ends without saying that the next one is a
    // thing: the eye reads a page of hairlines as one long list. The same
    // lit surface the team headings get, at the same width as the switches
    // under it, and the screen becomes a stack of cards instead.
    let middle = rect.center().y + space::SM;
    let band = Rect::from_min_max(
        pos2(rect.left() + space::LG, middle - 11.0),
        pos2(rect.right() - space::LG, middle + 11.0),
    );
    painter.add(shape::lit(
        band,
        shape::CHAMFER,
        colour::BG_RAISED,
        shape::blend(colour::BG_RAISED, colour::BG, 0.55),
    ));
    painter.add(shape::tick(
        pos2(band.left() + space::MD, middle - 5.0),
        10.0,
        colour::ENEMY,
    ));
    let drawn = caps_text(
        &painter,
        pos2(band.left() + space::MD + space::MD, middle),
        Align2::LEFT_CENTER,
        text,
        Face::Display.at(size::LABEL),
        colour::TEXT_DIM,
    );
    painter.text(
        pos2(drawn.right() + space::LG, middle),
        Align2::LEFT_CENTER,
        about,
        Face::Body.at(size::MICRO),
        colour::TEXT_FAINT,
    );
}

/// One switch: a mark, a name, and what it is for. True when clicked.
///
/// `one_of` says whether this is a choice among siblings or a thing that is
/// simply on or off, and it changes the mark: a disc for a choice, a square
/// for a switch. Drawing both the same way is how a settings screen ends up
/// with somebody trying to turn two corners on at once.
fn switch(ui: &mut Ui, name: &str, about: &str, on: bool, one_of: bool) -> bool {
    overseer_ui::choice(ui, name, about, on, one_of)
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

/// The detail panel, for the snapshot only: the window builds its own.
#[cfg(test)]
fn beside(
    ui: &mut Ui,
    width: f32,
    board: &Board,
    selected: Option<&str>,
    notes: &mut crate::notes::Notes,
    career: &crate::career::Career,
) {
    let panel_width = if width >= app::WIDE { 340.0 } else { 280.0 };
    Panel::right("detail")
        .exact_size(panel_width)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let all = ui.max_rect();
            ui.painter().add(egui::Shape::gradient_rect(
                all,
                egui::Direction::TopDown,
                [colour::BG_RAISED, colour::BG],
            ));
            ui.painter()
                .vline(all.left(), all.y_range(), (1.0, colour::LINE));
            ui.add_space(space::MD);
            let player = selected
                .and_then(|id| board.players.iter().find(|p| p.name.as_deref() == Some(id)));
            panel::show(ui, player, notes, career);
        });
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
    app::snapshot_chrome(ui, board, 12);
    if width >= app::COMPACT && settings.panel {
        beside(ui, width, board, selected, notes, career);
    }
    CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let all = ui.max_rect();
            ui.painter().add(egui::Shape::gradient_rect(
                all,
                egui::Direction::TopDown,
                [colour::BG, colour::VOID],
            ));
            ui.add_space(space::MD);
            let height = if width < app::COMPACT {
                space::ROW_TIGHT
            } else {
                space::ROW
            };
            if board.players.is_empty() {
                app::snapshot_empty(ui);
                return;
            }
            for (label, tint, team) in board::teams(board, settings.enemies_first) {
                let mut players = board.team(&team);
                crate::sort::apply(&mut players, sort);
                if players.is_empty() {
                    continue;
                }
                let block = board::open_block(ui);
                let _jumped = board::team_heading(ui, label, tint, board, &team);
                // Same id scope as the window uses, for the same reason.
                ui.push_id(&team, |ui| {
                    board::headings(ui, ui.available_width(), &settings.hidden_columns, sort);
                });
                let brackets = board::brackets(&players);
                for (at, player) in players.iter().enumerate() {
                    let style = RowStyle {
                        team: tint,
                        selected: player.name.as_deref() == selected,
                        height,
                        noted: player.puuid.as_deref().is_some_and(|id| notes.has(id)),
                        bracket: brackets.get(at).copied().flatten(),
                        arrive: 1.0,
                        pace: Pace {
                            hover: overseer_ui::motion::EFFICIENT,
                            select: overseer_ui::motion::EFFICIENT,
                        },
                    };
                    board::row(ui, player, &style, &settings.hidden_columns);
                }
                board::close_block(ui, block, false);
                ui.add_space(space::XL);
            }
            board::board_foot(ui, board, false);
        });
}
