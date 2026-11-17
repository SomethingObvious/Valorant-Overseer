use egui::{Sense, Stroke, Ui, vec2};
use overseer_ui::colour;

use overseer_core::Atlas;

use super::shapes::{COLOURS, KINDS, colour_name, ink};
use super::side::{title, words};
use super::{Mode, SPIKE, Sketch, View, pick};
use crate::controls::{self, Tone};

pub(super) fn show(ui: &mut Ui, atlas: &Atlas, view: &mut View) {
    let map = view.map.clone().unwrap_or_default();
    let Mode::Draw(sketch) = &mut view.mode else {
        return;
    };
    if sketch.scratch {
        title(ui, "Sandbox", &map);
        words(
            ui,
            "Nothing here is kept, and the lineups are hidden. Draw, or pick an agent and click the map to put them down.",
            colour::TEXT_DIM,
        );
    } else {
        title(ui, "Draw on the Map", &map);
        words(
            ui,
            "Pick a shape and a colour, then drag on the map. Right-click a drawing or press Delete to take it off.",
            colour::TEXT_DIM,
        );
    }
    controls::heading(ui, "Shape");
    ui.horizontal_wrapped(|ui| {
        for (index, (_, name)) in KINDS.iter().enumerate() {
            let on = sketch.kind == index && !sketch.stamping();
            if controls::chip(ui, name, on).clicked() {
                sketch.kind = index;
                sketch.stamp = None;
            }
        }
    });
    if sketch.scratch {
        stamps(ui, atlas, sketch);
    }
    if sketch.tool() == "cone" {
        controls::heading(ui, "Cone Width");
        let slid = ui.add(
            egui::Slider::new(&mut sketch.spread, 10.0..=180.0)
                .suffix("\u{b0}")
                .step_by(5.0),
        );
        let spread = f64::from(sketch.spread);
        slid.widget_info(|| egui::WidgetInfo::slider(true, spread, "Cone Width"));
    }
    let sized = brush_size(ui, sketch);
    let retyped = lettering(ui, sketch);
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
    if changed && !sketch.scratch {
        view.unsaved.insert(map, sketch.shapes.clone());
    }
    if done {
        view.mode = Mode::Browse;
    }
}

fn stamps(ui: &mut Ui, atlas: &Atlas, sketch: &mut Sketch) {
    controls::heading(ui, "Agents");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        let names = std::iter::once(SPIKE).chain(atlas.agents.iter().map(|k| k.name.as_str()));
        for name in names {
            let on = sketch.stamp.as_deref() == Some(name);
            if pick::tile(ui, name, on, None) {
                sketch.stamp = (!on).then(|| name.to_owned());
                sketch.picked = None;
            }
        }
    });
}

fn buttons(ui: &mut Ui, sketch: &mut Sketch, count: usize) -> (bool, bool) {
    let (mut changed, mut done) = (false, false);
    let typing = ui.memory(|m| m.focused().is_some());
    let pressed = !typing && ui.input(|i| i.key_pressed(egui::Key::Delete));
    let undo = !typing
        && ui.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::Z,
            ))
        });
    if !typing && !ui.input(|i| i.pointer.any_down()) && sketch.shapes != sketch.settled {
        let before = std::mem::replace(&mut sketch.settled, sketch.shapes.clone());
        sketch.history.push(before);
        if sketch.history.len() > super::UNDO_STEPS {
            sketch.history.remove(0);
        }
    }
    ui.horizontal_wrapped(|ui| {
        let back = !sketch.history.is_empty();
        if (controls::button(ui, "Undo", Tone::Plain, back).clicked() || undo)
            && let Some(before) = sketch.history.pop()
        {
            sketch.shapes.clone_from(&before);
            sketch.settled = before;
            sketch.picked = None;
            sketch.confirm = None;
            changed = true;
        }
        let picked = sketch.picked.filter(|&i| i < count);
        let delete = controls::button(ui, "Delete Shape", Tone::Danger, picked.is_some()).clicked();
        if let Some(index) = picked
            && (delete || pressed)
        {
            sketch.shapes.remove(index);
            sketch.picked = None;
            sketch.confirm = None;
            changed = true;
        }
        let now = ui.input(|i| i.time);
        let armed = sketch.confirm.is_some_and(|at| now - at < super::ARMED);
        let clear = if armed {
            "Click Again to Clear"
        } else {
            "Clear All"
        };
        if controls::button(ui, clear, Tone::Danger, count > 0).clicked() {
            if armed {
                sketch.shapes.clear();
                sketch.picked = None;
                changed = true;
            }
            sketch.confirm = (!armed).then_some(now);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(super::ARMED));
        }
        done = controls::button(ui, "Done", Tone::Primary, true).clicked();
    });
    (changed, done)
}

fn picked_kind(sketch: &Sketch) -> Option<&str> {
    sketch
        .picked
        .and_then(|i| sketch.shapes.get(i))
        .map(|s| s.kind.as_str())
}

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
    slid.widget_info(|| egui::WidgetInfo::slider(true, f64::from(size), "Brush Size"));
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

fn lettering(ui: &mut Ui, sketch: &mut Sketch) -> bool {
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
    let (typed, longest) = if boxed {
        let hint = "Type what the box says";
        (controls::notes(ui, &mut sketch.words, hint), 400)
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
    slid.widget_info(|| egui::WidgetInfo::slider(true, f64::from(percent), "Opacity"));
    if slid.changed() {
        sketch.opacity = percent / 100.0;
        if let Some(shape) = sketch.picked.and_then(|i| sketch.shapes.get_mut(i)) {
            shape.opacity = sketch.opacity;
            changed = true;
        }
    }
    changed
}

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
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, colour_name(hex))
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
