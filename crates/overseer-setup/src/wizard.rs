//! The three steps, and the window they are drawn in.
//!
//! Look, choose, install. Every step says what the next one will do before it
//! does it, and the last one shows the installer's own words rather than a
//! spinner, because the one thing a person wants from an installer that has
//! gone wrong is the line where it went wrong.

use std::path::PathBuf;

use eframe::{App, CreationContext, Frame};
use egui::{Align2, CentralPanel, Panel, Rect, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_ui::{Face, colour, label_text, size, space};

use crate::plan::{self, Finding, Profile, REGIONS, Survey};
use crate::run::{self, Line, Running};

/// Which step is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// What this machine has.
    Look,
    /// What to install, and where you play.
    Choose,
    /// Doing it, with the log.
    Install,
}

/// The wizard's state.
pub(crate) struct Wizard {
    root: PathBuf,
    survey: Survey,
    step: Step,
    profile: Profile,
    region: usize,
    running: Option<Running>,
    log: Vec<String>,
    outcome: Option<bool>,
}

impl std::fmt::Debug for Wizard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wizard")
            .field("step", &self.step)
            .field("profile", &self.profile)
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}

impl Wizard {
    /// Looks at the machine and opens on what it found.
    pub(crate) fn new(cc: &CreationContext<'_>, root: PathBuf) -> Self {
        overseer_ui::install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx
            .set_style_of(egui::Theme::Dark, overseer_ui::style());
        let survey = plan::survey(&root);
        Self {
            root,
            survey,
            step: Step::Look,
            profile: Profile::App,
            region: 0,
            running: None,
            log: Vec::new(),
            outcome: None,
        }
    }

    /// Takes whatever the installer has said since the last frame.
    fn pump(&mut self) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        for line in running.drain() {
            match line {
                Line::Said(text) => {
                    let trimmed = text.trim_end().to_owned();
                    if !trimmed.is_empty() {
                        self.log.push(trimmed);
                    }
                }
                Line::Done(ok) => self.outcome = Some(ok),
            }
        }
    }

    /// Starts the installer and moves to the last step.
    fn begin(&mut self, ctx: &egui::Context) {
        let region = REGIONS.get(self.region).map_or("na", |r| r.0);
        let waker = ctx.clone();
        self.log.clear();
        self.outcome = None;
        self.running = Some(run::start(&self.root, self.profile, region, move || {
            waker.request_repaint();
        }));
        self.step = Step::Install;
    }
}

impl App for Wizard {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        self.pump();
        let chrome = egui::Frame::NONE.fill(colour::BG);
        Panel::top("head")
            .exact_size(space::XXL + space::XL)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| header(ui, self.step));
        Panel::bottom("foot")
            .exact_size(space::XXL + space::MD)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| {
                let action = footer(ui, self);
                if let Some(next) = action {
                    match next {
                        Action::Choose => self.step = Step::Choose,
                        Action::Back => self.step = Step::Look,
                        Action::Install => self.begin(ui.ctx()),
                        Action::Close => {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
            });
        CentralPanel::default()
            .frame(chrome)
            .show(ui, |ui| match self.step {
                Step::Look => look(ui, &self.survey),
                Step::Choose => choose(ui, &mut self.profile, &mut self.region),
                Step::Install => install(ui, &self.log, self.outcome, self.profile),
            });
    }
}

/// What the footer's button does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// On to the choice.
    Choose,
    /// Back to the findings.
    Back,
    /// Start installing.
    Install,
    /// Finished, close the window.
    Close,
}

/// The wizard's title and where you are in it.
fn header(ui: &mut Ui, step: Step) {
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::XXL + space::XL),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let middle = rect.center().y - space::SM;
    let after = painter.text(
        pos2(rect.left() + space::XL, middle),
        Align2::LEFT_CENTER,
        label_text("valorant"),
        Face::Display.at(size::TITLE),
        colour::ENEMY,
    );
    let after = painter.text(
        pos2(after.right() + space::MD, middle),
        Align2::LEFT_CENTER,
        label_text("overseer"),
        Face::Display.at(size::TITLE),
        colour::TEXT_STRONG,
    );
    painter.text(
        pos2(after.right() + space::LG, middle + 1.0),
        Align2::LEFT_CENTER,
        label_text("setup"),
        Face::Display.at(size::LABEL),
        colour::TEXT_DIM,
    );

    // Three marks, one per step, filled up to where you are. A progress bar
    // for three steps is a lie about how long this takes.
    let mut x = rect.right() - space::XL;
    for at in [Step::Install, Step::Choose, Step::Look] {
        let done = step_index(step) >= step_index(at);
        let mark = Rect::from_min_size(pos2(x - 18.0, middle - 2.0), vec2(14.0, 3.0));
        painter.rect_filled(mark, 0, if done { colour::INFO } else { colour::LINE });
        x -= 22.0;
    }
    painter.hline(rect.x_range(), rect.bottom() - 1.0, (1.0, colour::LINE));
}

/// How far along a step is, for the marks.
const fn step_index(step: Step) -> u8 {
    match step {
        Step::Look => 0,
        Step::Choose => 1,
        Step::Install => 2,
    }
}

/// Step one: what the machine has.
pub(crate) fn look(ui: &mut Ui, survey: &Survey) {
    title(
        ui,
        "what this pc has",
        "Nothing has been changed yet. This is only a look.",
    );
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for finding in &survey.findings {
                row(ui, finding);
            }
            if survey.blocked() {
                ui.add_space(space::LG);
                note(
                    ui,
                    "The install cannot go ahead until the red line above is dealt with.",
                    colour::ENEMY,
                );
            }
        });
}

/// One finding: a mark, what was looked at, and what was found.
fn row(ui: &mut Ui, finding: &Finding) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let tint = if finding.blocking {
        colour::ENEMY
    } else {
        colour::ALLY
    };
    let mark = Rect::from_min_size(
        pos2(rect.left() + space::XL, rect.center().y - 4.0),
        vec2(8.0, 8.0),
    );
    painter.rect_filled(mark, 0, tint);
    painter.text(
        pos2(mark.right() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        &finding.what,
        Face::Body.at(size::BODY),
        colour::TEXT_STRONG,
    );
    painter.text(
        pos2(rect.left() + 190.0, rect.center().y),
        Align2::LEFT_CENTER,
        &finding.detail,
        Face::Body.at(size::MICRO),
        colour::TEXT_DIM,
    );
}

/// Step two: which front ends, and which region.
pub(crate) fn choose(ui: &mut Ui, profile: &mut Profile, region: &mut usize) {
    title(
        ui,
        "what to install",
        "You can run this again later and change your mind.",
    );
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for option in Profile::ALL {
                if option_row(ui, option.title(), option.about(), *profile == option) {
                    *profile = option;
                }
            }
            ui.add_space(space::XL);
            heading(ui, "where you play");
            note(
                ui,
                "So the backend talks to the right Riot servers.",
                colour::TEXT_FAINT,
            );
            for (index, (key, name)) in REGIONS.iter().enumerate() {
                if option_row(ui, name, key, *region == index) {
                    *region = index;
                }
            }
        });
}

/// Step three: the installer's own words.
pub(crate) fn install(ui: &mut Ui, log: &[String], outcome: Option<bool>, profile: Profile) {
    let (heading_text, detail) = match outcome {
        None => (
            "installing",
            "This takes a minute the first time, while Python is fetched.",
        ),
        Some(true) => ("done", "The desktop shortcut opens it. You can close this."),
        Some(false) => (
            "that did not work",
            "The reason is in the last few lines below.",
        ),
    };
    title(ui, heading_text, detail);
    if outcome == Some(true) {
        note(
            ui,
            &format!("The shortcut opens {}.", profile.shortcut()),
            colour::ALLY,
        );
    }
    let frame = egui::Frame::NONE
        .fill(colour::BG_RAISED)
        .stroke(egui::Stroke::new(1.0, colour::LINE))
        .inner_margin(egui::Margin::symmetric(space::LG as i8, space::MD as i8));
    ui.add_space(space::MD);
    frame.show(ui, |ui| {
        ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for line in log {
                    let (rect, _response) = ui
                        .allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    ui.painter().text(
                        pos2(rect.left(), rect.center().y),
                        Align2::LEFT_CENTER,
                        line,
                        Face::Number.at(size::MICRO),
                        tint_for(line),
                    );
                }
            });
    });
}

/// The installer marks its own lines, so the log is read rather than scanned.
fn tint_for(line: &str) -> egui::Color32 {
    let trimmed = line.trim_start();
    if trimmed.starts_with("x ") {
        colour::ENEMY
    } else if trimmed.starts_with("+ ") {
        colour::ALLY
    } else if trimmed.starts_with("! ") {
        colour::WARN
    } else if trimmed.starts_with("==>") {
        colour::TEXT_STRONG
    } else {
        colour::TEXT_DIM
    }
}

/// The step's title and the sentence under it.
fn title(ui: &mut Ui, text: &str, about: &str) {
    ui.add_space(space::LG);
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
    note(ui, about, colour::TEXT_FAINT);
    ui.add_space(space::MD);
}

/// A heading inside a step.
fn heading(ui: &mut Ui, text: &str) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().text(
        pos2(rect.left() + space::XL, rect.center().y),
        Align2::LEFT_CENTER,
        label_text(text),
        Face::Display.at(size::LABEL),
        colour::TEXT_DIM,
    );
}

/// A line of explanation.
fn note(ui: &mut Ui, text: &str, tint: egui::Color32) {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().text(
        pos2(rect.left() + space::XL, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        Face::Body.at(size::MICRO),
        tint,
    );
}

/// One choice. True when it was clicked.
fn option_row(ui: &mut Ui, name: &str, about: &str, chosen: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::ROW + space::SM),
        Sense::click(),
    );
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    let painter = ui.painter().clone();
    if chosen {
        painter.rect_filled(rect, 0, colour::BG_SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0, colour::BG_HOVER);
    }
    let dot = Rect::from_min_size(
        pos2(rect.left() + space::XL, rect.center().y - 5.0),
        vec2(10.0, 10.0),
    );
    if chosen {
        painter.rect_filled(dot, 0, colour::INFO);
    } else {
        painter.rect_stroke(
            dot,
            0,
            egui::Stroke::new(1.0, colour::LINE),
            egui::StrokeKind::Inside,
        );
    }
    painter.text(
        pos2(dot.right() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        Face::Body.at(size::BODY),
        if chosen {
            colour::TEXT_STRONG
        } else {
            colour::TEXT
        },
    );
    painter.text(
        pos2(rect.left() + 190.0, rect.center().y),
        Align2::LEFT_CENTER,
        about,
        Face::Body.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    response.clicked()
}

/// The footer: what happens next, and the button that does it.
fn footer(ui: &mut Ui, wizard: &Wizard) -> Option<Action> {
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::XXL + space::MD),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return None;
    }
    ui.painter()
        .hline(rect.x_range(), rect.top(), (1.0, colour::LINE));

    let (label, action, enabled) = match wizard.step {
        Step::Look => ("continue", Action::Choose, !wizard.survey.blocked()),
        Step::Choose => ("install", Action::Install, true),
        Step::Install => match wizard.outcome {
            None => ("working", Action::Close, false),
            Some(_) => ("close", Action::Close, true),
        },
    };

    let mut clicked = None;
    if wizard.step == Step::Choose {
        let back = button(ui, rect, "back", 1, true);
        if back {
            clicked = Some(Action::Back);
        }
    }
    if button(ui, rect, label, 0, enabled) && enabled {
        clicked = Some(action);
    }
    clicked
}

/// A button in the footer, counted from the right.
fn button(ui: &Ui, footer: Rect, label: &str, from_right: usize, enabled: bool) -> bool {
    let width = 108.0;
    let height = space::XXL;
    let step = width + space::MD;
    let x = step.mul_add(-(from_right as f32), footer.right() - space::XL) - width;
    let rect = Rect::from_min_size(
        pos2(x, footer.center().y - height / 2.0),
        vec2(width, height),
    );
    let response = ui.interact(rect, ui.id().with(label), Sense::click());
    let painter = ui.painter();
    let (fill, text) = if !enabled {
        (colour::LINE, colour::TEXT_FAINT)
    } else if from_right == 0 {
        // The one that moves you forward is the bright one, and there is
        // only ever one bright thing on the screen.
        (
            if response.hovered() {
                colour::TEXT_STRONG
            } else {
                colour::INFO
            },
            colour::BG,
        )
    } else {
        (colour::BG_RAISED, colour::TEXT)
    };
    painter.rect_filled(rect, 0, fill);
    if from_right > 0 {
        painter.rect_stroke(
            rect,
            0,
            egui::Stroke::new(1.0, colour::LINE),
            egui::StrokeKind::Inside,
        );
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label_text(label),
        Face::Display.at(size::LABEL),
        text,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::{Step, step_index, tint_for};
    use overseer_ui::colour;

    /// The marks fill left to right, so the order has to be the order.
    #[test]
    fn the_steps_are_in_order() {
        assert!(step_index(Step::Look) < step_index(Step::Choose));
        assert!(step_index(Step::Choose) < step_index(Step::Install));
    }

    /// The installer marks its own lines and the log reads them, so a
    /// failure is red without anybody parsing English.
    #[test]
    fn the_log_reads_the_installers_own_marks() {
        assert_eq!(tint_for("  x Setup failed: no disk"), colour::ENEMY);
        assert_eq!(tint_for("  + Python 3.12.10 x64 ready."), colour::ALLY);
        assert_eq!(tint_for("  ! repair needed"), colour::WARN);
        assert_eq!(tint_for("==> Checking this PC ..."), colour::TEXT_STRONG);
        assert_eq!(tint_for("something else entirely"), colour::TEXT_DIM);
    }
}
