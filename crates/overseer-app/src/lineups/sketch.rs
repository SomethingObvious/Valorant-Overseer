//! The panel for drawing on the map: which shape, which colour, how solid,
//! how wide a cone opens, and taking back or clearing what is drawn. The
//! colour and the opacity also change the shape drawn or moved last. Every
//! change is kept straight away, so closing the window loses nothing.

use egui::{Sense, Stroke, Ui, vec2};
use overseer_ui::colour;

use super::shapes::{COLOURS, KINDS, ink};
use super::side::{title, words};
use super::{Mode, Sketch, View};
use crate::controls::{self, Tone};

/// Draws the panel while shapes are being drawn.
pub(super) fn show(ui: &mut Ui, view: &mut View) {
    let map = view.map.clone().unwrap_or_default();
    let Mode::Draw(sketch) = &mut view.mode else {
        return;
    };
    title(ui, "Draw on the Map", &map);
    words(
        ui,
        "Pick a shape and a colour, then drag on the map to draw it, like a circle for a molly's range. Brush draws freehand, Text puts a few words wherever you click, Text Box is dragged out like a rectangle and holds as much as you type, and Eraser takes off any drawing you click or drag across. Drag a shape's dot to move it, drag the small squares on the picked shape to resize it, and Delete Shape or the Delete key takes the picked one off, as does a right-click on any. The colour, the opacity and the brush size change the shape with the lit dot too.",
        colour::TEXT_DIM,
    );
    controls::heading(ui, "Shape");
    ui.horizontal_wrapped(|ui| {
        for (index, (_, name)) in KINDS.iter().enumerate() {
            if controls::chip(ui, name, sketch.kind == index).clicked() {
                sketch.kind = index;
            }
        }
    });
    if KINDS
        .get(sketch.kind)
        .is_some_and(|(kind, _)| *kind == "cone")
    {
        controls::heading(ui, "Cone Width");
        ui.add(
            egui::Slider::new(&mut sketch.spread, 10.0..=180.0)
                .suffix("\u{b0}")
                .step_by(5.0),
        );
    }
    let sized = brush_size(ui, sketch);
    let retyped = lettering(ui, sketch);
    // The eraser has no colour or opacity of its own.
    let restyled = sketch.tool() != "erase" && look(ui, sketch);
    let mut changed = restyled || retyped || sized;
    ui.add_space(overseer_ui::space::MD);
    let count = sketch.shapes.len();
    words(
        ui,
        &match count {
            0 => format!("Nothing drawn on {map} yet."),
            1 => format!("1 shape on {map}."),
            n => format!("{n} shapes on {map}."),
        },
        colour::TEXT_FAINT,
    );
    let (more, done) = buttons(ui, sketch, count);
    changed |= more;
    if changed {
        view.unsaved = Some((map, sketch.shapes.clone()));
    }
    if done {
        view.mode = Mode::Browse;
    }
}

/// Undo, Delete Shape, Clear All and Done, and the Delete key, under the
/// tools. Says whether the shapes changed and whether Done was pressed.
fn buttons(ui: &mut Ui, sketch: &mut Sketch, count: usize) -> (bool, bool) {
    let (mut changed, mut done) = (false, false);
    // The Delete key too, unless a label's words have the keyboard.
    let typing = ui.memory(|m| m.focused().is_some());
    let pressed = !typing
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    ui.horizontal_wrapped(|ui| {
        if controls::button(ui, "Undo", Tone::Plain, count > 0).clicked() {
            sketch.shapes.pop();
            sketch.picked = None;
            sketch.confirm = false;
            changed = true;
        }
        let picked = sketch.picked.filter(|&i| i < count);
        let delete = controls::button(ui, "Delete Shape", Tone::Danger, picked.is_some()).clicked();
        if let Some(index) = picked
            && (delete || pressed)
        {
            sketch.shapes.remove(index);
            sketch.picked = None;
            sketch.confirm = false;
            changed = true;
        }
        let clear = if sketch.confirm {
            "Click Again to Clear"
        } else {
            "Clear All"
        };
        if controls::button(ui, clear, Tone::Danger, count > 0).clicked() {
            if sketch.confirm {
                sketch.shapes.clear();
                sketch.picked = None;
                changed = true;
            }
            sketch.confirm = !sketch.confirm;
        }
        done = controls::button(ui, "Done", Tone::Primary, true).clicked();
    });
    (changed, done)
}

/// The kind of the picked shape, if one is picked.
fn picked_kind(sketch: &Sketch) -> Option<&str> {
    sketch
        .picked
        .and_then(|i| sketch.shapes.get(i))
        .map(|s| s.kind.as_str())
}

/// How thick the brush draws, while the brush is out or a stroke is picked.
/// Says whether the picked stroke changed.
fn brush_size(ui: &mut Ui, sketch: &mut Sketch) -> bool {
    let stroke = picked_kind(sketch) == Some("brush");
    if sketch.tool() != "brush" && !stroke {
        return false;
    }
    controls::heading(ui, "Brush Size");
    // Shown from 1 to 20, kept as a share of the map so a stroke keeps its
    // look at any window size.
    let mut size = sketch.width * 400.0;
    let slid = ui.add(
        egui::Slider::new(&mut size, 1.0..=20.0)
            .step_by(1.0)
            .fixed_decimals(0),
    );
    if !slid.changed() {
        return false;
    }
    sketch.width = size / 400.0;
    if stroke && let Some(shape) = sketch.picked.and_then(|i| sketch.shapes.get_mut(i)) {
        shape.width = sketch.width;
        return true;
    }
    false
}

/// The words for a label or a text box, while that tool is out or one of
/// them is picked. Says whether the picked one changed.
fn lettering(ui: &mut Ui, sketch: &mut Sketch) -> bool {
    // Whether a label or a box is picked, and which.
    let picked = match picked_kind(sketch) {
        Some("text") => Some(false),
        Some("textbox") => Some(true),
        _ => None,
    };
    let boxed = picked.unwrap_or_else(|| sketch.tool() == "textbox");
    if picked.is_none() && !matches!(sketch.tool(), "text" | "textbox") {
        return false;
    }
    controls::heading(ui, "Words");
    // A label is a line of a few words. A box holds a paragraph or two.
    let (typed, longest) = if boxed {
        let hint = "Type what the box says";
        (controls::notes(ui, &mut sketch.words, hint, false), 400)
    } else {
        let wide = ui.available_width();
        (
            controls::field(ui, &mut sketch.words, "Type the label", wide),
            40,
        )
    };
    if let Some((cut, _)) = sketch.words.char_indices().nth(longest) {
        sketch.words.truncate(cut);
    }
    let words = if boxed {
        sketch.words.trim_end()
    } else {
        sketch.words.trim()
    };
    // A label with no words is nothing, so it keeps the ones it had. An
    // empty box is still a box.
    if typed.changed()
        && picked.is_some()
        && (boxed || !words.is_empty())
        && let Some(shape) = sketch.picked.and_then(|i| sketch.shapes.get_mut(i))
    {
        words.clone_into(&mut shape.text);
        return true;
    }
    false
}

/// The colour and the opacity, for the next shape and the picked one. Says
/// whether the picked shape changed.
fn look(ui: &mut Ui, sketch: &mut Sketch) -> bool {
    let mut changed = false;
    controls::heading(ui, "Colour");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (index, hex) in COLOURS.iter().enumerate() {
            if swatch(ui, hex, sketch.colour == index).clicked() {
                sketch.colour = index;
                if let Some(shape) = sketch.picked.and_then(|i| sketch.shapes.get_mut(i)) {
                    (*hex).clone_into(&mut shape.colour);
                    changed = true;
                }
            }
        }
    });
    controls::heading(ui, "Opacity");
    let mut percent = sketch.opacity * 100.0;
    let slid = ui.add(
        egui::Slider::new(&mut percent, 10.0..=100.0)
            .suffix("%")
            .step_by(5.0)
            .fixed_decimals(0),
    );
    if slid.changed() {
        sketch.opacity = percent / 100.0;
        if let Some(shape) = sketch.picked.and_then(|i| sketch.shapes.get_mut(i)) {
            shape.opacity = sketch.opacity;
            changed = true;
        }
    }
    changed
}

/// One colour to pick, ringed in cream when it is the one picked.
pub(crate) fn swatch(ui: &mut Ui, hex: &str, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect.shrink(3.0), 0, ink(hex));
    if on {
        painter.rect_stroke(
            rect,
            0,
            Stroke::new(2.0, colour::TEXT_STRONG),
            egui::StrokeKind::Inside,
        );
    } else if response.hovered() {
        painter.rect_stroke(
            rect,
            0,
            Stroke::new(1.0, colour::TEXT_DIM),
            egui::StrokeKind::Inside,
        );
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, hex));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
