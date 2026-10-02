//! The map: its minimap with each site's plantable ground lit, the places the
//! game names, the shapes drawn on it, and a pin for where each lineup is
//! thrown from and where it lands. Browsing only looks: a click picks a
//! lineup and resting on one shows its notes. Once Add a Lineup is pressed, a
//! drag from where you stand to where it lands places both at once. While
//! drawing, a drag draws a shape or moves one by its dot, and a click places
//! a label. An export asks the window for a picture of it.

use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2};
use overseer_core::{Atlas, Lineup, Plan};
use overseer_ui::{Face, art, caps_text, caps_width, colour, motion, size, space};

use super::shapes::{self, KINDS, Square};
use super::{Mode, Place, View, ability, face};
use crate::controls;
use crate::settings::MapTurn;

/// Below this the minimap is too small for the callouts' names.
const NAMED: f32 = 440.0;
/// How near a click has to be to a pin to pick its lineup, in points.
const REACH: f32 = 16.0;
/// Half the side of the agent's square where a lineup is thrown from.
const FACE: f32 = 13.0;
/// The radius of the ring where it lands.
const RING: f32 = 11.0;
/// How near another lineup's landing spot one has to land to land on exactly
/// that spot, in game units: a metre. A hand on a mouse misses by about that
/// much, and two throws for different parts of a site are much further apart.
const SAME_SPOT: f32 = 100.0;
/// The radius of the dot a shape is dragged by.
const DOT: f32 = 5.0;
/// Half the side of a grip that resizes the picked shape.
const GRIP: f32 = 4.0;

/// Draws the map row and the map, and takes clicks and drags on the map.
pub(super) fn show(ui: &mut Ui, atlas: &Atlas, view: &mut View) {
    ui.add_space(space::LG);
    maps(ui, atlas, view);
    let Some(plan) = view
        .map
        .as_ref()
        .and_then(|m| atlas.maps.iter().find(|p| &p.name == m))
    else {
        return;
    };
    let room = ui
        .available_rect_before_wrap()
        .shrink2(vec2(space::XL, space::LG));
    let side = room.width().min(room.height());
    if side < 120.0 {
        return;
    }
    // Each map comes in as it loads or is picked, a little below its place.
    let shown = motion::enter(ui.ctx(), egui::Id::new(("lineups-map", &plan.name)), 0.0);
    let square = Square {
        rect: Rect::from_min_size(
            pos2(
                room.center().x - side / 2.0,
                motion::RISE.mul_add(1.0 - shown, room.top()),
            ),
            vec2(side, side),
        ),
        turn: (turn_for(plan, view.turn_to) + view.turned) % 4,
    };
    let response = ui.allocate_rect(*square, Sense::click_and_drag());
    let mut painter = ui.painter_at(square.expand(space::XL));
    painter.multiply_opacity(shown);
    backdrop(&painter, square, plan);
    if side >= NAMED {
        callouts(&painter, square, plan);
    }
    for drawing in view.shapes(atlas) {
        shapes::paint(&painter, square, drawing);
    }
    pins(&painter, square, atlas, view, plan.scale);
    if view.export.is_some() {
        // The picture is of the map alone, without the hint along its edge.
        if view.shot.is_none() {
            view.shot = Some(*square);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    } else {
        prompt(&painter, square, view);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if matches!(view.mode, Mode::Draw(_)) {
        draw(ui, &painter, square, &response, view);
    } else {
        drag(ui, atlas, view, square, &response);
        if response.clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            click(atlas, view, square, at);
        }
        if matches!(view.mode, Mode::Browse)
            && let Some(at) = response.hover_pos()
            && let Some(lineup) = nearest(atlas, view, square, at)
        {
            let _shown = response
                .clone()
                .on_hover_ui_at_pointer(|ui| note(ui, atlas, lineup));
        }
    }
}

/// A chip for every map. The map stays put while a lineup is being written
/// or shapes drawn on it.
fn maps(ui: &mut Ui, atlas: &Atlas, view: &mut View) {
    let browsing = matches!(view.mode, Mode::Browse);
    let (names, turn) = ("Names (T)", "Turn (R)");
    let tools = controls::chip_width(ui, names) + controls::chip_width(ui, turn) + space::SM;
    let room = space::XL.mul_add(-2.0, ui.available_width() - tools);
    ui.horizontal_top(|ui| {
        ui.allocate_ui(vec2(room, 0.0), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(space::XL);
                ui.spacing_mut().item_spacing = vec2(space::SM, space::SM);
                for plan in &atlas.maps {
                    let on = view.map.as_ref() == Some(&plan.name);
                    if controls::chip(ui, &plan.name, on).clicked() && browsing && !on {
                        view.map = Some(plan.name.clone());
                        view.selected = None;
                        view.confirm = false;
                    }
                }
            });
        });
        // Every lineup's name on the map, lit while they show, and a quarter
        // turn of the map. T and R do the same.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(space::XL);
            ui.spacing_mut().item_spacing = vec2(space::SM, space::SM);
            let shown = view.naming();
            if controls::chip(ui, names, shown).clicked() {
                view.names = Some(!shown);
            }
            if controls::chip(ui, turn, view.turned != 0).clicked() {
                view.turned = (view.turned + 1) % 4;
            }
        });
    });
}

/// Where a point on the minimap is on screen.
fn point(square: Square, at: [f32; 2]) -> Pos2 {
    shapes::at(square, at)
}

/// How many quarter turns put the side `turn_to` asks for at the bottom of
/// `plan`, along the line from the other side's spawn to its own, since one
/// side can start in the middle, like the defenders on Fracture.
fn turn_for(plan: &Plan, turn_to: MapTurn) -> u8 {
    let (Some(attack), Some(defend)) = (plan.attack, plan.defend) else {
        return 0;
    };
    let (from, to) = match turn_to {
        MapTurn::Attack => (defend, attack),
        MapTurn::Defend => (attack, defend),
        MapTurn::Drawn => return 0,
    };
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    // How far down the line points after each number of quarter turns.
    let down = [dy, dx, -dy, -dx];
    down.iter()
        .zip(0..4u8)
        .max_by(|(a, _), (b, _)| a.total_cmp(b))
        .map_or(0, |(_, turn)| turn)
}

/// Draws `texture` filling the map's square, turned with it.
fn turned_image(painter: &egui::Painter, square: Square, texture: egui::TextureId, tint: Color32) {
    let mut mesh = egui::Mesh::with_texture(texture);
    for corner in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
        let [u, v] = shapes::turned(corner, 4 - square.turn % 4);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: square.min + vec2(corner[0], corner[1]) * square.width(),
            uv: pos2(u, v),
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// The minimap on black with its plantable ground lit, or a line saying it
/// isn't here yet.
fn backdrop(painter: &egui::Painter, square: Square, plan: &Plan) {
    painter.rect_filled(*square, 0, colour::VOID);
    match plan
        .minimap
        .as_deref()
        .and_then(|path| art::file(painter.ctx(), path))
    {
        Some(texture) => {
            // Dark enough that the names and pins read on its floors.
            turned_image(painter, square, texture.id(), Color32::from_gray(118));
            // The game's minimap tints where the spike can be planted, the
            // same olive on every map, so lighting that shows each plantable
            // area exactly without anybody drawing it.
            if let Some(plantable) = plan
                .minimap
                .as_deref()
                .and_then(|path| art::file_where(painter.ctx(), path, "plantable", plantable))
            {
                turned_image(
                    painter,
                    square,
                    plantable.id(),
                    colour::WARN.gamma_multiply(0.42),
                );
            }
        }
        None => {
            painter.text(
                square.center(),
                Align2::CENTER_CENTER,
                format!(
                    "The {} minimap hasn't downloaded. Open Lineups again to retry.",
                    plan.name
                ),
                Face::Body.at(size::BODY),
                colour::TEXT_DIM,
            );
        }
    }
}

/// Whether a minimap pixel is the olive the game tints plantable ground in.
fn plantable(pixel: Color32) -> bool {
    let [r, g, b, a] = pixel.to_srgba_unmultiplied();
    a > 200 && r.abs_diff(g) < 6 && r > b.saturating_add(20)
}

/// The names the game gives places, and each site's letter in amber. Each
/// name sits on a dark backing so it reads over a wall as well as a floor.
fn callouts(painter: &egui::Painter, square: Square, plan: &Plan) {
    let font = Face::Display.at(size::MICRO);
    let count = plan.callouts.len();
    let spots = spots(painter, square, plan, &font);
    for (index, (callout, spot)) in plan.callouts.iter().zip(spots).enumerate() {
        // The names follow the map in, one after another.
        let id = egui::Id::new(("callout", &plan.name, index));
        let shown = motion::enter(painter.ctx(), id, 0.15 + motion::stagger(index, count));
        let mut painter = painter.clone();
        painter.multiply_opacity(shown);
        let painter = &painter;
        let at = square.min
            + vec2(spot[0], spot[1]) * square.width()
            + vec2(0.0, motion::RISE * 0.5 * (1.0 - shown));
        if let Some(letter) = callout.name.strip_suffix(" Site") {
            let _letter = caps_text(
                painter,
                at,
                Align2::CENTER_CENTER,
                letter,
                Face::Heavy.at(28.0),
                colour::WARN.gamma_multiply(0.9),
            );
        } else {
            let wide = caps_width(painter, &callout.name, font.clone());
            let plate = Rect::from_center_size(at, vec2(wide + 8.0, PLATE));
            painter.rect_filled(plate, 3.0, Color32::from_black_alpha(150));
            let _name = caps_text(
                painter,
                plate.center(),
                Align2::CENTER_CENTER,
                &callout.name,
                font.clone(),
                colour::TEXT_DIM,
            );
        }
    }
}

/// How tall a callout's backing is, in points.
const PLATE: f32 = 14.0;
/// How many cells each side of the map is split into to find floor for the
/// names on.
const CELLS: u16 = 256;
/// How far a name may move from Riot's spot to find floor, in cells, about
/// a tenth of the map.
const NEAR: i32 = 26;
/// The space kept between a name and a wall or another name, in points.
const GAP: f32 = 6.0;

/// Where each callout goes on the map as it is turned, 0 to 1 each way.
/// Riot's own spots put a few names half on a wall or off the drawn map into
/// the dark, so each name goes on the nearest spot to Riot's where all of it
/// is on the floor and clear of the names before it, the site letters first.
/// Worked out once for each map, turn and size.
fn spots(
    painter: &egui::Painter,
    square: Square,
    plan: &Plan,
    font: &egui::FontId,
) -> Vec<[f32; 2]> {
    let side = square.width();
    let id = egui::Id::new((
        "callout-spots",
        &plan.name,
        square.turn,
        side.round() as i32,
    ));
    let ctx = painter.ctx();
    if let Some(kept) = ctx.data(|store| store.get_temp::<Vec<[f32; 2]>>(id)) {
        return kept;
    }
    let image = plan
        .minimap
        .as_deref()
        .and_then(|path| art::file_pixels(ctx, path));
    let floor = Floor::new(image.as_deref(), square.turn);
    let cells = f32::from(CELLS);
    let cell = side / cells;
    let labels: Vec<Label> = plan
        .callouts
        .iter()
        .map(|c| {
            let at = shapes::turned(c.at, square.turn);
            let letter = c.name.ends_with(" Site");
            let backing = if letter {
                vec2(24.0, 32.0)
            } else {
                vec2(caps_width(painter, &c.name, font.clone()) + 8.0, PLATE)
            };
            Label {
                at: [at[0] * cells, at[1] * cells],
                size: [(backing.x + GAP) / cell, (backing.y + GAP) / cell],
                letter,
            }
        })
        .collect();
    let placed: Vec<[f32; 2]> = floor
        .place(&labels)
        .into_iter()
        .map(|[x, y]| [x / cells, y / cells])
        .collect();
    ctx.data_mut(|store| store.insert_temp(id, placed.clone()));
    placed
}

/// A name to put on the map, in cells of the map as it is turned.
struct Label {
    /// Riot's spot for it.
    at: [f32; 2],
    /// Its backing with the gap round it.
    size: [f32; 2],
    /// A site's letter, which goes down before the names.
    letter: bool,
}

/// A box of cells, from its top left corner to just past its bottom right.
type Cells = [i32; 4];

/// Which cells of the map, as it is turned, are floor, kept as running
/// totals so how much of any box is floor takes four lookups.
struct Floor {
    sums: Vec<u32>,
}

impl Floor {
    /// The floor of `image` turned `turn` quarter turns, or all floor when
    /// the minimap isn't on disk.
    fn new(image: Option<&egui::ColorImage>, turn: u8) -> Self {
        let row = usize::from(CELLS) + 1;
        let cells = f32::from(CELLS);
        let on = |gx: u16, gy: u16| {
            image.is_none_or(|image| {
                let [w, h] = image.size;
                let shown = [(f32::from(gx) + 0.5) / cells, (f32::from(gy) + 0.5) / cells];
                let [x, y] = shapes::turned(shown, 4 - turn % 4);
                let px = ((x * w as f32) as usize).min(w.saturating_sub(1));
                let py = ((y * h as f32) as usize).min(h.saturating_sub(1));
                image.pixels.get(py * w + px).is_some_and(|p| p.a() > 128)
            })
        };
        let mut sums = vec![0_u32; row];
        for gy in 0..CELLS {
            let above = sums.len() - row;
            let mut line = 0;
            sums.push(0);
            for gx in 0..CELLS {
                line += u32::from(on(gx, gy));
                let over = sums.get(above + usize::from(gx) + 1).copied().unwrap_or(0);
                sums.push(line + over);
            }
        }
        Self { sums }
    }

    /// The floor cells above and left of the corner `x`, `y`.
    fn upto(&self, x: i32, y: i32) -> u32 {
        let row = i32::from(CELLS) + 1;
        usize::try_from(y * row + x)
            .ok()
            .and_then(|at| self.sums.get(at))
            .copied()
            .unwrap_or(0)
    }

    /// How many cells of a box are floor.
    fn count(&self, [x0, y0, x1, y1]: Cells) -> u32 {
        (self.upto(x1, y1) + self.upto(x0, y0))
            .saturating_sub(self.upto(x0, y1) + self.upto(x1, y0))
    }

    /// Where each of `labels` is centred, in cells, letters first.
    fn place(&self, labels: &[Label]) -> Vec<[f32; 2]> {
        let mut near: Vec<(i32, i32)> = (-NEAR..=NEAR)
            .flat_map(|dy| (-NEAR..=NEAR).map(move |dx| (dx, dy)))
            .filter(|(dx, dy)| dx * dx + dy * dy <= NEAR * NEAR)
            .collect();
        near.sort_by_key(|(dx, dy)| dx * dx + dy * dy);
        let mut taken: Vec<Cells> = Vec::new();
        let mut placed: Vec<[f32; 2]> = labels.iter().map(|l| l.at).collect();
        for letters in [true, false] {
            for (spot, label) in placed.iter_mut().zip(labels) {
                if label.letter != letters {
                    continue;
                }
                if let Some(b) = self.best(label, &near, &taken) {
                    taken.push(b);
                    *spot = [(b[0] + b[2]) as f32 / 2.0, (b[1] + b[3]) as f32 / 2.0];
                }
            }
        }
        placed
    }

    /// The box for `label` nearest its own spot, trying each of `near` in
    /// turn, that is all floor and clear of `taken`, or else the clear one
    /// with the most floor.
    fn best(&self, label: &Label, near: &[(i32, i32)], taken: &[Cells]) -> Option<Cells> {
        let edge = i32::from(CELLS);
        let (wide, tall) = (label.size[0].ceil() as i32, label.size[1].ceil() as i32);
        let left = (label.at[0] - label.size[0] / 2.0).round() as i32;
        let top = (label.at[1] - label.size[1] / 2.0).round() as i32;
        let whole = u32::try_from(wide * tall).unwrap_or(u32::MAX);
        let mut best: Option<(u32, Cells)> = None;
        for &(dx, dy) in near {
            let at = [left + dx, top + dy, left + dx + wide, top + dy + tall];
            let inside = at[0] >= 0 && at[1] >= 0 && at[2] <= edge && at[3] <= edge;
            let clear = !taken
                .iter()
                .any(|t| at[0] < t[2] && t[0] < at[2] && at[1] < t[3] && t[1] < at[3]);
            if !inside || !clear {
                continue;
            }
            let floor = self.count(at);
            if best.is_none_or(|(most, _)| floor > most) {
                best = Some((floor, at));
            }
            if floor >= whole {
                break;
            }
        }
        best.map(|(_, b)| b)
    }
}

/// What a click or a drag on the map does next, on a plate along its bottom
/// edge, so the map says how to use it without anybody finding a button.
fn prompt(painter: &egui::Painter, square: Square, view: &View) {
    let drawing;
    let words = match &view.mode {
        Mode::Browse => "Click a lineup to see it, or rest on one for its notes",
        Mode::Edit(draft) => match (draft.placing, draft.lineup.land.is_some()) {
            (Place::Stand, _) => "Click or drag from where you stand",
            (Place::Land, false) => "Now click where it lands",
            (Place::Land, true) => "Drag either pin to move it",
        },
        Mode::Draw(sketch) => {
            let name = KINDS.get(sketch.kind).map_or("shape", |(_, name)| name);
            drawing = match sketch.tool() {
                "text" => "Type the words on the right, then click where they go".to_owned(),
                "brush" => "Drag to draw freehand, or drag a dot to move a stroke".to_owned(),
                "erase" => "Click a drawing to erase it, or drag across several".to_owned(),
                _ => format!(
                    "Drag to draw a {name}, drag a dot to move one, or a square to resize it"
                ),
            };
            &drawing
        }
    };
    let font = Face::Display.at(size::TITLE);
    let wide = space::XL.mul_add(2.0, caps_width(painter, words, font.clone()));
    let plate = Rect::from_center_size(
        pos2(square.center().x, square.bottom() - 28.0),
        vec2(wide, 30.0),
    );
    painter.add(crate::board::paint::slant(
        plate,
        true,
        true,
        colour::VOID.gamma_multiply(0.9),
    ));
    let _words = caps_text(
        painter,
        plate.center(),
        Align2::CENTER_CENTER,
        words,
        font,
        colour::TEXT_STRONG,
    );
}

/// A drag draws a shape, shown as it goes, unless it starts on a shape's dot,
/// which moves that shape instead. A right click takes off the newest shape
/// under the pointer.
fn draw(ui: &Ui, painter: &egui::Painter, square: Square, response: &Response, view: &mut View) {
    let map = view.map.clone().unwrap_or_default();
    let Mode::Draw(sketch) = &mut view.mode else {
        return;
    };
    if sketch.tool() == "erase" {
        if erase(sketch, square, response) {
            view.unsaved = Some((map, sketch.shapes.clone()));
        }
        return;
    }
    let mut changed = false;
    let dot_at = |shapes: &[overseer_core::Drawing], at: Pos2| dot_at(square, shapes, at);
    let grip_on = |sketch: &super::Sketch, at: Pos2| grip_on(square, sketch, at);
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        take_hold(sketch, square, origin);
    }
    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        if grip_on(sketch, at).is_some() {
            // A click on a grip is a resize that didn't move.
        } else if let Some(index) = dot_at(&sketch.shapes, at) {
            sketch.pick(index);
        } else if sketch.lettering() && !sketch.words.trim().is_empty() {
            let spot = shapes::spot(square, at);
            sketch.shapes.push(sketch.drawing(spot, spot));
            sketch.picked = sketch.shapes.len().checked_sub(1);
            changed = true;
        }
    }
    let hover = ui.input(|i| i.pointer.latest_pos());
    let now = hover.map(|at| shapes::spot(square, at));
    if let (Some((index, [fx, fy], was)), Some([x, y])) = (&sketch.moving, now) {
        if let Some(shape) = sketch.shapes.get_mut(*index) {
            *shape = shapes::moved(was, [x - fx, y - fy]);
        }
        if response.drag_stopped() {
            sketch.moving = None;
            changed = true;
        }
    }
    changed |= resize(sketch, square, now, response.drag_stopped());
    changed |= brush(painter, sketch, square, now, response.drag_stopped());
    if let (Some(start), Some(end)) = (sketch.dragging, now) {
        shapes::paint(painter, square, &sketch.drawing(start, end));
        if response.drag_stopped() {
            if shapes::long_enough(square, start, end) {
                sketch.shapes.push(sketch.drawing(start, end));
                sketch.picked = sketch.shapes.len().checked_sub(1);
                changed = true;
            }
            sketch.dragging = None;
        }
    }
    if response.secondary_clicked()
        && let Some(at) = response.interact_pointer_pos()
        && let Some(index) = sketch
            .shapes
            .iter()
            .rposition(|s| shapes::hit(square, s, at))
    {
        sketch.shapes.remove(index);
        sketch.picked = None;
        changed = true;
    }
    dots(painter, square, sketch);
    pointer(
        ui,
        square,
        sketch,
        response.hovered().then_some(hover).flatten(),
    );
    if changed {
        sketch.confirm = false;
        view.unsaved = Some((map, sketch.shapes.clone()));
    }
}

/// Takes off the newest drawing under a click, or under the pointer as it
/// sweeps across the map, and leaves the lineups alone. True when one went.
fn erase(sketch: &mut super::Sketch, square: Square, response: &Response) -> bool {
    let touched = (response.clicked() || response.dragged())
        .then(|| response.interact_pointer_pos())
        .flatten();
    let found = touched.and_then(|at| {
        sketch
            .shapes
            .iter()
            .rposition(|s| shapes::hit(square, s, at))
    });
    let Some(index) = found else {
        return false;
    };
    sketch.shapes.remove(index);
    sketch.picked = None;
    sketch.confirm = false;
    true
}

/// Every shape's dot, the picked one lit in its own colour and with its
/// grips, small cream squares at its corners or its far end.
fn dots(painter: &egui::Painter, square: Square, sketch: &super::Sketch) {
    for (index, shape) in sketch.shapes.iter().enumerate() {
        let picked = sketch.picked == Some(index);
        let fill = if picked {
            shapes::ink(&shape.colour)
        } else {
            colour::VOID.gamma_multiply(0.85)
        };
        let at = shapes::handle(square, shape);
        painter.circle(at, DOT, fill, Stroke::new(1.5, colour::TEXT_STRONG));
        if !picked {
            continue;
        }
        for grip in shapes::grips(shape) {
            let spot =
                Rect::from_center_size(shapes::grip_at(square, grip), vec2(GRIP * 2.0, GRIP * 2.0));
            painter.rect(
                spot,
                1.0,
                colour::TEXT_STRONG,
                Stroke::new(1.5, colour::VOID),
                egui::StrokeKind::Outside,
            );
        }
    }
}

/// Starts a drag at `origin`: on a grip of the picked shape it resizes it,
/// on a dot it moves that shape, and anywhere else it draws a new one.
fn take_hold(sketch: &mut super::Sketch, square: Square, origin: Pos2) {
    if let Some((index, grip)) = grip_on(square, sketch, origin) {
        sketch.resizing = sketch.shapes.get(index).map(|s| (index, grip, s.clone()));
        return;
    }
    match dot_at(square, &sketch.shapes, origin) {
        Some(index) => {
            sketch.moving = sketch
                .shapes
                .get(index)
                .map(|s| (index, shapes::spot(square, origin), s.clone()));
            sketch.pick(index);
        }
        None if sketch.lettering() => {}
        None if sketch.tool() == "brush" => sketch.stroke = vec![shapes::spot(square, origin)],
        None => sketch.dragging = Some(shapes::spot(square, origin)),
    }
}

/// The newest shape whose dot is under `at`.
fn dot_at(square: Square, shapes: &[overseer_core::Drawing], at: Pos2) -> Option<usize> {
    shapes
        .iter()
        .rposition(|s| shapes::handle(square, s).distance(at) <= DOT + 4.0)
}

/// The picked shape and which of its grips is under `at`. Only the picked
/// shape has grips, and they come before its dot and before drawing a new
/// shape, since they sit on top.
fn grip_on(square: Square, sketch: &super::Sketch, at: Pos2) -> Option<(usize, usize)> {
    let index = sketch.picked?;
    let shape = sketch.shapes.get(index)?;
    shapes::grips(shape)
        .iter()
        .position(|&g| shapes::grip_at(square, g).distance(at) <= GRIP + 4.0)
        .map(|grip| (index, grip))
}

/// Follows the brush to `now`, drawing the stroke so far. True once it is
/// let go with a stroke worth keeping.
fn brush(
    painter: &egui::Painter,
    sketch: &mut super::Sketch,
    square: Square,
    now: Option<[f32; 2]>,
    stopped: bool,
) -> bool {
    let Some(&last) = sketch.stroke.last() else {
        return false;
    };
    // A point every couple of pixels is smooth enough, and keeps a long
    // stroke to a few hundred points.
    if let Some(to) = now
        && shapes::at(square, last).distance(shapes::at(square, to)) >= 2.0
    {
        sketch.stroke.push(to);
    }
    let first = sketch.stroke.first().copied().unwrap_or(last);
    let drawn = sketch.drawing(first, sketch.stroke.last().copied().unwrap_or(first));
    shapes::paint(painter, square, &drawn);
    if !stopped {
        return false;
    }
    sketch.stroke.clear();
    let (low, high) = drawn
        .points
        .iter()
        .fold(([1.0_f32; 2], [0.0_f32; 2]), |(l, h), p| {
            (
                [l[0].min(p[0]), l[1].min(p[1])],
                [h[0].max(p[0]), h[1].max(p[1])],
            )
        });
    if drawn.points.len() < 2 || !shapes::long_enough(square, low, high) {
        return false;
    }
    sketch.shapes.push(drawn);
    sketch.picked = sketch.shapes.len().checked_sub(1);
    true
}

/// Follows a grip being dragged to `now`. True once it is let go, when the
/// new size wants keeping.
fn resize(
    sketch: &mut super::Sketch,
    square: Square,
    now: Option<[f32; 2]>,
    stopped: bool,
) -> bool {
    let (Some((index, grip, was)), Some(to)) = (&sketch.resizing, now) else {
        return false;
    };
    let bigger = shapes::resized(was, *grip, to);
    // Too small to see or to take hold of again is a slip, so it keeps the
    // size it had.
    if let Some(shape) = sketch.shapes.get_mut(*index)
        && shapes::long_enough(square, bigger.a, bigger.b)
    {
        *shape = bigger;
    }
    if stopped {
        sketch.resizing = None;
    }
    stopped
}

/// The pointer for what a press would take hold of: a shape being moved, a
/// grip, or a dot. `hover` is where it is over the map, if it is.
fn pointer(ui: &Ui, square: Square, sketch: &super::Sketch, hover: Option<Pos2>) {
    let gripped = sketch
        .resizing
        .as_ref()
        .map(|r| (r.0, r.1))
        .or_else(|| hover.and_then(|at| grip_on(square, sketch, at)));
    if sketch.moving.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if let Some((index, grip)) = gripped {
        ui.ctx()
            .set_cursor_icon(grip_cursor(sketch.shapes.get(index), grip));
    } else if hover.is_some_and(|at| dot_at(square, &sketch.shapes, at).is_some()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
}

/// The pointer over a grip: the diagonal arrows a corner drags along, or a
/// move cross for the one grip of a circle, a cone or a triangle.
fn grip_cursor(shape: Option<&overseer_core::Drawing>, grip: usize) -> egui::CursorIcon {
    let Some(shape) = shape.filter(|s| matches!(s.kind.as_str(), "rect" | "oval" | "textbox"))
    else {
        return egui::CursorIcon::Move;
    };
    // The corners go round from the first point, and every other one leans
    // the same way, which way depending on how the shape was drawn.
    let ([ax, ay], [bx, by]) = (shape.a, shape.b);
    let leans = (bx - ax) * (by - ay) >= 0.0;
    if grip.is_multiple_of(2) == leans {
        egui::CursorIcon::ResizeNwSe
    } else {
        egui::CursorIcon::ResizeNeSw
    }
}

/// The colour of a lineup's side: the attackers' red or the defenders' blue.
fn tint(side: Option<&str>) -> Color32 {
    match side {
        Some("attack") => colour::ENEMY,
        Some("defense") => colour::INFO,
        _ => colour::TEXT_DIM,
    }
}

/// Every lineup on the map, the picked one last so it is on top, and the one
/// being written in cream.
fn pins(painter: &egui::Painter, square: Square, atlas: &Atlas, view: &View, scale: Option<f32>) {
    let main = view.main.as_deref();
    let writing = match &view.mode {
        Mode::Edit(draft) => Some(&draft.lineup),
        Mode::Browse | Mode::Draw(_) => None,
    };
    let (picked, rest): (Vec<&Lineup>, Vec<&Lineup>) = view
        .shown(atlas)
        .into_iter()
        .filter(|l| writing.is_none_or(|w| w.id.is_none() || w.id != l.id))
        .partition(|l| writing.is_none() && l.id.is_some() && l.id == view.selected);
    let dim = writing.is_some() || !picked.is_empty();
    // Every area first, so no pin is under somebody else's area.
    let areas = |lineup: &Lineup, ink: Color32| area(painter, square, lineup, scale, ink);
    let faint =
        |lineup: &Lineup| tint(lineup.side.as_deref()).gamma_multiply(if dim { 0.45 } else { 0.9 });
    // In the colour the settings pick, or else the side's, dimmed the same
    // as its pin.
    let shade = |l: &Lineup| view.area.unwrap_or_else(|| tint(l.side.as_deref()));
    let dimmed = if dim { 0.45 } else { 0.9 };
    let all = rest
        .iter()
        .map(|l| (*l, shade(l).gamma_multiply(dimmed)))
        .chain(picked.iter().map(|l| (*l, shade(l))))
        .chain(writing.map(|l| (l, view.area.unwrap_or(colour::TEXT_STRONG))));
    for (lineup, ink) in all {
        areas(lineup, ink);
    }
    for lineup in &rest {
        pin(painter, square, atlas, lineup, (faint(lineup), main));
    }
    // The picked one's name first, so the rest move out of its way.
    let mut taken = Vec::new();
    for lineup in &picked {
        pin(
            painter,
            square,
            atlas,
            lineup,
            (tint(lineup.side.as_deref()), main),
        );
        label(painter, square, lineup, (&mut taken, colour::TEXT_STRONG));
    }
    if view.naming() && writing.is_none() {
        for lineup in &rest {
            label(painter, square, lineup, (&mut taken, colour::TEXT_DIM));
        }
    }
    if let Some(lineup) = writing {
        pin(painter, square, atlas, lineup, (colour::TEXT_STRONG, main));
    }
}

/// How far an ability hurts from where it lands, in game units, a hundredth
/// of a metre: its edge, and the inner circle where it does full damage when
/// it has one. From Riot's wiki, Hot Hands' being an estimate there. Only the
/// ones that leave an area on the ground.
fn reach(ability: &str) -> Option<(f32, Option<f32>)> {
    match ability {
        "Incendiary" | "Snake Bite" | "Nanoswarm" | "Hot Hands" => Some((450.0, None)),
        "FRAG/ment" => Some((400.0, Some(100.0))),
        "Mosh Pit" => Some((620.0, Some(550.0))),
        _ => None,
    }
}

/// The ground a lineup's ability covers where it lands, to the map's scale:
/// faint inside, its edge solid, and the full damage circle inside it when
/// there is one. It moves with the pin, since it is drawn from it.
fn area(
    painter: &egui::Painter,
    square: Square,
    lineup: &Lineup,
    scale: Option<f32>,
    ink: Color32,
) {
    let (Some(land), Some((edge, inner)), Some(scale)) = (
        lineup.land,
        lineup.ability.as_deref().and_then(reach),
        scale,
    ) else {
        return;
    };
    let at = point(square, land);
    let size = |units: f32| units * scale * square.width();
    painter.circle(
        at,
        size(edge),
        ink.gamma_multiply(0.14),
        Stroke::new(1.5, ink.gamma_multiply(0.7)),
    );
    if let Some(inner) = inner {
        painter.circle_stroke(at, size(inner), Stroke::new(1.0, ink.gamma_multiply(0.5)));
    }
}

/// Where a lineup is thrown from and where it lands, joined by a dashed line.
fn pin(
    painter: &egui::Painter,
    square: Square,
    atlas: &Atlas,
    lineup: &Lineup,
    (ink, main): (Color32, Option<&str>),
) {
    let stand = lineup.stand.map(|p| point(square, p));
    let land = lineup.land.map(|p| point(square, p));
    if let (Some(from), Some(to)) = (stand, land)
        && from.distance(to) > FACE + RING
    {
        let along = (to - from).normalized();
        painter.add(Shape::dashed_line(
            &[from + along * FACE, to - along * RING],
            Stroke::new(1.5, ink),
            5.0,
            4.0,
        ));
    }
    if let Some(to) = land {
        land_mark(painter, to, RING, atlas, lineup, ink);
    }
    if let Some(from) = stand {
        stand_mark(painter, from, FACE, lineup, (ink, main));
    }
}

/// Where a lineup lands: a ring `radius` round, with the ability's icon in it
/// when there is one.
pub(super) fn land_mark(
    painter: &egui::Painter,
    at: Pos2,
    radius: f32,
    atlas: &Atlas,
    lineup: &Lineup,
    ink: Color32,
) {
    painter.circle_filled(at, radius, colour::VOID.gamma_multiply(0.85));
    painter.circle_stroke(at, radius, Stroke::new(2.0, ink));
    let icon = lineup
        .ability
        .as_deref()
        .and_then(|name| ability(atlas, &lineup.agent, name))
        .and_then(|(_, a)| a.icon.as_deref())
        .and_then(|path| art::file(painter.ctx(), path));
    if let Some(icon) = icon {
        let side = radius * 1.27;
        painter.image(
            icon.id(),
            Rect::from_center_size(at, vec2(side, side)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            ink,
        );
    }
}

/// Where a lineup is thrown from: the agent's face in a square `half` either
/// side of `at`.
pub(super) fn stand_mark(
    painter: &egui::Painter,
    at: Pos2,
    half: f32,
    lineup: &Lineup,
    (ink, main): (Color32, Option<&str>),
) {
    let square = Rect::from_center_size(at, vec2(half * 2.0, half * 2.0));
    painter.rect_filled(square, 0, colour::VOID);
    face(
        painter,
        square.shrink(2.0),
        &lineup.agent,
        f32::from(ink.a()) / 255.0,
        main,
    );
    painter.rect_stroke(square, 0, Stroke::new(1.5, ink), egui::StrokeKind::Inside);
}

/// The picked lineup's title above where it lands.
fn label(
    painter: &egui::Painter,
    square: Square,
    lineup: &Lineup,
    (taken, ink): (&mut Vec<Rect>, Color32),
) {
    let Some(at) = lineup.land.or(lineup.stand).map(|p| point(square, p)) else {
        return;
    };
    let font = Face::Display.at(size::LABEL);
    let wide = space::MD.mul_add(2.0, caps_width(painter, &lineup.title, font.clone()));
    let plate = clear_of(
        taken,
        Rect::from_center_size(at - vec2(0.0, RING + 14.0), vec2(wide, 18.0)),
    );
    taken.push(plate);
    painter.rect_filled(plate, 0, colour::VOID.gamma_multiply(0.9));
    let _title = caps_text(
        painter,
        plate.center(),
        Align2::CENTER_CENTER,
        &lineup.title,
        font,
        ink,
    );
}

/// `plate` moved just above or below any title it would cover, trying the
/// nearer side first, so lineups landing on one spot keep their titles apart.
fn clear_of(taken: &[Rect], plate: Rect) -> Rect {
    let step = plate.height() + 2.0;
    for shift in [0.0, -step, step, -2.0 * step, 2.0 * step] {
        let moved = plate.translate(vec2(0.0, shift));
        if !taken.iter().any(|t| t.intersects(moved)) {
            return moved;
        }
    }
    plate
}

/// A drag on the map while a lineup is being written: from where you stand
/// to where it lands, or one of its pins picked up and moved.
fn drag(ui: &Ui, atlas: &Atlas, view: &mut View, square: Square, response: &Response) {
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        grab(atlas, view, square, origin);
    }
    if let Some(now) = ui.input(|i| i.pointer.latest_pos()) {
        follow(atlas, view, square, now, response.drag_stopped());
    }
    let held = matches!(&view.mode, Mode::Edit(draft) if draft.grab.is_some());
    if held {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
}

/// Starts a drag at `origin` on the lineup being written. On one of its pins
/// it picks that pin up. Anywhere else it throws from there, with where it
/// lands following the pointer. Browsing, a drag does nothing.
fn grab(atlas: &Atlas, view: &mut View, square: Square, origin: Pos2) {
    let Mode::Edit(draft) = &mut view.mode else {
        return;
    };
    let on = |p: Option<[f32; 2]>| p.is_some_and(|p| point(square, p).distance(origin) <= REACH);
    draft.grab = Some(if on(draft.lineup.land) {
        Place::Land
    } else if on(draft.lineup.stand) {
        Place::Stand
    } else {
        let spot = shapes::spot(square, origin);
        draft.lineup.stand = Some(snapped(atlas, &draft.lineup, spot, Place::Stand));
        Place::Land
    });
}

/// Moves the pin being held to `now`, and lets go of it once the drag is
/// `done`. Where it lands dropped back on where you stand is taken back, as
/// that was a press held still rather than a throw.
fn follow(atlas: &Atlas, view: &mut View, square: Square, now: Pos2, done: bool) {
    let Mode::Edit(draft) = &mut view.mode else {
        return;
    };
    let Some(place) = draft.grab else {
        return;
    };
    let spot = Some(shapes::spot(square, now));
    match place {
        Place::Stand => {
            draft.lineup.stand = spot.map(|s| snapped(atlas, &draft.lineup, s, Place::Stand));
        }
        Place::Land => {
            draft.lineup.land = spot.map(|s| snapped(atlas, &draft.lineup, s, Place::Land));
        }
    }
    if done {
        draft.grab = None;
        draft.placing = Place::Land;
        if let (Some(from), Some(to)) = (draft.lineup.stand, draft.lineup.land)
            && point(square, from).distance(point(square, to)) < FACE
        {
            draft.lineup.land = None;
        }
    }
}

/// `spot` moved onto the nearest pin of the same kind on another lineup on
/// the same map within `SAME_SPOT`: any lineup's landing spot, so two lineups
/// for one spot share one ring, or where a lineup for the same agent is
/// thrown from, so one corner is one agent pin.
fn snapped(atlas: &Atlas, lineup: &Lineup, spot: [f32; 2], place: Place) -> [f32; 2] {
    let Some(scale) = atlas
        .maps
        .iter()
        .find(|p| p.name == lineup.map)
        .and_then(|p| p.scale)
    else {
        return spot;
    };
    atlas
        .lineups
        .iter()
        .filter(|l| l.map == lineup.map && (lineup.id.is_none() || l.id != lineup.id))
        .filter_map(|l| match place {
            Place::Land => l.land,
            Place::Stand => l
                .stand
                .filter(|_| l.agent.eq_ignore_ascii_case(&lineup.agent)),
        })
        .map(|p| ((p[0] - spot[0]).hypot(p[1] - spot[1]), p))
        .filter(|(far, _)| *far <= SAME_SPOT * scale)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(spot, |(_, p)| p)
}

/// The saved lineup with a pin nearest `at`, when one is in reach.
fn nearest<'a>(atlas: &'a Atlas, view: &View, square: Square, at: Pos2) -> Option<&'a Lineup> {
    view.shown(atlas)
        .into_iter()
        .filter_map(|l| {
            let reach = [l.stand, l.land]
                .into_iter()
                .flatten()
                .map(|p| point(square, p).distance(at))
                .fold(f32::INFINITY, f32::min);
            (reach <= REACH).then_some((reach, l))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, l)| l)
}

/// What shows beside the pointer resting on a lineup's pin: its name, what
/// it is, and its notes on how to throw it.
fn note(ui: &mut Ui, atlas: &Atlas, lineup: &Lineup) {
    ui.set_max_width(300.0);
    ui.label(overseer_ui::caps(
        &lineup.title,
        Face::Display.at(size::TITLE),
        colour::TEXT_STRONG,
    ));
    let about = super::side::about(atlas, lineup);
    if !about.is_empty() {
        ui.label(overseer_ui::caps(
            &about,
            Face::Display.at(size::MICRO),
            colour::TEXT_DIM,
        ));
    }
    if let Some(notes) = lineup
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        ui.add_space(space::SM);
        ui.label(egui::RichText::new(notes.to_uppercase()).color(colour::TEXT));
    }
}

/// A click on the map: a place for the lineup being written, or else the
/// lineup under the pointer, or else nothing picked.
fn click(atlas: &Atlas, view: &mut View, square: Square, at: Pos2) {
    let spot = shapes::spot(square, at);
    if let Mode::Edit(draft) = &mut view.mode {
        match draft.placing {
            Place::Stand => {
                draft.lineup.stand = Some(snapped(atlas, &draft.lineup, spot, Place::Stand));
                if draft.lineup.land.is_none() {
                    draft.placing = Place::Land;
                }
            }
            Place::Land => {
                draft.lineup.land = Some(snapped(atlas, &draft.lineup, spot, Place::Land));
            }
        }
        return;
    }
    view.confirm = false;
    view.selected = nearest(atlas, view, square, at).and_then(|l| l.id.clone());
}

#[cfg(test)]
mod tests {
    use super::{
        Floor, Label, Mode, Place, Square, View, click, follow, grab, plantable, reach, turn_for,
    };
    use crate::settings::MapTurn;
    use egui::{Color32, Rect, pos2, vec2};
    use overseer_core::Plan;
    use overseer_core::{Atlas, Lineup};

    /// The olive the game tints plantable ground in is picked, and the grey
    /// of the floor, the white of the walls and a clear pixel are not.
    #[test]
    fn only_the_plantable_olive_is_picked() {
        assert!(plantable(Color32::from_rgb(152, 152, 118)));
        assert!(plantable(Color32::from_rgb(153, 153, 119)));
        for other in [
            Color32::from_rgb(118, 118, 118),
            Color32::WHITE,
            Color32::TRANSPARENT,
        ] {
            assert!(!plantable(other), "{other:?}");
        }
    }

    /// Browsing, a click on a pin picks that lineup and a click anywhere else
    /// picks nothing, without starting one. Once Add a Lineup has opened one,
    /// a click says where it's thrown from and the next where it lands.
    #[test]
    fn browsing_only_views_and_adding_places() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)));
        let atlas = Atlas {
            lineups: vec![Lineup {
                id: Some("b-1".to_owned()),
                map: "Ascent".to_owned(),
                stand: Some([0.9, 0.9]),
                ..Lineup::default()
            }],
            ..Atlas::default()
        };
        let mut view = View {
            map: Some("Ascent".to_owned()),
            you: Some("Brimstone".to_owned()),
            ..View::default()
        };
        click(&atlas, &mut view, square, pos2(90.0, 91.0));
        assert_eq!(view.selected.as_deref(), Some("b-1"));
        assert!(matches!(view.mode, Mode::Browse));

        click(&atlas, &mut view, square, pos2(20.0, 30.0));
        assert!(matches!(view.mode, Mode::Browse), "browsing only views");
        assert_eq!(view.selected, None);

        view.mode = Mode::Edit(Box::new(view.draft()));
        click(&atlas, &mut view, square, pos2(20.0, 30.0));
        let Mode::Edit(draft) = &view.mode else {
            panic!("the lineup should be open");
        };
        assert_eq!(draft.lineup.agent, "Brimstone");
        assert_eq!(draft.lineup.stand, Some([0.2, 0.3]));
        assert_eq!(draft.placing, Place::Land);

        click(&atlas, &mut view, square, pos2(50.0, 10.0));
        let Mode::Edit(draft) = &view.mode else {
            panic!("the lineup should still be open");
        };
        assert_eq!(draft.lineup.land, Some([0.5, 0.1]));
    }

    /// Browsing, a drag starts nothing. Adding one, a drag from an empty spot
    /// is the whole lineup, from where you stand to where it lands, and a
    /// drag on either of its pins moves just that pin.
    #[test]
    fn a_drag_places_a_lineup_and_moves_its_pins() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)));
        let mut view = View {
            map: Some("Ascent".to_owned()),
            ..View::default()
        };
        grab(&Atlas::default(), &mut view, square, pos2(20.0, 30.0));
        assert!(matches!(view.mode, Mode::Browse), "browsing only views");

        view.mode = Mode::Edit(Box::new(view.draft()));
        grab(&Atlas::default(), &mut view, square, pos2(20.0, 30.0));
        follow(
            &Atlas::default(),
            &mut view,
            square,
            pos2(40.0, 40.0),
            false,
        );
        follow(&Atlas::default(), &mut view, square, pos2(60.0, 10.0), true);
        let Mode::Edit(draft) = &view.mode else {
            panic!("the lineup should still be open");
        };
        assert_eq!(draft.lineup.stand, Some([0.2, 0.3]));
        assert_eq!(draft.lineup.land, Some([0.6, 0.1]));
        assert_eq!((draft.grab, draft.placing), (None, Place::Land));

        grab(&Atlas::default(), &mut view, square, pos2(22.0, 31.0));
        follow(&Atlas::default(), &mut view, square, pos2(25.0, 50.0), true);
        let Mode::Edit(draft) = &view.mode else {
            panic!("the lineup should still be open");
        };
        assert_eq!(draft.lineup.stand, Some([0.25, 0.5]), "the stand pin moved");
        assert_eq!(draft.lineup.land, Some([0.6, 0.1]), "and only that pin");

        grab(&Atlas::default(), &mut view, square, pos2(70.0, 70.0));
        follow(&Atlas::default(), &mut view, square, pos2(71.0, 70.0), true);
        let Mode::Edit(draft) = &view.mode else {
            panic!("the lineup should still be open");
        };
        assert_eq!(draft.lineup.stand, Some([0.7, 0.7]));
        assert_eq!(draft.lineup.land, None, "a press held still isn't a throw");
    }

    /// Ascent's attackers start on the right of Riot's minimap. Turned for
    /// them they end up at the bottom, turned for the defenders it's those,
    /// and as drawn nothing turns.
    #[test]
    fn the_chosen_side_ends_up_at_the_bottom() {
        let ascent = Plan {
            attack: Some([0.82, 0.57]),
            defend: Some([0.13, 0.43]),
            ..Plan::default()
        };
        let low = |turn_to: MapTurn, spawn: [f32; 2]| {
            super::shapes::turned(spawn, turn_for(&ascent, turn_to))[1]
        };
        assert!(low(MapTurn::Attack, [0.82, 0.57]) > 0.75);
        assert!(low(MapTurn::Defend, [0.13, 0.43]) > 0.75);
        assert_eq!(turn_for(&ascent, MapTurn::Drawn), 0);
        assert_eq!(
            turn_for(&Plan::default(), MapTurn::Attack),
            0,
            "no spawns, no turn"
        );
    }

    /// The mollies reach 4.5 metres, KAY/O's and Gekko's have their full
    /// damage circle inside, and a smoke has no area here.
    #[test]
    fn a_molly_reaches_as_far_as_the_game_says() {
        assert_eq!(reach("Incendiary"), Some((450.0, None)));
        assert_eq!(reach("FRAG/ment"), Some((400.0, Some(100.0))));
        assert_eq!(reach("Sky Smoke"), None);
    }

    /// Where you stand snaps onto where another lineup for the same agent is
    /// thrown from, by a click or by the drag that starts a lineup, and a
    /// different agent's spot stays apart.
    #[test]
    fn a_standing_spot_snaps_only_onto_the_same_agent() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1000.0)));
        let atlas = Atlas {
            maps: vec![Plan {
                name: "Ascent".to_owned(),
                scale: Some(0.0001),
                ..Plan::default()
            }],
            lineups: vec![Lineup {
                id: Some("b-1".to_owned()),
                map: "Ascent".to_owned(),
                agent: "Brimstone".to_owned(),
                stand: Some([0.2, 0.2]),
                ..Lineup::default()
            }],
            ..Atlas::default()
        };
        let stand = |agent: &str, how: &str| {
            let mut view = View {
                map: Some("Ascent".to_owned()),
                you: Some(agent.to_owned()),
                ..View::default()
            };
            view.mode = Mode::Edit(Box::new(view.draft()));
            if how == "click" {
                click(&atlas, &mut view, square, pos2(204.0, 197.0));
            } else {
                grab(&atlas, &mut view, square, pos2(204.0, 197.0));
            }
            match &view.mode {
                Mode::Edit(draft) => draft.lineup.stand,
                Mode::Browse | Mode::Draw(_) => None,
            }
        };
        assert_eq!(stand("Brimstone", "click"), Some([0.2, 0.2]));
        assert_eq!(stand("Brimstone", "drag"), Some([0.2, 0.2]));
        assert_eq!(
            stand("Viper", "click"),
            Some([0.204, 0.197]),
            "another agent's"
        );
    }

    /// A landing spot a few centimetres from another lineup's lands on
    /// exactly that spot, and one a couple of metres away stays where it was
    /// put. A lineup being edited doesn't snap onto where it already lands.
    #[test]
    fn a_landing_spot_snaps_onto_one_a_metre_away() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1000.0)));
        let atlas = Atlas {
            // A thousandth of the map is ten game units here.
            maps: vec![Plan {
                name: "Ascent".to_owned(),
                scale: Some(0.0001),
                ..Plan::default()
            }],
            lineups: vec![Lineup {
                id: Some("b-1".to_owned()),
                map: "Ascent".to_owned(),
                land: Some([0.5, 0.5]),
                ..Lineup::default()
            }],
            ..Atlas::default()
        };
        let mut view = View {
            map: Some("Ascent".to_owned()),
            ..View::default()
        };
        view.mode = Mode::Edit(Box::new(view.draft()));
        click(&atlas, &mut view, square, pos2(100.0, 100.0));
        click(&atlas, &mut view, square, pos2(505.0, 497.0));
        let land = |view: &View| match &view.mode {
            Mode::Edit(draft) => draft.lineup.land,
            Mode::Browse | Mode::Draw(_) => None,
        };
        assert_eq!(land(&view), Some([0.5, 0.5]), "half a metre off snaps");

        click(&atlas, &mut view, square, pos2(520.0, 500.0));
        assert_eq!(land(&view), Some([0.52, 0.5]), "two metres off stays put");

        grab(&Atlas::default(), &mut view, square, pos2(520.0, 500.0));
        follow(&atlas, &mut view, square, pos2(503.0, 500.0), true);
        assert_eq!(land(&view), Some([0.5, 0.5]), "a drag snaps as well");

        if let Mode::Edit(draft) = &mut view.mode {
            draft.lineup.id = Some("b-1".to_owned());
        }
        click(&atlas, &mut view, square, pos2(505.0, 500.0));
        assert_eq!(land(&view), Some([0.505, 0.5]), "not onto itself");
    }

    /// A name whose spot is half off the floor moves fully onto it, and a
    /// second name on the same spot goes beside the first, not over it.
    #[test]
    fn a_name_moves_onto_the_floor_and_off_another() {
        // Floor is the middle of the map, cells 64 to 192 each way.
        let size = 256;
        let pixels = (0..size)
            .flat_map(|y| (0..size).map(move |x| (x, y)))
            .map(|(x, y)| {
                if (64..192).contains(&x) && (64..192).contains(&y) {
                    Color32::WHITE
                } else {
                    Color32::TRANSPARENT
                }
            })
            .collect();
        let image = egui::ColorImage::new([size, size], pixels);
        let floor = Floor::new(Some(&image), 0);
        let label = |x: f32| Label {
            at: [x, 100.0],
            size: [30.0, 6.0],
            letter: false,
        };
        let placed = floor.place(&[label(64.0), label(64.0)]);
        let inside = |[x, y]: [f32; 2]| {
            x - 15.0 >= 64.0 && x + 15.0 <= 192.0 && y - 3.0 >= 64.0 && y + 3.0 <= 192.0
        };
        assert!(placed.iter().all(|&p| inside(p)), "{placed:?}");
        let [one, two] = placed.as_slice() else {
            panic!("two names go in and two come out, not {placed:?}");
        };
        let apart = (one[0] - two[0]).abs() >= 30.0 || (one[1] - two[1]).abs() >= 6.0;
        assert!(apart, "{placed:?}");
    }
}
