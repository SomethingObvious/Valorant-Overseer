//! What the window says while there is no board to draw.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use overseer_core::Status;

use crate::board;
use overseer_ui::{self, Face, caps_text, colour, motion, size, space};

/// What to say while there is no board. Nothing on it moves, because this is
/// the screen the window shows most, and a pulse would cost frames the whole
/// time somebody sits in the game's menus.
pub(super) fn empty(ui: &mut Ui, status: &Status, trouble: Option<&str>, fade: f32) {
    let (title, detail, reached): (&str, &str, f32) = match (trouble, status) {
        (Some(why), _) => ("This Build Can't Read the Bridge", why, 0.0),
        (None, Status::Live) => (
            "Signed In, Nothing in Progress",
            "Open VALORANT and this fills in by itself.",
            2.0,
        ),
        (None, Status::Connecting(_)) => (
            "Looking for the Backend",
            "The window connects once the backend is up and listening.",
            0.0,
        ),
        (None, Status::Lost(why)) => ("Not Connected", why, 0.0),
    };
    let broken = trouble.is_some() || matches!(status, Status::Lost(_));
    let room = ui.available_rect_before_wrap();
    if room.height() < 210.0 {
        plain(ui, title, detail);
        return;
    }
    let (rect, _response) = ui.allocate_exact_size(room.size(), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let width = 420.0_f32.min(space::XXL.mul_add(-2.0, rect.width()));
    // Laid out first, because the plate is as tall as its sentence, and the
    // longest sentence is whatever the bridge last refused to do.
    let mut job = egui::text::LayoutJob::simple(
        detail.to_owned(),
        Face::Body.at(size::BODY),
        colour::TEXT_DIM,
        space::XL.mul_add(-2.0, width),
    );
    job.wrap.max_rows = 3;
    job.wrap.overflow_character = Some('\u{2026}');
    let sentence = painter.layout_job(job);
    let plate = Rect::from_center_size(
        pos2(
            rect.center().x,
            rect.top() + (rect.height() * 0.44).max(120.0),
        ),
        vec2(width, 54.0 + sentence.size().y + space::XL + 48.0),
    );
    board::paint::slab(&painter, plate, colour::BG_RAISED, 0.0, false);
    // The words fade in again whenever they change, and the steps light up
    // in turn, so the backend and Riot coming up read as progress.
    let mut ink = painter.clone();
    ink.multiply_opacity(restated(ui, title, fade));
    words(&ink, plate, title, sentence);
    let lit = ui
        .ctx()
        .animate_value_with_time(egui::Id::new("empty-steps"), reached, fade * 2.0);
    chain(&painter, plate, lit, broken);
}

/// How far the plate's words have faded in since `title` last changed.
fn restated(ui: &Ui, title: &str, fade: f32) -> f32 {
    let now = ui.input(|i| i.time);
    let key = egui::Id::new(title);
    let since = ui.ctx().data_mut(|d| {
        let kept = d.get_temp_mut_or_insert_with(egui::Id::new("empty-since"), || (key, now));
        if kept.0 != key {
            *kept = (key, now);
        }
        now - kept.1
    });
    if fade <= 0.0 {
        return 1.0;
    }
    let shown = motion::eased(since as f32 / fade);
    if shown < 1.0 {
        ui.ctx().request_repaint();
    }
    shown
}

/// The words on the plate: the headline, and the sentence already laid
/// out to decide how tall the plate had to be.
fn words(
    painter: &egui::Painter,
    plate: Rect,
    title: &str,
    sentence: std::sync::Arc<egui::Galley>,
) {
    let _title = caps_text(
        painter,
        pos2(plate.left() + space::XL, plate.top() + 28.0),
        Align2::LEFT_CENTER,
        title,
        Face::Heavy.at(24.0),
        colour::TEXT_STRONG,
    );
    painter.galley(
        pos2(plate.left() + space::XL, plate.top() + 52.0),
        sentence,
        colour::TEXT_DIM,
    );
    painter.hline(
        plate.left() + space::XL..=plate.right() - space::XL,
        plate.bottom() - 48.0,
        (1.0, colour::LINE),
    );
}

/// The backend, Riot and the match, lit up to where things have got. Each can
/// fail alone and every failure looks like an empty window, so naming them
/// tells waiting apart from stuck.
fn chain(painter: &egui::Painter, plate: Rect, reached: f32, broken: bool) {
    let middle = plate.bottom() - 22.0;
    let mut x = plate.left() + space::XL;
    let mut first = 0.0_f32;
    for (step, name) in ["Backend", "Riot", "Match"].into_iter().enumerate() {
        // How lit this step is, from 0 to 1, as `reached` eases towards it.
        let on = (reached - first).clamp(0.0, 1.0);
        first += 1.0;
        let tint = if broken && step == 0 {
            colour::ENEMY
        } else {
            colour::TEXT_FAINT.lerp_to_gamma(colour::ALLY, on)
        };
        let pip = Rect::from_center_size(pos2(x + 5.0, middle), vec2(10.0, 14.0));
        let solid = if broken && step == 0 { 1.0 } else { on };
        painter.add(board::paint::slant(
            pip,
            true,
            true,
            tint.gamma_multiply(0.65_f32.mul_add(solid, 0.35)),
        ));
        let after = caps_text(
            painter,
            pos2(x + space::XL, middle),
            Align2::LEFT_CENTER,
            name,
            Face::Display.at(size::MICRO),
            colour::TEXT_FAINT.lerp_to_gamma(colour::TEXT, on),
        );
        x = after.right() + space::LG;
        if step < 2 {
            painter.hline(x..=x + space::LG, middle, (1.0, colour::LINE));
            x += space::LG + space::LG;
        }
    }
}

/// The same words with no furniture, for a window too short to hold any.
fn plain(ui: &mut Ui, title: &str, detail: &str) {
    ui.add_space(space::XXL);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XXL * 2.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let _title = caps_text(
        painter,
        pos2(rect.center().x, rect.top() + space::LG),
        Align2::CENTER_CENTER,
        title,
        Face::Heavy.at(22.0),
        colour::TEXT_STRONG,
    );
    painter.text(
        pos2(rect.center().x, rect.bottom() - space::LG),
        Align2::CENTER_CENTER,
        detail,
        Face::Body.at(size::BODY),
        colour::TEXT_DIM,
    );
}
