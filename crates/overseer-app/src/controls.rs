//! Buttons, chips and text boxes in the board's slant, for the screens that
//! take input. egui's own look like a web form next to everything else.

use egui::{Align2, Color32, Rect, Response, RichText, Sense, Ui, pos2, vec2};
use overseer_ui::{Face, caps_text, caps_width, colour, size, space};

use crate::board::paint;

/// How loud a button is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// One of several things to do.
    Plain,
    /// The thing this screen is for: outlined in cream, since a solid cream
    /// plate is a choice that is already on.
    Primary,
    /// A button that throws something away.
    Danger,
}

/// How tall a button or a chip is.
const HEIGHT: f32 = 24.0;

/// How wide [`button`] draws `text`, for laying something out beside it.
pub(crate) fn width(ui: &Ui, text: &str) -> f32 {
    space::XL.mul_add(2.0, caps_width(ui.painter(), text, paint::label()))
}

/// A button in the current layout. A disabled one is dim and ignores clicks.
pub(crate) fn button(ui: &mut Ui, text: &str, tone: Tone, enabled: bool) -> Response {
    let wide = width(ui, text);
    button_wide(ui, text, (tone, enabled), wide)
}

/// [`button`] `wide` points across, so a row of them lines up as a grid.
pub(crate) fn button_wide(
    ui: &mut Ui,
    text: &str,
    (tone, enabled): (Tone, bool),
    wide: f32,
) -> Response {
    let painter = ui.painter().clone();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(wide, HEIGHT), sense);
    let hot = enabled && response.hovered();
    let (fill, ink) = match (tone, enabled) {
        (_, false) => (colour::BG_INSET, colour::TEXT_FAINT.gamma_multiply(0.6)),
        (Tone::Primary, true) if hot => (
            colour::TEXT_STRONG.gamma_multiply(0.16),
            colour::TEXT_STRONG,
        ),
        (Tone::Primary, true) => (colour::BG_INSET, colour::TEXT_STRONG),
        (Tone::Danger, true) if hot => (colour::ENEMY, colour::TEXT_STRONG),
        (Tone::Danger, true) => (colour::BG_INSET, colour::BAD),
        (Tone::Plain, true) if hot => (colour::BG_HOVER, colour::TEXT_STRONG),
        (Tone::Plain, true) => (colour::BG_INSET, colour::TEXT),
    };
    if ui.is_rect_visible(rect) {
        painter.add(paint::slant(rect, true, true, fill));
        if enabled && tone == Tone::Primary {
            let mut edge = paint::slanted(rect.shrink(0.75), true, true);
            if let Some(first) = edge.first().copied() {
                edge.push(first);
            }
            painter.add(egui::Shape::line(
                edge,
                egui::Stroke::new(1.5, colour::TEXT_STRONG),
            ));
        }
        let _drawn = caps_text(
            &painter,
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            paint::label(),
            ink,
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// The card a menu opens in: raised, with a hairline round it.
pub(crate) fn menu_card() -> egui::Frame {
    egui::Frame::NONE
        .fill(colour::BG_RAISED)
        .stroke(egui::Stroke::new(1.0, colour::LINE))
        .corner_radius(6)
        .inner_margin(4)
}

/// A row in a menu `wide` points across, lit under the pointer, with a red
/// mark on the left when it is `on`.
pub(crate) fn menu_row(ui: &mut Ui, text: &str, on: bool, wide: f32) -> Response {
    let (rect, row) = ui.allocate_exact_size(vec2(wide, 26.0), Sense::click());
    let row = row.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    if row.hovered() {
        painter.rect_filled(rect, 4.0, colour::BG_HOVER);
    } else if on {
        painter.rect_filled(rect, 4.0, colour::BG_SELECTED);
    }
    if on {
        let mark =
            Rect::from_center_size(pos2(rect.left() + 4.0, rect.center().y), vec2(3.0, 14.0));
        painter.rect_filled(mark, 1.5, colour::ENEMY);
    }
    let ink = if on || row.hovered() {
        colour::TEXT_STRONG
    } else {
        colour::TEXT
    };
    painter.text(
        rect.left_center() + vec2(12.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        Face::Body.at(size::BODY),
        ink,
    );
    row
}

/// How wide [`chip`] draws `text`, for laying something out beside it.
pub(crate) fn chip_width(ui: &Ui, text: &str) -> f32 {
    space::LG.mul_add(2.0, caps_width(ui.painter(), text, paint::label()))
}

/// One choice of several, lit when it is the one chosen.
pub(crate) fn chip(ui: &mut Ui, text: &str, on: bool) -> Response {
    let painter = ui.painter().clone();
    let wide = space::LG.mul_add(2.0, caps_width(&painter, text, paint::label()));
    let (rect, response) = ui.allocate_exact_size(vec2(wide, HEIGHT - 4.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if on {
            colour::TEXT_STRONG
        } else if response.hovered() {
            colour::BG_HOVER
        } else {
            colour::BG_INSET
        };
        painter.add(paint::slant(rect, true, true, fill));
        let _drawn = caps_text(
            &painter,
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            paint::label(),
            if on { colour::BG } else { colour::TEXT_DIM },
        );
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, text));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A chip with a small white icon before its words, tinted like them, for
/// an ability.
pub(crate) fn chip_with(
    ui: &mut Ui,
    icon: Option<&egui::TextureHandle>,
    text: &str,
    on: bool,
) -> Response {
    let painter = ui.painter().clone();
    let room = if icon.is_some() {
        14.0 + space::SM
    } else {
        0.0
    };
    let wide = space::LG.mul_add(2.0, caps_width(&painter, text, paint::label())) + room;
    let (rect, response) = ui.allocate_exact_size(vec2(wide, HEIGHT - 2.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if on {
            colour::TEXT_STRONG
        } else if response.hovered() {
            colour::BG_HOVER
        } else {
            colour::BG_INSET
        };
        let ink = if on { colour::BG } else { colour::TEXT };
        painter.add(paint::slant(rect, true, true, fill));
        if let Some(icon) = icon {
            painter.image(
                icon.id(),
                Rect::from_center_size(
                    pos2(rect.left() + space::LG + 7.0, rect.center().y),
                    vec2(14.0, 14.0),
                ),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                ink,
            );
        }
        let _drawn = caps_text(
            &painter,
            pos2(rect.left() + space::LG + room, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            paint::label(),
            ink,
        );
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, text));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small caps heading over the controls that follow it.
pub(crate) fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(space::MD);
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), space::XL), Sense::hover());
    if ui.is_rect_visible(rect) {
        let _drawn = caps_text(
            ui.painter(),
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            text,
            paint::label(),
            colour::TEXT_FAINT,
        );
    }
}

/// A one-line text box `width` wide, set into the board's slant.
pub(crate) fn field(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    hinted(ui, text, hint_text(hint, colour::HINT), width)
}

/// A one-line box with its hint already set.
fn hinted(ui: &mut Ui, text: &mut String, hint: RichText, width: f32) -> Response {
    let (rect, _response) = ui.allocate_exact_size(vec2(width, HEIGHT + 2.0), Sense::hover());
    ui.painter()
        .add(paint::slant(rect, false, true, colour::BG_INSET));
    let response = ui.put(
        rect.shrink2(vec2(space::MD, 2.0)),
        egui::TextEdit::singleline(text)
            .frame(egui::Frame::NONE)
            .vertical_align(egui::Align::Center)
            .font(Face::Body.at(size::BODY))
            .hint_text(hint),
    );
    underline(ui, rect, &response);
    response
}

/// A text box a few lines tall, for notes. With `caps`, whatever is typed or
/// pasted goes in as capitals.
pub(crate) fn notes(ui: &mut Ui, text: &mut String, hint: &str, caps: bool) -> Response {
    let (rect, _response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT * 3.0), Sense::hover());
    ui.painter()
        .add(paint::slant(rect, false, true, colour::BG_INSET));
    let id = ui.make_persistent_id(("notes", hint));
    // Changed before the box reads them, so a lowercase letter never shows,
    // not even for the frame before the text is fixed up.
    if caps && ui.memory(|m| m.has_focus(id)) {
        ui.input_mut(|i| {
            for event in &mut i.events {
                if let egui::Event::Text(typed) | egui::Event::Paste(typed) = event {
                    *typed = typed.to_uppercase();
                }
            }
        });
    }
    let response = ui.put(
        rect.shrink2(vec2(space::MD, space::SM)),
        egui::TextEdit::multiline(text)
            .id(id)
            .frame(egui::Frame::NONE)
            .font(Face::Body.at(size::BODY))
            .hint_text(hint_text(hint, colour::HINT)),
    );
    // Anything that got past the events, like an input method's composed text.
    if caps && response.changed() && text.chars().any(char::is_lowercase) {
        *text = text.to_uppercase();
        ui.ctx().request_repaint();
    }
    underline(ui, rect, &response);
    response
}

/// A cream line along the bottom of a text box while it has the keyboard,
/// the way the search box shows it.
fn underline(ui: &Ui, rect: Rect, response: &Response) {
    if response.has_focus() {
        ui.painter().rect_filled(
            Rect::from_min_size(
                pos2(rect.left(), rect.bottom() - 2.0),
                vec2(rect.width() - 6.0, 2.0),
            ),
            0,
            colour::TEXT_STRONG,
        );
    }
}

/// A hint in the text box's own face, in `tint`.
fn hint_text(hint: &str, tint: Color32) -> RichText {
    RichText::new(hint)
        .color(tint)
        .font(Face::Body.at(size::BODY))
}

#[cfg(test)]
mod tests {
    use egui::Ui;
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;

    /// A caps box takes typed and pasted text in capitals, and a plain one
    /// leaves it as typed.
    #[test]
    fn a_caps_box_writes_in_capitals_as_you_type() {
        for (caps, want) in [(true, "JUMP, LEFT CLICK"), (false, "jump, left click")] {
            let mut harness = Harness::builder()
                .with_size(egui::vec2(400.0, 200.0))
                .build_ui_state(
                    move |ui: &mut Ui, state: &mut (bool, String)| {
                        if state.0 {
                            let _box = super::notes(ui, &mut state.1, "hint", caps);
                        }
                    },
                    (false, String::new()),
                );
            // The faces are bound on the frame after they are installed.
            overseer_ui::install_fonts(&harness.ctx);
            harness.run();
            harness.state_mut().0 = true;
            harness.run();
            harness.get_by_role(Role::MultilineTextInput).focus();
            harness.run();
            harness
                .get_by_role(Role::MultilineTextInput)
                .type_text("jump, left click");
            harness.run();
            assert_eq!(harness.state().1, want, "caps {caps}");
        }
    }
}
