//! The settings screen, and the entry point the snapshot tests draw through.
//! Every switch is on this one screen with a sentence saying what it does,
//! because a setting you have to toggle to understand is one nobody touches.

use egui::{Align2, ScrollArea, Sense, Ui, pos2, vec2};

#[cfg(test)]
use crate::board;
use crate::board::COLUMNS;
use crate::board::grid::Column;
use crate::controls;
use crate::hotkey::{Failure, LABEL};
use crate::lineups::{COLOURS, SIZES, SPEEDS, speed_name, swatch};
use crate::offline::Offline;
use crate::overlay::Corner;
use crate::settings::{MapTurn, Quality, Settings};
#[cfg(test)]
use crate::sort::Sort;
#[cfg(test)]
use crate::{app, panel};
#[cfg(test)]
use egui::{CentralPanel, Panel};
#[cfg(test)]
use overseer_core::Board;
use overseer_ui::{Face, caps_at, caps_text, colour, size, space};

/// What failed to start and why, since each one changes what the screen can
/// promise.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Trouble<'a> {
    /// Why there is no global hotkey.
    pub(crate) hotkey: Option<&'a Failure>,
    /// Why there is no icon beside the clock.
    pub(crate) tray: Option<&'a str>,
}

/// Draws the settings screen. True when something changed and wants saving.
pub(crate) fn settings(
    ui: &mut Ui,
    settings: &mut Settings,
    quality: Quality,
    dropped: bool,
    (trouble, root, offline): (Trouble<'_>, &std::path::Path, &mut Offline),
) -> bool {
    let mut changed = false;
    let still = quality == Quality::Efficient;
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(space::LG);
            title(ui, "Settings", "Press Esc or the gear to go back.");

            offline_chat(ui, offline, still);
            changed |= columns(ui, settings, still);

            section(ui, "Layout", "How the main window is arranged.");
            if switch(
                ui,
                "Detail Panel",
                "Show the selected player's details on the right",
                settings.panel,
                false,
                still,
            ) {
                settings.panel = !settings.panel;
                changed = true;
            }
            if switch(
                ui,
                "Enemies First",
                "List the enemy team above your team",
                settings.enemies_first,
                false,
                still,
            ) {
                settings.enemies_first = !settings.enemies_first;
                changed = true;
            }
            if switch(
                ui,
                "Minimize in Matches",
                "Minimize this window when agent select ends, so closing a browser takes you back to the game",
                settings.minimize_in_matches,
                false,
                still,
            ) {
                settings.minimize_in_matches = !settings.minimize_in_matches;
                changed = true;
            }

            changed |= overlay(ui, settings, trouble, still);

            changed |= clips(ui, settings, still);

            changed |= effort(ui, settings, quality, dropped);
            keys(ui);
            data(ui, root);
            ui.add_space(space::XXL);
        });
    changed
}

/// Offline chat, first since it's what people come to Settings for most.
fn offline_chat(ui: &mut Ui, offline: &mut Offline, still: bool) {
    section(
        ui,
        "Offline Chat",
        "Appear offline to your friends in Riot chat. Nothing changes in the game.",
    );
    let about = match offline.state {
        None => "Asking the backend whether the chat proxy is running",
        Some(s) if !s.running => {
            "Restarts the Riot Client and VALORANT through Overseer, so finish your match first"
        }
        Some(s) if s.enabled && s.connected => "Your friends see you as offline",
        Some(s) if s.enabled => "On, from the moment the Riot Client signs in to chat",
        Some(_) => "Your friends see you online. Switch it back on without a restart",
    };
    let on = offline.on();
    if switch(ui, "Appear Offline", about, on, false, still) && offline.state.is_some() {
        offline.wanted = Some(!on);
    }
    if let Some(said) = offline.said.as_deref() {
        note(ui, said);
    }
}

/// Where everything Overseer keeps lives, with a button to open each folder.
/// It is all plain files, to look at, back up, copy to another PC or change.
fn data(ui: &mut Ui, root: &std::path::Path) {
    section(
        ui,
        "Your Data",
        "Everything Overseer keeps is plain files on this PC, yours to open and change.",
    );
    ui.horizontal_wrapped(|ui| {
        ui.add_space(space::XL);
        for (label, folder) in [
            ("Settings and Notes", root.join(".overseer")),
            ("Lineups", root.join("lineups")),
            ("Match History", root.join("backend").join("data")),
            ("App Folder", root.to_path_buf()),
        ] {
            if controls::button(ui, label, controls::Tone::Plain, true).clicked() {
                // Made first if it isn't there yet, so the button never does
                // nothing. Explorer opens it by itself, and nothing waits on it.
                drop(std::fs::create_dir_all(&folder));
                drop(std::process::Command::new("explorer").arg(&folder).spawn());
            }
        }
    });
    note(
        ui,
        "Settings and Notes holds app.json, your settings, and notes.json, your notes on players. \
         Lineups has a folder per map with each lineup, its clip and its pictures. \
         Match History has your past games and who you met in them. Close Overseer before \
         changing a file, or it may write over the change when it saves.",
    );
}

/// Every key the window listens for, including the ones the footer leaves out.
fn keys(ui: &mut Ui) {
    section(
        ui,
        "Keyboard Shortcuts",
        "These work while the Overseer window is in front.",
    );
    for (key, what) in [
        ("/", "Search for a player or an agent"),
        (",", "Open or close settings"),
        ("h", "Open or close your match history"),
        ("l", "Open or close your lineups"),
        ("w", "Jump to the next player worth a look"),
        (
            "o",
            "Show the overlay for 15 seconds, or hide it. Ctrl+Alt+O does the same in game",
        ),
        ("up, down", "Select the previous or next player"),
        ("n", "Write a note about the selected player"),
        ("ctrl+c", "Copy the selected player's Riot ID"),
        (
            "escape",
            "Clear the search, leave a full screen clip, or close this screen",
        ),
        ("space", "Play or pause a lineup's clip"),
        ("t", "Show or hide every lineup's name on the map"),
        ("r", "Turn the lineup map a quarter turn"),
        (
            "f",
            "Show a lineup's clip full screen, or leave full screen",
        ),
        ("left, right", "Skip a lineup's clip back or on 5 seconds"),
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
fn columns(ui: &mut Ui, settings: &mut Settings, still: bool) -> bool {
    section(
        ui,
        "Columns",
        "Choose which stats show on each player's row.",
    );
    // One long list is a thin ribbon down a wide window, so they go two
    // abreast wherever two fit.
    if ui.available_width() < TWO_ABREAST {
        return switches(ui, &COLUMNS, settings, still);
    }
    let (first, second) = COLUMNS.split_at(COLUMNS.len().div_ceil(2));
    let mut changed = false;
    ui.columns(2, |side| {
        if let [left, right] = side {
            changed |= switches(left, first, settings, still);
            changed |= switches(right, second, settings, still);
        }
    });
    changed
}

/// Below this width the column switches stay in one list.
const TWO_ABREAST: f32 = 760.0;

/// One toggle per column, in the order the board draws them.
fn switches(ui: &mut Ui, columns: &[Column], settings: &mut Settings, still: bool) -> bool {
    let mut changed = false;
    for column in columns {
        let hidden = settings.hidden_columns.iter().any(|h| h == column.head);
        if switch(ui, column.label, column.about, !hidden, false, still) {
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

/// The overlay and tray sections, both about what the app does while the
/// game has the screen.
fn overlay(ui: &mut Ui, settings: &mut Settings, trouble: Trouble<'_>, still: bool) -> bool {
    let mut changed = false;
    section(
        ui,
        "Overlay",
        "A small window over the game that shows the enemy team. It never takes the focus from the game.",
    );
    if switch(
        ui,
        "Overlay in Agent Select",
        "Show it through agent select, until the match loads",
        settings.overlay.auto,
        false,
        still,
    ) {
        settings.overlay.auto = !settings.overlay.auto;
        changed = true;
    }
    for corner in Corner::ALL {
        if switch(
            ui,
            corner.label(),
            corner.about(),
            settings.overlay.corner == corner,
            true,
            still,
        ) {
            settings.overlay.corner = corner;
            changed = true;
        }
    }
    note(
        ui,
        &match trouble.hotkey {
            None => format!(
                "Press {LABEL} in game to turn the overlay on or off. It only shows when VALORANT runs in windowed fullscreen, not fullscreen."
            ),
            Some(Failure::Taken) => format!(
                "Another program is using {LABEL}, so the overlay can only be turned on or off here. Close that program and restart Overseer to use the shortcut."
            ),
            Some(Failure::Refused(_)) => format!(
                "Windows couldn't set up {LABEL}, so the overlay can only be turned on or off here. Restart Overseer to try again."
            ),
        },
    );

    section(ui, "System Tray", "The Overseer icon near the clock.");
    match trouble.tray {
        None => note(
            ui,
            "Closing the window quits Overseer. While it runs, click the icon to bring the window back, or right-click it and choose Quit.",
        ),
        Some(_) => note(
            ui,
            "Overseer couldn't add its tray icon. Reinstalling Overseer puts it back.",
        ),
    }
    changed
}

/// A button that installs yt-dlp and `FFmpeg`, or updates them, with winget in
/// a console of its own so the progress shows. Nothing runs until it's
/// clicked, since this build never updates anything by itself.
fn clip_tools(ui: &mut Ui) {
    ui.add_space(space::SM);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(space::XL);
        if controls::button(ui, "Update Clip Tools", controls::Tone::Plain, true).clicked() {
            use std::os::windows::process::CommandExt as _;
            // CREATE_NEW_CONSOLE. winget installs a missing package and
            // upgrades one that is already there.
            let get = |id: &str| {
                format!(
                    "winget install --id {id} --exact --accept-package-agreements \
                     --accept-source-agreements"
                )
            };
            let script = format!("{} & {} & pause", get("yt-dlp.yt-dlp"), get("Gyan.FFmpeg"));
            drop(
                std::process::Command::new("cmd")
                    .raw_arg(format!("/c {script}"))
                    .creation_flags(0x0000_0010)
                    .spawn(),
            );
        }
    });
    note(
        ui,
        "Clips from a link need yt-dlp, and cutting and playing them needs FFmpeg. This \
         installs both, or updates them, with winget. A link that stops downloading usually \
         means yt-dlp is out of date, since video sites change often.",
    );
}

/// How a lineup's clip plays when it first shows. True when something changed.
fn clips(ui: &mut Ui, settings: &mut Settings, still: bool) -> bool {
    let mut changed = false;
    section(
        ui,
        "Lineup Clips",
        "How a lineup's video plays when you open it.",
    );
    if switch(
        ui,
        "Autoplay",
        "Play a lineup's clip as soon as you pick it",
        settings.lineups.clip_autoplay,
        false,
        still,
    ) {
        settings.lineups.clip_autoplay = !settings.lineups.clip_autoplay;
        changed = true;
    }
    if switch(
        ui,
        "Loop",
        "Start a clip over when it ends, so you can watch the throw again",
        settings.lineups.clip_loop,
        false,
        still,
    ) {
        settings.lineups.clip_loop = !settings.lineups.clip_loop;
        changed = true;
    }
    let speeds: Vec<(f32, String)> = SPEEDS.iter().map(|&s| (s, speed_name(s))).collect();
    changed |= choice(ui, "Speed", &speeds, &mut settings.lineups.clip_speed);
    let volumes: Vec<(f32, String)> = [0.0, 25.0, 50.0, 75.0, 100.0]
        .iter()
        .map(|&v| {
            (
                v,
                if v == 0.0 {
                    "Muted".to_owned()
                } else {
                    format!("{v}%")
                },
            )
        })
        .collect();
    changed |= choice(ui, "Volume", &volumes, &mut settings.lineups.clip_volume);
    let sizes: Vec<(f32, String)> = SIZES.iter().map(|&(s, n)| (s, n.to_owned())).collect();
    changed |= choice(ui, "Size", &sizes, &mut settings.lineups.clip_size);
    clip_tools(ui);

    section(ui, "Lineup Maps", "How each map is drawn.");
    changed |= area_colour(ui, settings);
    if switch(
        ui,
        "Lineup Names",
        "Start with every lineup's name on the map. Names above the map, or T, switches it until you leave Lineups",
        settings.lineups.lineup_names,
        false,
        still,
    ) {
        settings.lineups.lineup_names = !settings.lineups.lineup_names;
        changed = true;
    }
    for turn in MapTurn::ALL {
        let (name, about) = turn.label();
        if switch(
            ui,
            name,
            about,
            settings.lineups.map_turn == turn,
            true,
            still,
        ) {
            settings.lineups.map_turn = turn;
            changed = true;
        }
    }
    changed
}

/// The colour a molly's area is drawn in: By Side, or one of the drawing
/// colours. True when one was picked.
fn area_colour(ui: &mut Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.set_min_height(space::ROW + space::SM);
        ui.add_space(space::XL);
        let (rect, _response) = ui.allocate_exact_size(vec2(96.0, space::ROW), Sense::hover());
        let _drawn = caps_text(
            ui.painter(),
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            "Area Colour",
            Face::Heavy.at(size::BODY),
            colour::TEXT,
        );
        if controls::chip(ui, "By Side", settings.lineups.area_colour.is_none()).clicked() {
            settings.lineups.area_colour = None;
            changed = true;
        }
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for hex in COLOURS {
            let on = settings
                .lineups
                .area_colour
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(hex));
            if swatch(ui, hex, on).clicked() {
                settings.lineups.area_colour = Some(hex.to_owned());
                changed = true;
            }
        }
    });
    changed
}

/// A row of chips for one value, with its name on the left. True when one
/// was picked.
fn choice(ui: &mut Ui, name: &str, options: &[(f32, String)], value: &mut f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_height(space::ROW + space::SM);
        ui.add_space(space::XL);
        let (rect, _response) = ui.allocate_exact_size(vec2(96.0, space::ROW), Sense::hover());
        let _drawn = caps_text(
            ui.painter(),
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            name,
            Face::Heavy.at(size::BODY),
            colour::TEXT,
        );
        for (option, label) in options {
            if controls::chip(ui, label, (*value - option).abs() < 0.001).clicked() {
                *value = *option;
                changed = true;
            }
        }
    });
    changed
}

/// How much the window may spend on looking good. True when it changed.
fn effort(ui: &mut Ui, settings: &mut Settings, quality: Quality, dropped: bool) -> bool {
    let mut changed = false;
    let still = quality == Quality::Efficient;
    section(ui, "Graphics", "Animations and visual effects.");
    for option in [Quality::Auto, Quality::Efficient, Quality::Rich] {
        let about = match option {
            Quality::Auto => "Full effects, switching to Efficient if the window starts to lag",
            Quality::Efficient => "No animations or glow, which uses the least of your PC",
            Quality::Rich => "Every animation and effect",
        };
        if switch(
            ui,
            option.label(),
            about,
            settings.quality == option,
            true,
            still,
        ) {
            settings.quality = option;
            changed = true;
        }
    }
    if dropped && settings.quality == Quality::Auto {
        note(
            ui,
            "Switched to Efficient because the window was lagging. Pick Full or Efficient to choose yourself.",
        );
    } else {
        note(ui, &format!("Currently using {}.", quality.label()));
    }
    changed
}

/// The screen's own title, and a sentence under it.
pub(crate) fn title(ui: &mut Ui, text: &str, about: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XXL), Sense::hover());
    if ui.is_rect_visible(rect) {
        caps_at(
            ui.painter(),
            pos2(rect.left() + space::XL, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            Face::Heavy.at(size::DISPLAY),
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

/// A section heading: the name, a sentence about it, and a hairline to the
/// edge.
fn section(ui: &mut Ui, text: &str, about: &str) {
    ui.add_space(space::XL);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let drawn = caps_text(
        &painter,
        pos2(rect.left() + space::XL, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        Face::Heavy.at(17.0),
        colour::TEXT_STRONG,
    );
    let about_at = painter.text(
        pos2(drawn.right() + space::LG, rect.center().y + 1.0),
        Align2::LEFT_CENTER,
        about,
        Face::Body.at(size::MICRO + 1.0),
        colour::TEXT_FAINT,
    );
    if about_at.right() + space::MD < rect.right() - space::XL {
        painter.hline(
            about_at.right() + space::MD..=rect.right() - space::XL,
            rect.center().y + 1.0,
            (1.0, colour::LINE),
        );
    }
}

/// One switch, true when clicked. `one_of` draws it as one choice among
/// siblings instead of an on/off pip, so nobody tries to pick two corners.
fn switch(ui: &mut Ui, name: &str, about: &str, on: bool, one_of: bool, still: bool) -> bool {
    overseer_ui::choice(ui, name, about, on, one_of, still)
}

/// A line of explanation under a group, wrapped to the screen, since the
/// hotkey and tray notes run past a narrow window.
pub(crate) fn note(ui: &mut Ui, text: &str) {
    let galley = ui.painter().layout(
        text.to_owned(),
        Face::Body.at(size::MICRO),
        colour::TEXT_DIM,
        2.0f32.mul_add(-space::XL, ui.available_width()).max(120.0),
    );
    let tall = galley.size().y.max(space::ROW - space::MD) + space::MD;
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), tall), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let top = rect.center().y - galley.size().y / 2.0;
    ui.painter()
        .galley(pos2(rect.left() + space::XL, top), galley, colour::TEXT_DIM);
}

/// One frame of the app, for the snapshot and budget tests to draw.
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
    let panel_width = app::panel_width(width, None);
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
                .vline(all.left(), all.y_range(), (1.0, colour::VOID));
            // A dark stroke then a light one, the same cut that separates
            // the rows, so the panel reads as a column beside the board.
            ui.painter().vline(
                all.left() + 1.0,
                all.y_range(),
                (1.0, colour::TEXT_STRONG.gamma_multiply(0.07)),
            );
            ui.add_space(space::MD);
            let player = selected
                .and_then(|id| board.players.iter().find(|p| p.name.as_deref() == Some(id)));
            let side = player.map_or(board::Side::Enemy, |p| board::side_of(board, p));
            panel::show(ui, player, side, notes, career, false);
        });
}

/// The board and the panel as the window draws them, for the snapshot and
/// budget tests.
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
            if board.players.is_empty() {
                app::snapshot_empty(ui);
                return;
            }
            // Fixtures pick by name, and the board keys by account the way
            // the window does.
            let chosen = selected
                .and_then(|name| {
                    board
                        .players
                        .iter()
                        .find(|p| p.name.as_deref() == Some(name))
                })
                .and_then(|p| p.puuid.as_deref());
            // `since` is well past the entrance animation, so the picture is
            // of a settled board.
            let _touched = board::draw(
                ui,
                &board::Scene {
                    board,
                    sort,
                    filter: "",
                    selected: chosen,
                    notes,
                    hidden: &settings.hidden_columns,
                    enemies_first: settings.enemies_first,
                    place: board::Place::Window,
                    still: false,
                    since: 10.0,
                },
            );
        });
}
