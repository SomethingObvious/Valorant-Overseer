//! The wizard's window and its three steps. The last one shows the installer's
//! own log rather than a spinner, because when an install fails the line where
//! it failed is what somebody needs.

use std::path::PathBuf;

use eframe::{App, CreationContext, Frame};
use egui::{Align2, CentralPanel, Panel, Rect, ScrollArea, Sense, Ui, pos2, vec2};
use overseer_ui::chrome::{self, Button};
use overseer_ui::{Face, art, caps_at, caps_text, colour, shape, size, space};

use crate::plan::{self, Finding, REGIONS, Survey};
use crate::run::{self, Line, Running};

/// Which step is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// What this machine has.
    Look,
    /// Where you play.
    Choose,
    /// Doing it, with the log.
    Install,
}

/// The wizard's state.
pub(crate) struct Wizard {
    root: PathBuf,
    /// This setup's own exe, when the app is inside it.
    carried: Option<PathBuf>,
    survey: Survey,
    step: Step,
    region: usize,
    running: Option<Running>,
    log: Vec<String>,
    outcome: Option<bool>,
}

impl std::fmt::Debug for Wizard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wizard")
            .field("step", &self.step)
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}

impl Wizard {
    /// Looks at the machine and opens on what it found.
    pub(crate) fn new(cc: &CreationContext<'_>, root: PathBuf, carried: Option<PathBuf>) -> Self {
        overseer_ui::install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx
            .set_style_of(egui::Theme::Dark, overseer_ui::style());
        let survey = plan::survey(&root, carried.is_some());
        Self {
            root,
            carried,
            survey,
            step: Step::Look,
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
        self.running = Some(run::start(
            &self.root,
            region,
            self.carried.as_deref(),
            move || waker.request_repaint(),
        ));
        self.step = Step::Install;
    }
}

impl App for Wizard {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        self.pump();
        let chrome = egui::Frame::NONE.fill(colour::BG);
        let pressed = Panel::top("head")
            .exact_size(HEAD)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| header(ui, self.step))
            .inner;
        match pressed {
            Some(Button::Minimize) => {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            Some(Button::Close) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
        chrome::outline(ui.ctx());
        Panel::bottom("foot")
            .exact_size(space::XXL + space::MD)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| {
                if let Some(next) = footer(ui, self.step, self.outcome, self.survey.blocked()) {
                    match next {
                        Action::Choose => self.step = Step::Choose,
                        Action::Back => self.step = Step::Look,
                        Action::Install => self.begin(ui.ctx()),
                        Action::Open => {
                            run::open(&self.root);
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
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
                Step::Choose => choose(ui, &mut self.region),
                Step::Install => install(ui, &self.log, self.outcome),
            });
    }
}

/// What the footer's button does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// On to the choice.
    Choose,
    /// Back to the findings.
    Back,
    /// Start installing.
    Install,
    /// Installed, so open the app and close this.
    Open,
    /// Finished, close the window.
    Close,
}

/// How tall the title bar is.
const HEAD: f32 = space::XXL + space::XL;

/// The wizard's title bar: the mark and the name, where you are in it, then
/// minimize and close. The window has no frame from Windows, so this bar is
/// what drags it. Says which button was clicked.
pub(crate) fn header(ui: &mut Ui, step: Step) -> Option<Button> {
    let (rect, bar) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEAD), Sense::click_and_drag());
    // The window has one size, so a double click has nothing to fill.
    let _fills = chrome::drag(ui, &bar);
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let (buttons_at, clicked) = chrome::buttons(
        ui,
        rect,
        &[Button::Close, Button::Minimize],
        chrome::State::default(),
    );
    let painter = ui.painter().clone();
    let middle = rect.center().y;
    let font = Face::Display.at(size::TITLE);
    let mut start = rect.left() + space::XL;
    if let Some(mark) = art::mark(ui.ctx()) {
        // Centred on the capitals beside it and a little taller than them,
        // the way the window's own title sets it.
        let galley = painter.layout_job(overseer_ui::caps("Valorant", font.clone(), colour::ENEMY));
        let caps = galley.mesh_bounds;
        let centre = middle - galley.size().y / 2.0 + caps.center().y;
        let tall = caps.height() * art::MARK_OVER_CAPS;
        let at = Rect::from_min_size(
            pos2(start, centre - tall / 2.0),
            vec2(tall * mark.aspect_ratio(), tall),
        );
        let mut mesh = egui::Mesh::with_texture(mark.id());
        mesh.add_rect_with_uv(
            at,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        painter.add(egui::Shape::mesh(mesh));
        start = at.right() + space::MD;
    }
    let after = caps_text(
        &painter,
        pos2(start, middle),
        Align2::LEFT_CENTER,
        "Valorant",
        font,
        colour::ENEMY,
    );
    let after = caps_text(
        &painter,
        pos2(after.right() + space::MD, middle),
        Align2::LEFT_CENTER,
        "Overseer",
        Face::Display.at(size::TITLE),
        colour::TEXT_STRONG,
    );
    caps_at(
        &painter,
        pos2(after.right() + space::LG, middle + 1.0),
        Align2::LEFT_CENTER,
        "Setup",
        Face::Display.at(size::LABEL),
        colour::TEXT_DIM,
    );

    // One mark per step, filled up to where you are, because a progress bar
    // would claim to know how long this takes.
    let mut x = buttons_at - space::XL;
    for at in [Step::Install, Step::Choose, Step::Look] {
        let done = step_index(step) >= step_index(at);
        let mark = Rect::from_min_size(pos2(x - 18.0, middle - 2.0), vec2(14.0, 3.0));
        painter.rect_filled(mark, 0, if done { colour::INFO } else { colour::LINE });
        x -= 22.0;
    }
    painter.hline(rect.x_range(), rect.bottom() - 1.0, (1.0, colour::LINE));
    clicked
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
        "What This PC Has",
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
                overseer_ui::say(
                    ui,
                    space::XL,
                    colour::ENEMY,
                    "The install can't go ahead until the red line above is fixed. Then run setup again.",
                );
            }
        });
}

/// One finding: a mark, what was looked at, and what was found.
fn row(ui: &mut Ui, finding: &Finding) {
    let width = ui.available_width();
    // Wrapped inside its own column, because a folder path or a Windows error
    // can run past the edge of the window.
    let detail = ui.painter().layout(
        finding.detail.clone(),
        Face::Body.at(size::MICRO),
        colour::TEXT_DIM,
        width - 190.0 - space::XL,
    );
    let line = detail.rows.first().map_or(0.0, |r| r.height());
    let (rect, _response) = ui.allocate_exact_size(
        vec2(width, space::ROW + detail.size().y - line),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let tint = if finding.blocking {
        colour::ENEMY
    } else {
        colour::ALLY
    };
    // Level with the first line, which is where a one line finding sits.
    let middle = rect.top() + space::ROW / 2.0;
    let mark = Rect::from_min_size(pos2(rect.left() + space::XL, middle - 4.0), vec2(8.0, 8.0));
    painter.rect_filled(mark, 0, tint);
    painter.text(
        pos2(mark.right() + space::LG, middle),
        Align2::LEFT_CENTER,
        &finding.what,
        Face::Body.at(size::BODY),
        colour::TEXT_STRONG,
    );
    painter.galley(
        pos2(rect.left() + 190.0, middle - line / 2.0),
        detail,
        colour::TEXT_DIM,
    );
}

/// Step two: which region.
pub(crate) fn choose(ui: &mut Ui, region: &mut usize) {
    title(
        ui,
        "Where You Play",
        "So the backend talks to the right Riot servers. Run setup again to change it.",
    );
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, (key, name)) in REGIONS.iter().enumerate() {
                if option_row(ui, name, &key.to_uppercase(), *region == index) {
                    *region = index;
                }
            }
        });
}

/// Step three: the installer's own words.
pub(crate) fn install(ui: &mut Ui, log: &[String], outcome: Option<bool>) {
    let (heading_text, detail) = match outcome {
        None => (
            "Installing",
            "This takes a minute the first time, while Python is fetched.",
        ),
        Some(true) => (
            "Done",
            "Everything is installed. Open it now, or from the desktop any time.",
        ),
        Some(false) => (
            "The Install Failed",
            "The last few lines below say why. Fix that, then run setup again.",
        ),
    };
    title(ui, heading_text, detail);
    if outcome == Some(true) {
        note(ui, "The desktop shortcut opens the window.", colour::ALLY);
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

/// A log line's colour, from the mark the installer puts at its start.
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
        caps_at(
            ui.painter(),
            pos2(rect.left() + space::XL, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            Face::Heavy.at(size::DISPLAY),
            colour::TEXT_STRONG,
        );
    }
    note(ui, about, colour::TEXT_FAINT);
    ui.add_space(space::MD);
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
    overseer_ui::choice(ui, name, about, chosen, true, false)
}

/// The footer: what happens next, and the button that does it.
pub(crate) fn footer(
    ui: &mut Ui,
    step: Step,
    outcome: Option<bool>,
    blocked: bool,
) -> Option<Action> {
    let (rect, _response) = ui.allocate_exact_size(
        vec2(ui.available_width(), space::XXL + space::MD),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return None;
    }
    ui.painter()
        .hline(rect.x_range(), rect.top(), (1.0, colour::LINE));

    let (label, action, enabled) = match step {
        Step::Look => ("Continue", Action::Choose, !blocked),
        Step::Choose => ("Install", Action::Install, true),
        Step::Install => match outcome {
            None => ("Working", Action::Close, false),
            Some(true) => ("Open Valorant Overseer", Action::Open, true),
            Some(false) => ("Close", Action::Close, true),
        },
    };

    let back = step == Step::Choose && button(ui, rect, "Back", 1, true);
    if button(ui, rect, label, 0, enabled) && enabled {
        Some(action)
    } else {
        back.then_some(Action::Back)
    }
}

/// A button in the footer, counted from the right.
fn button(ui: &Ui, footer: Rect, label: &str, from_right: usize, enabled: bool) -> bool {
    // The buttons to its left step along by the narrow width, and only the
    // last step's button is ever wider, with nothing beside it.
    let narrow = 108.0;
    let font = Face::Display.at(size::LABEL);
    let width = space::XL
        .mul_add(
            2.0,
            overseer_ui::caps_width(ui.painter(), label, font.clone()),
        )
        .max(narrow);
    let height = space::XXL;
    let step = narrow + space::MD;
    let x = step.mul_add(-(from_right as f32), footer.right() - space::XL) - width;
    let rect = Rect::from_min_size(
        pos2(x, footer.center().y - height / 2.0),
        vec2(width, height),
    );
    let response = ui.interact(rect, ui.id().with(label), Sense::click());
    let painter = ui.painter();
    // Under the pointer a button inverts outright rather than tinting, as
    // Riot's own buttons do. Otherwise only the one that moves you forward is
    // bright.
    let (fill, text) = if !enabled {
        (colour::LINE, colour::TEXT_FAINT)
    } else if response.hovered() {
        (colour::TEXT_STRONG, colour::VOID)
    } else if from_right == 0 {
        (colour::ENEMY, colour::TEXT_STRONG)
    } else {
        (colour::BG_INSET, colour::TEXT)
    };
    painter.add(egui::Shape::gradient_rect(
        rect,
        egui::Direction::TopDown,
        [shape::blend(fill, colour::TEXT_STRONG, 0.10), fill],
    ));
    if from_right > 0 {
        painter.rect_stroke(
            rect,
            0,
            egui::Stroke::new(1.0, colour::LINE),
            egui::StrokeKind::Inside,
        );
    }
    caps_at(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
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
        assert_eq!(
            tint_for("  x Setup failed: This PC doesn't meet the requirements above."),
            colour::ENEMY
        );
        assert_eq!(tint_for("  + Python 3.12.10 x64 ready."), colour::ALLY);
        assert_eq!(
            tint_for("  ! pip install failed. Trying again (2 of 3)."),
            colour::WARN
        );
        assert_eq!(tint_for("==> Checking this PC"), colour::TEXT_STRONG);
        assert_eq!(tint_for("something else entirely"), colour::TEXT_DIM);
    }
}
