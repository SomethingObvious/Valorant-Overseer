//! The map: its minimap with each site's plantable ground lit, the places the
//! game names, the shapes drawn on it, and a pin for where each lineup is
//! thrown from and where it lands. Browsing only looks: a click picks a
//! lineup and resting on one shows its notes. Once Add a Lineup is pressed, a
//! drag from where you stand to where it lands places both at once. While
//! drawing, a drag draws a shape or moves one by its dot, and a click places
//! a label. An export asks the window for a picture of it.

use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};
use overseer_core::{Atlas, Lineup, Plan};
use overseer_ui::{Face, art, caps_text, caps_width, colour, motion, size, space};

use super::areas::Reach;
use super::shapes::{self, KINDS, Square};
use super::{Mode, Place, SPIKE, View, ability, areas, face, spike};
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
    pins(&painter, (square, ui.clip_rect()), atlas, view, plan);
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
        if matches!(view.mode, Mode::Browse) {
            choose(ui, atlas, view);
        }
        // Every lineup under the pointer, one under another, when several
        // share a spot.
        let under_pointer = response
            .hover_pos()
            .map(|at| under(atlas, view, square, at));
        if matches!(view.mode, Mode::Browse)
            && view.choosing.is_none()
            && let Some(found) = under_pointer.filter(|f| !f.is_empty())
        {
            let _shown = response.clone().on_hover_ui_at_pointer(|ui| {
                for (index, lineup) in found.iter().enumerate() {
                    if index > 0 {
                        ui.add_space(space::SM);
                        ui.separator();
                    }
                    note(ui, atlas, lineup);
                }
            });
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

/// How near plantable ground the Spike has to be put to move onto it, in game
/// units. Further off it stays where it is put.
const STICKY: f32 = 200.0;

/// `at`, on the minimap `image`, moved onto the nearest plantable pixel
/// within `reach` map widths when it is just off the plantable ground, or
/// left where it is.
fn stick(image: &egui::ColorImage, at: [f32; 2], reach: f32) -> [f32; 2] {
    let [w, h] = image.size;
    let on = |x: i64, y: i64| {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return false;
        };
        x < w && y < h && image.pixels.get(y * w + x).is_some_and(|p| plantable(*p))
    };
    let (x, y) = ((at[0] * w as f32) as i64, (at[1] * h as f32) as i64);
    if on(x, y) {
        return at;
    }
    let far = (reach * w as f32).ceil() as i64;
    let mut best: Option<(i64, i64, i64)> = None;
    for dy in -far..=far {
        for dx in -far..=far {
            let gap = dx * dx + dy * dy;
            if gap <= far * far && best.is_none_or(|b| gap < b.0) && on(x + dx, y + dy) {
                best = Some((gap, dx, dy));
            }
        }
    }
    best.map_or(at, |(_, dx, dy)| {
        [
            ((x + dx) as f32 + 0.5) / w as f32,
            ((y + dy) as f32 + 0.5) / h as f32,
        ]
    })
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
    let floor = Floor::new(image.as_deref(), square.turn, false);
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

/// The floor of the map on screen, worked out once for each map and turn
/// once its minimap is on disk. All floor until then.
fn floor_of(ctx: &egui::Context, square: Square, plan: &Plan) -> std::sync::Arc<Floor> {
    let id = egui::Id::new(("walled-floor", &plan.name, square.turn));
    if let Some(kept) = ctx.data(|store| store.get_temp::<std::sync::Arc<Floor>>(id)) {
        return kept;
    }
    let image = plan
        .minimap
        .as_deref()
        .and_then(|path| art::file_pixels(ctx, path));
    let floor = std::sync::Arc::new(Floor::new(image.as_deref(), square.turn, true));
    if image.is_some() {
        ctx.data_mut(|store| store.insert_temp(id, std::sync::Arc::clone(&floor)));
    }
    floor
}

/// How far a turret at `from` sees along `way` on `square`, up to `most`
/// points: to the first clear pixel of the minimap, which is off its floor,
/// or off the map. Only that is sure to block it, as the lines drawn inside
/// the floor are as often ledges and the edges of raised ground as walls.
/// The edge is found to a sixteenth of a point, so the cone's rim runs smooth.
fn sight(
    image: &egui::ColorImage,
    square: Square,
    (from, way): (Pos2, Vec2),
    most: f32,
) -> Option<f32> {
    let [w, h] = image.size;
    let clear = |t: f32| {
        let shown = (from + way * t - square.min) / square.width();
        if !(0.0..1.0).contains(&shown.x) || !(0.0..1.0).contains(&shown.y) {
            return true;
        }
        let [x, y] = shapes::turned([shown.x, shown.y], 4 - square.turn % 4);
        let px = ((x * w as f32) as usize).min(w.saturating_sub(1));
        let py = ((y * h as f32) as usize).min(h.saturating_sub(1));
        image.pixels.get(py * w + px).is_none_or(|p| p.a() <= 128)
    };
    let steps = most.min(f32::from(u16::MAX)) as u16;
    let hit = (1..=steps).map(f32::from).find(|&t| clear(t))?;
    let (mut open, mut shut) = (hit - 1.0, hit);
    for _ in 0..4 {
        let middle = f32::midpoint(open, shut);
        if clear(middle) {
            shut = middle;
        } else {
            open = middle;
        }
    }
    Some(open)
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
    /// the minimap isn't on disk. With `lines`, the white lines Riot draws
    /// for walls inside the floor are wall too. They are a pixel or two wide,
    /// so then a cell is only floor when every pixel across it is.
    fn new(image: Option<&egui::ColorImage>, turn: u8, lines: bool) -> Self {
        let row = usize::from(CELLS) + 1;
        let cells = f32::from(CELLS);
        let across: &[f32] = if lines {
            &[0.125, 0.375, 0.625, 0.875]
        } else {
            &[0.5]
        };
        let floor = |image: &egui::ColorImage, shown: [f32; 2]| {
            let [w, h] = image.size;
            let [x, y] = shapes::turned(shown, 4 - turn % 4);
            let px = ((x * w as f32) as usize).min(w.saturating_sub(1));
            let py = ((y * h as f32) as usize).min(h.saturating_sub(1));
            // The floor is mid grey, the sites yellow and the wall lines
            // white, so only the lines are this bright in red and green.
            image
                .pixels
                .get(py * w + px)
                .is_some_and(|p| p.a() > 128 && !(lines && p.r() > 190 && p.g() > 190))
        };
        let on = |gx: u16, gy: u16| {
            image.is_none_or(|image| {
                across.iter().all(|dy| {
                    across.iter().all(|dx| {
                        floor(
                            image,
                            [(f32::from(gx) + dx) / cells, (f32::from(gy) + dy) / cells],
                        )
                    })
                })
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

    /// Whether the cell at `x`, `y` is wall, or off the map.
    fn wall(&self, x: i32, y: i32) -> bool {
        self.count([x, y, x + 1, y + 1]) == 0
    }

    /// How many points the floor runs from `from` along `way` on `square`
    /// before a wall, when one comes within `most`. A point on a wall's edge
    /// starts from the floor beside it, up to three cells out, since a trip
    /// or a mesh is put against a wall. One deeper in the wall is at it.
    fn run(&self, square: Square, (from, way): (Pos2, Vec2), most: f32) -> Option<f32> {
        let cell = square.width() / f32::from(CELLS);
        let wall = |t: f32| {
            let at = (from + way * t - square.min) / cell;
            self.wall(at.x.floor() as i32, at.y.floor() as i32)
        };
        let step = cell / 2.0;
        let Some(start) = (0..=6_u8).map(|i| f32::from(i) * step).find(|&t| !wall(t)) else {
            return Some(0.0);
        };
        std::iter::successors(Some(start), |t| Some(t + step))
            .take_while(|&t| t <= most)
            .find(|&t| wall(t))
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
        Mode::Edit(draft) => {
            let reach = draft.lineup.ability.as_deref().and_then(areas::reach);
            match (draft.placing, draft.lineup.land.is_some(), reach) {
                _ if draft.lineup.agent == SPIKE => "Click where the Spike is planted",
                (Place::Stand, ..) => "Click or drag from where you stand",
                (Place::Land, false, _) => "Now click where it lands",
                (Place::More, ..) => "Click to put down another, or click one to take it off",
                (_, true, Some(Reach::Wire(_))) => "Drag the small dots to set the trip's two ends",
                (_, true, Some(Reach::Bent(_))) => {
                    "Drag the wall to bend it, or its end to make it longer"
                }
                (_, true, Some(Reach::Aim(..))) => "Drag the small dot to turn the turret",
                (_, true, Some(Reach::Bounce(_))) => "Drag the small dot to where it bounces",
                _ => "Drag either pin to move it",
            }
        }
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
fn pins(
    painter: &egui::Painter,
    (square, room): (Square, Rect),
    atlas: &Atlas,
    view: &View,
    plan: &Plan,
) {
    let (scale, floor) = (plan.scale, floor_of(painter.ctx(), square, plan));
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
    let run = |from: Pos2, way: Vec2, most: f32| floor.run(square, (from, way), most);
    let image = plan
        .minimap
        .as_deref()
        .and_then(|path| art::file_pixels(painter.ctx(), path));
    let see = |from: Pos2, way: Vec2, most: f32| {
        image
            .as_deref()
            .and_then(|image| sight(image, square, (from, way), most))
    };
    // Areas stay on the map, since Cosmic Divide has no end and a big one
    // like Haunt's would cover the panel beside it.
    let inside = painter.with_clip_rect(square.rect.intersect(painter.clip_rect()));
    let areas =
        |lineup: &Lineup, ink: Color32| area(&inside, square, lineup, scale, (ink, &run, &see));
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
    // Names keep off every face and ring, and may run off the map as
    // far as the screen goes. The picked one's name goes first, so the rest
    // move out of its way.
    let shown: Vec<&Lineup> = rest.iter().chain(&picked).copied().chain(writing).collect();
    let mut taken = covered(square, &shown);
    let painter = &painter.with_clip_rect(room);
    for lineup in &picked {
        pin(
            painter,
            square,
            atlas,
            lineup,
            (tint(lineup.side.as_deref()), main),
        );
        label(
            painter,
            (square, room),
            lineup,
            (&mut taken, colour::TEXT_STRONG),
        );
    }
    if view.naming() {
        for lineup in &rest {
            label(
                painter,
                (square, room),
                lineup,
                (&mut taken, colour::TEXT_DIM),
            );
        }
    }
    if let Some(lineup) = writing {
        pin(painter, square, atlas, lineup, (colour::TEXT_STRONG, main));
        handles(painter, square, lineup, scale);
    }
}

/// What only shows while a lineup is being written: how far a smoke put
/// down from afar reaches from where you stand, a trip's two ends to drag,
/// and the point a wall bends through.
fn handles(painter: &egui::Painter, square: Square, lineup: &Lineup, scale: Option<f32>) {
    let units = scale.map(|s| s * square.width());
    let stand = lineup.stand.map(|p| point(square, p));
    if let Some((_, Some(range))) = areas::spots(lineup.ability.as_deref())
        && let (Some(from), Some(units)) = (stand, units)
    {
        painter.circle_stroke(from, range * units, Stroke::new(1.0, colour::TEXT_FAINT));
    }
    let dot = |at: Pos2| {
        painter.circle_filled(at, DOT, colour::TEXT_STRONG);
        painter.circle_stroke(at, DOT, Stroke::new(1.5, colour::VOID));
    };
    match lineup.ability.as_deref().and_then(areas::reach) {
        Some(Reach::Wire(_) | Reach::Aim(..) | Reach::Bounce(_)) => {
            lineup.points.iter().for_each(|p| dot(point(square, *p)));
        }
        Some(Reach::Bent(length)) => {
            if let (Some(from), Some(to), Some(units)) =
                (stand, lineup.land.map(|p| point(square, p)), units)
            {
                let bend = lineup.points.first().map(|p| point(square, *p));
                let path = areas::bent(from, bend, to, length * units);
                let middle = path.get(path.len() >> 1).copied();
                if let Some(at) = bend.or(middle) {
                    dot(at);
                }
            }
        }
        _ => {}
    }
}

/// The ground a lineup's ability covers, to the map's scale, with trips and
/// walls stopping at the map's walls. It moves with the pins, since it is
/// drawn from them.
fn area(
    painter: &egui::Painter,
    square: Square,
    lineup: &Lineup,
    scale: Option<f32>,
    (ink, run, see): (Color32, areas::Run<'_>, areas::Run<'_>),
) {
    let reach = if lineup.agent == SPIKE {
        Some(areas::DEFUSE)
    } else {
        lineup.ability.as_deref().and_then(areas::reach)
    };
    let (Some(land), Some(reach), Some(scale)) = (lineup.land, reach, scale) else {
        return;
    };
    let stand = lineup.stand.map(|s| point(square, s));
    let points: Vec<Pos2> = lineup.points.iter().map(|p| point(square, *p)).collect();
    let afar = areas::spots(lineup.ability.as_deref()).is_some();
    // Put down from afar, each point is another place it lands. Otherwise
    // they are the ability's own, a trip's ends or a wall's bend.
    let (lands, own): (Vec<Pos2>, &[Pos2]) = if afar {
        (
            std::iter::once(point(square, land))
                .chain(points.iter().copied())
                .collect(),
            &[],
        )
    } else {
        (vec![point(square, land)], &points)
    };
    for at in lands {
        areas::draw(
            painter,
            (stand, at, own),
            (reach, scale * square.width()),
            (
                ink,
                if matches!(reach, Reach::Aim(_)) {
                    see
                } else {
                    run
                },
            ),
        );
    }
}

/// The other places a lineup put down from afar lands, which are its points.
fn more_lands(lineup: &Lineup) -> &[[f32; 2]] {
    if areas::spots(lineup.ability.as_deref()).is_some() {
        &lineup.points
    } else {
        &[]
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
    for more in more_lands(lineup) {
        land_mark(painter, point(square, *more), RING, atlas, lineup, ink);
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
    // The Spike has no ring, as its defuse range is barely wider than one.
    if lineup.agent == SPIKE {
        spike(painter, at, radius * 0.75, ink);
        return;
    }
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
    (square, room): (Square, Rect),
    lineup: &Lineup,
    (taken, ink): (&mut Vec<Rect>, Color32),
) {
    let Some(at) = lineup.land.or(lineup.stand).map(|p| point(square, p)) else {
        return;
    };
    let font = Face::Display.at(size::LABEL);
    let wide = space::MD.mul_add(2.0, caps_width(painter, &lineup.title, font.clone())) + BAR;
    let plate = clear_of(
        taken,
        Rect::from_center_size(at - vec2(0.0, RING + 14.0), vec2(wide, 20.0)),
        room,
    );
    taken.push(plate);
    // Outlined and barred in its side's colour, so a lineup's name reads
    // apart from the map's own place names on their plain dark plates.
    let side = tint(lineup.side.as_deref());
    painter.rect(
        plate,
        0,
        colour::VOID.gamma_multiply(0.95),
        Stroke::new(1.0, side.gamma_multiply(0.8)),
        egui::StrokeKind::Inside,
    );
    let bar = Rect::from_min_size(plate.min, vec2(BAR, plate.height()));
    painter.rect_filled(bar, 0, side);
    let _title = caps_text(
        painter,
        pos2(plate.center().x + BAR / 2.0, plate.center().y),
        Align2::CENTER_CENTER,
        &lineup.title,
        font,
        ink,
    );
}

/// What a name must keep off: every lineup's face where it is thrown from
/// and its ring where it lands. The ground a molly covers is fine to sit on.
fn covered(square: Square, lineups: &[&Lineup]) -> Vec<Rect> {
    let mut out = Vec::new();
    for lineup in lineups {
        if let Some(stand) = lineup.stand {
            out.push(Rect::from_center_size(
                point(square, stand),
                Vec2::splat(FACE * 2.0),
            ));
        }
        let Some(land) = lineup.land else {
            continue;
        };
        out.push(Rect::from_center_size(
            point(square, land),
            Vec2::splat(RING * 2.0),
        ));
    }
    out
}

/// How wide the bar in its side's colour is down a lineup name's left edge.
const BAR: f32 = 3.0;

/// The space kept between two names on the map, in points.
const APART: f32 = 4.0;

/// `plate` kept inside `bounds`, the screen it is drawn on, and moved to the
/// nearest place clear of everything in `taken` by `APART`: names placed
/// before it, and the faces and rings it mustn't cover. That can be above,
/// below or out to either side, whichever is nearest, so lineups on one spot
/// stack their names. When nothing near is clear it takes the spot covering
/// the least.
fn clear_of(taken: &[Rect], plate: Rect, bounds: Rect) -> Rect {
    let inside = |r: Rect| {
        let dx = (bounds.left() - r.left()).max(0.0) + (bounds.right() - r.right()).min(0.0);
        let dy = (bounds.top() - r.top()).max(0.0) + (bounds.bottom() - r.bottom()).min(0.0);
        r.translate(vec2(dx, dy))
    };
    let overlap = |r: Rect| -> f32 {
        taken
            .iter()
            .map(|t| t.expand(APART).intersect(r))
            .filter(Rect::is_positive)
            .map(|i| i.area())
            .sum()
    };
    // Rows of the name's height, and steps of a quarter of its width out to a
    // width and a half either side, tried nearest first. Below costs a little
    // more than above, so a stack still grows upwards.
    let (up, across) = (plate.height() + APART, (plate.width() + APART) / 4.0);
    let mut shifts: Vec<Vec2> = (-5_i8..=5)
        .flat_map(|row| {
            (-6_i8..=6).map(move |step| vec2(f32::from(step) * across, f32::from(row) * up))
        })
        .collect();
    let cost = |v: &Vec2| v.x.hypot(if v.y > 0.0 { v.y * 1.15 } else { v.y });
    shifts.sort_by(|a, b| cost(a).total_cmp(&cost(b)));
    let mut best = (f32::INFINITY, inside(plate));
    for shift in shifts {
        let moved = inside(plate.translate(shift));
        let covered = overlap(moved);
        if covered <= 0.0 {
            return moved;
        }
        if covered < best.0 {
            best = (covered, moved);
        }
    }
    best.1
}

/// A drag on the map while a lineup is being written: from where you stand
/// to where it lands, or one of its pins picked up and moved.
fn drag(ui: &Ui, atlas: &Atlas, view: &mut View, square: Square, response: &Response) {
    if let Mode::Edit(draft) = &mut view.mode {
        let held = match draft.grab {
            Some(Place::Point(index)) => Some(index),
            _ => None,
        };
        let scale = scale_of(atlas, &draft.lineup.map);
        settle(&mut draft.lineup, scale, held);
        // The Spike is only where it's planted, so a click always plants it.
        if draft.lineup.agent == SPIKE {
            draft.lineup.stand = None;
            draft.lineup.ability = None;
            draft.lineup.points.clear();
            if draft.placing == Place::Stand {
                draft.placing = Place::Land;
            }
            let image = atlas
                .maps
                .iter()
                .find(|p| p.name == draft.lineup.map)
                .and_then(|p| p.minimap.as_deref())
                .and_then(|path| art::file_pixels(ui.ctx(), path));
            if let (Some(image), Some(land), Some(scale)) = (image, draft.lineup.land, scale) {
                draft.lineup.land = Some(stick(&image, land, STICKY * scale));
            }
        }
        // Another ability can't put down more, so a click places it again.
        if draft.placing == Place::More && areas::spots(draft.lineup.ability.as_deref()).is_none() {
            draft.placing = Place::Land;
        }
    }
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
    // A trip's ends sit close to where it lands, so they come first.
    let own = draft
        .lineup
        .points
        .iter()
        .position(|p| point(square, *p).distance(origin) <= REACH);
    let scale = scale_of(atlas, &draft.lineup.map);
    draft.grab = Some(if let Some(index) = own {
        Place::Point(index)
    } else if on(draft.lineup.land) {
        Place::Land
    } else if on(draft.lineup.stand) {
        Place::Stand
    } else if draft.lineup.agent == SPIKE {
        draft.lineup.land = Some(shapes::spot(square, origin));
        Place::Land
    } else if on_wall(&draft.lineup, square, scale, origin) {
        draft.lineup.points = vec![shapes::spot(square, origin)];
        Place::Point(0)
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
        Place::Point(index) => {
            if let (Some(at), Some(spot)) = (draft.lineup.points.get_mut(index), spot) {
                *at = spot;
            }
        }
        Place::More => {}
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

/// The game's scale on `map`, map widths to a game unit, once it is known.
fn scale_of(atlas: &Atlas, map: &str) -> Option<f32> {
    atlas
        .maps
        .iter()
        .find(|p| p.name == map)
        .and_then(|p| p.scale)
}

/// Whether `at` is on the wall a lineup for a bendable wall draws.
fn on_wall(lineup: &Lineup, square: Square, scale: Option<f32>, at: Pos2) -> bool {
    let Some(Reach::Bent(length)) = lineup.ability.as_deref().and_then(areas::reach) else {
        return false;
    };
    let (Some(from), Some(to), Some(scale)) = (lineup.stand, lineup.land, scale) else {
        return false;
    };
    let bend = lineup.points.first().map(|p| point(square, *p));
    areas::bent(
        point(square, from),
        bend,
        point(square, to),
        length * scale * square.width(),
    )
    .iter()
    .any(|p| p.distance(at) <= REACH)
}

/// How far a trip's two dots sit from it, in game units.
const TRIP_DOT: f32 = 300.0;

/// Keeps a lineup's points to what its ability allows, at `scale` map
/// widths to a game unit. Smokes put down from afar stay within reach of
/// where you stand, and no more of them than it has. A trip gets two dots
/// `TRIP_DOT` either side of it, across the throw at first, and turns to
/// follow the one `held`. A bent wall ends at the most it can be. Any other ability has none.
fn settle(lineup: &mut Lineup, scale: Option<f32>, held: Option<usize>) {
    let ability = lineup.ability.clone();
    let reach = ability.as_deref().and_then(areas::reach);
    if let Some((most, range)) = areas::spots(ability.as_deref()) {
        lineup.points.truncate(most.saturating_sub(1));
        if let (Some(stand), Some(range), Some(scale)) = (lineup.stand, range, scale) {
            let within = |p: [f32; 2]| within(stand, p, range * scale);
            lineup.land = lineup.land.map(within);
            lineup.points.iter_mut().for_each(|p| *p = within(*p));
        }
        return;
    }
    let Some(scale) = scale else {
        return;
    };
    match reach {
        Some(Reach::Wire(_)) => {
            let Some(land) = lineup.land.map(|l| pos2(l[0], l[1])) else {
                lineup.points.clear();
                return;
            };
            let at = |p: [f32; 2]| pos2(p[0], p[1]);
            // The dot held sets which way it runs, and the other mirrors it
            // through the trip. Otherwise it keeps its way as the trip moves,
            // and before it has one, it runs across the throw.
            let way = match (lineup.points.as_slice(), held) {
                (&[a, _], Some(0)) => land - at(a),
                (&[_, b], Some(1)) => at(b) - land,
                (&[a, b], _) => at(b) - at(a),
                _ => lineup.stand.map_or(Vec2::Y, |s| (land - at(s)).rot90()),
            };
            let way = if way.length() > 0.0 {
                way.normalized()
            } else {
                Vec2::Y
            };
            let reach = way * (TRIP_DOT * scale);
            lineup.points = [land - reach, land + reach].map(|p| [p.x, p.y]).to_vec();
        }
        Some(Reach::Bent(length)) => {
            lineup.points.truncate(1);
            // Moving the end changes the curve, so it takes a few goes to
            // land on the most it can be.
            for _ in 0..4 {
                let (Some(stand), Some(land)) = (lineup.stand, lineup.land) else {
                    break;
                };
                let bend = lineup.points.first().map(|p| pos2(p[0], p[1]));
                let path = areas::bent(
                    pos2(stand[0], stand[1]),
                    bend,
                    pos2(land[0], land[1]),
                    length * scale,
                );
                if let Some(end) = path.last() {
                    lineup.land = Some([end.x, end.y]);
                }
            }
        }
        Some(Reach::Aim(..) | Reach::Bounce(_)) => {
            lineup.points.truncate(1);
            if let (true, Some(land)) = (lineup.points.is_empty(), lineup.land) {
                let at = pos2(land[0], land[1]);
                let from = lineup.stand.map_or(at - Vec2::X, |s| pos2(s[0], s[1]));
                // A turret points on the way it was thrown, ten metres out,
                // and a bolt bounces halfway.
                let start = if matches!(reach, Some(Reach::Aim(..))) {
                    at + (at - from).normalized() * (1000.0 * scale)
                } else {
                    from.lerp(at, 0.5)
                };
                lineup.points = vec![[start.x, start.y]];
            }
        }
        _ => lineup.points.clear(),
    }
}

/// `p` moved toward `from` until it is no further than `most`.
fn within(from: [f32; 2], p: [f32; 2], most: f32) -> [f32; 2] {
    let way = pos2(p[0], p[1]) - pos2(from[0], from[1]);
    if way.length() <= most {
        return p;
    }
    let at = pos2(from[0], from[1]) + way.normalized() * most;
    [at.x, at.y]
}

/// `spot` moved onto the nearest pin of the same kind on another lineup on
/// the same map within `SAME_SPOT`: any lineup's landing spot, so two lineups
/// for one spot share one ring, or where a lineup for the same agent is
/// thrown from, so one corner is one agent pin.
fn snapped(atlas: &Atlas, lineup: &Lineup, spot: [f32; 2], place: Place) -> [f32; 2] {
    let Some(scale) = scale_of(atlas, &lineup.map) else {
        return spot;
    };
    atlas
        .lineups
        .iter()
        .filter(|l| l.map == lineup.map && (lineup.id.is_none() || l.id != lineup.id))
        .filter_map(|l| match place {
            Place::Land | Place::More | Place::Point(_) => l.land,
            Place::Stand => l
                .stand
                .filter(|_| l.agent.eq_ignore_ascii_case(&lineup.agent)),
        })
        .map(|p| ((p[0] - spot[0]).hypot(p[1] - spot[1]), p))
        .filter(|(far, _)| *far <= SAME_SPOT * scale)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(spot, |(_, p)| p)
}

/// Every lineup on the map with a pin in reach of `at`, nearest first. More
/// than one when lineups are stacked on one spot.
fn under<'a>(atlas: &'a Atlas, view: &View, square: Square, at: Pos2) -> Vec<&'a Lineup> {
    let mut found: Vec<(f32, &Lineup)> = view
        .shown(atlas)
        .into_iter()
        .filter_map(|l| {
            let reach = [l.stand, l.land]
                .into_iter()
                .flatten()
                .chain(more_lands(l).iter().copied())
                .map(|p| point(square, p).distance(at))
                .fold(f32::INFINITY, f32::min);
            (reach <= REACH).then_some((reach, l))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, l)| l).collect()
}

/// The list a click on stacked lineups opens, beside the click: one row
/// each, and picking one shows it. A click anywhere else or Escape closes it.
fn choose(ui: &Ui, atlas: &Atlas, view: &mut View) {
    let Some((at, ids, opened)) = view.choosing.clone() else {
        return;
    };
    let pass = ui.ctx().cumulative_pass_nr();
    if opened == 0
        && let Some(choice) = &mut view.choosing
    {
        choice.2 = pass;
    }
    let main = view.main.clone();
    let mut picked = None;
    let shown = egui::Area::new(egui::Id::new("lineup-stack"))
        .order(egui::Order::Foreground)
        .fixed_pos(at + vec2(12.0, 12.0))
        .constrain(true)
        .show(ui.ctx(), |ui| {
            controls::menu_card().show(ui, |ui| {
                ui.set_width(300.0);
                let _heading = caps_text(
                    ui.painter(),
                    ui.cursor().min + vec2(space::SM, space::SM),
                    Align2::LEFT_TOP,
                    "Lineups on this spot",
                    Face::Display.at(size::MICRO),
                    colour::TEXT_DIM,
                );
                ui.add_space(space::XL);
                for id in &ids {
                    let Some(lineup) = atlas.lineups.iter().find(|l| l.id.as_ref() == Some(id))
                    else {
                        continue;
                    };
                    let on = view.selected.as_ref() == Some(id);
                    if super::side::row(ui, atlas, lineup, (on, main.as_deref())).clicked() {
                        picked = Some(id.clone());
                    }
                }
            });
        });
    let away = opened != 0 && opened != pass && shown.response.clicked_elsewhere();
    if let Some(id) = picked {
        view.selected = Some(id);
        view.choosing = None;
    } else if away || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        view.choosing = None;
    }
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
            Place::More => {
                let most = areas::spots(draft.lineup.ability.as_deref()).map_or(1, |s| s.0);
                let points = &mut draft.lineup.points;
                if let Some(index) = points
                    .iter()
                    .position(|p| point(square, *p).distance(at) <= REACH)
                {
                    points.remove(index);
                } else if points.len() + 1 < most {
                    points.push(spot);
                }
            }
            Place::Point(_) => {}
        }
        return;
    }
    view.confirm = false;
    // One lineup there is picked straight away. Several stacked on one spot
    // open a list to pick from.
    let ids: Vec<String> = under(atlas, view, square, at)
        .iter()
        .filter_map(|l| l.id.clone())
        .collect();
    view.choosing = None;
    match ids.as_slice() {
        [] => view.selected = None,
        [one] => view.selected = Some(one.clone()),
        _ => view.choosing = Some((at, ids, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        APART, Floor, Label, Mode, Place, Square, View, clear_of, click, follow, grab, plantable,
        settle, stick, turn_for,
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
            default_agent: "Brimstone".to_owned(),
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
                default_agent: agent.to_owned(),
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
        let floor = Floor::new(Some(&image), 0, false);
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

    /// A click on two lineups stacked on one spot opens a list of both and
    /// picks neither, and a click on a lone pin still picks it.
    #[test]
    fn a_click_on_stacked_lineups_asks_which() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)));
        let lineup = |id: &str, land: [f32; 2]| Lineup {
            id: Some(id.to_owned()),
            map: "Ascent".to_owned(),
            land: Some(land),
            ..Lineup::default()
        };
        let atlas = Atlas {
            lineups: vec![
                lineup("a", [0.5, 0.5]),
                lineup("b", [0.5, 0.5]),
                lineup("c", [0.9, 0.9]),
            ],
            ..Atlas::default()
        };
        let mut view = View {
            map: Some("Ascent".to_owned()),
            ..View::default()
        };
        click(&atlas, &mut view, square, pos2(50.0, 50.0));
        let asked = view.choosing.as_ref().map(|c| c.1.clone());
        assert_eq!(asked, Some(vec!["a".to_owned(), "b".to_owned()]));
        assert_eq!(view.selected, None);
        click(&atlas, &mut view, square, pos2(90.0, 90.0));
        assert_eq!(view.selected.as_deref(), Some("c"));
        assert!(view.choosing.is_none());
    }

    /// Names on one spot stack upwards with a gap between them, one that
    /// would run off the screen is pulled back onto it, and one over a face
    /// moves off it.
    #[test]
    fn names_keep_apart_and_stay_on_the_map() {
        let map = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 400.0));
        let name = Rect::from_center_size(pos2(200.0, 200.0), vec2(80.0, 18.0));
        let first = clear_of(&[], name, map);
        let second = clear_of(&[first], name, map);
        assert_eq!(first, name);
        assert!(
            second.bottom() <= first.top() - APART + 0.01,
            "{second:?} over {first:?}"
        );
        let off = Rect::from_center_size(pos2(395.0, 5.0), vec2(80.0, 18.0));
        let kept = clear_of(&[], off, map);
        assert!(map.contains_rect(kept), "{kept:?}");
        // A name over an agent's face moves off it.
        let face = Rect::from_center_size(pos2(200.0, 200.0), vec2(26.0, 26.0));
        let moved = clear_of(&[face], name, map);
        assert!(!moved.intersects(face), "{moved:?} still covers the face");
        // Boxed in above and below, it goes beside rather than far up.
        let column = Rect::from_min_max(pos2(150.0, 60.0), pos2(250.0, 340.0));
        let beside = clear_of(&[column], name, map);
        assert!(!beside.intersects(column), "{beside:?}");
        assert!(
            (beside.center().y - 200.0).abs() < 1.0,
            "{beside:?} left its row"
        );
    }

    /// Sky Smoke goes down in three places at most, each within its 55
    /// metres of where you stand, and a click on one takes it off.
    #[test]
    fn smokes_from_afar_stay_in_reach_and_in_number() {
        let square = Square::from(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1000.0)));
        let mut view = View::default();
        let mut draft = view.draft();
        draft.lineup.ability = Some("Sky Smoke".to_owned());
        draft.lineup.stand = Some([0.1, 0.1]);
        draft.lineup.land = Some([0.2, 0.1]);
        draft.placing = Place::More;
        view.mode = Mode::Edit(Box::new(draft));
        for at in [pos2(300.0, 300.0), pos2(400.0, 400.0), pos2(500.0, 500.0)] {
            click(&Atlas::default(), &mut view, square, at);
        }
        let Mode::Edit(draft) = &mut view.mode else {
            panic!("the lineup should still be open");
        };
        assert_eq!(
            draft.lineup.points,
            [[0.3, 0.3], [0.4, 0.4]],
            "a third is past its three"
        );
        // At a ten-thousandth of the map to a game unit, 55 metres is 0.55.
        draft.lineup.points.push([0.95, 0.1]);
        settle(&mut draft.lineup, Some(0.0001), None);
        assert_eq!(draft.lineup.points.len(), 2);
        draft.lineup.points = vec![[0.95, 0.1]];
        settle(&mut draft.lineup, Some(0.0001), None);
        let far = draft.lineup.points.first().copied().unwrap_or_default();
        assert!(
            (far[0] - 0.65).abs() < 1e-5 && (far[1] - 0.1).abs() < 1e-5,
            "{far:?}"
        );
        click(&Atlas::default(), &mut view, square, pos2(650.0, 100.0));
        let Mode::Edit(draft) = &view.mode else {
            panic!("the lineup should still be open");
        };
        assert!(
            draft.lineup.points.is_empty(),
            "a click on one takes it off"
        );
    }

    /// A trip's two dots sit three metres either side of it, across the
    /// throw at first, and dragging one turns the trip round it while both
    /// keep their distance. Moving the trip brings them along.
    #[test]
    fn a_trips_dots_turn_it_and_keep_their_distance() {
        let near =
            |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).abs() < 1e-5 && (p[1] - q[1]).abs() < 1e-5;
        let dots = |l: &Lineup, a: [f32; 2], b: [f32; 2]| matches!(l.points.as_slice(), &[p, q] if near(p, a) && near(q, b));
        let mut lineup = Lineup {
            ability: Some("Trapwire".to_owned()),
            stand: Some([0.1, 0.5]),
            land: Some([0.5, 0.5]),
            ..Lineup::default()
        };
        settle(&mut lineup, Some(0.0001), None);
        assert!(
            dots(&lineup, [0.5, 0.53], [0.5, 0.47]),
            "{:?}",
            lineup.points
        );
        lineup.points = vec![[0.5, 0.47], [0.9, 0.5]];
        settle(&mut lineup, Some(0.0001), Some(1));
        assert!(
            dots(&lineup, [0.47, 0.5], [0.53, 0.5]),
            "{:?}",
            lineup.points
        );
        lineup.land = Some([0.2, 0.2]);
        settle(&mut lineup, Some(0.0001), None);
        assert!(
            dots(&lineup, [0.17, 0.2], [0.23, 0.2]),
            "{:?}",
            lineup.points
        );
        lineup.ability = Some("Incendiary".to_owned());
        settle(&mut lineup, Some(0.0001), None);
        assert!(lineup.points.is_empty(), "a molly has no points of its own");
    }

    /// The Spike put down just off plantable ground moves onto it, and one
    /// put down further off or already on it stays where it is.
    #[test]
    fn the_spike_sticks_to_plantable_ground_nearby() {
        let olive = Color32::from_rgb(152, 152, 118);
        let mut image = egui::ColorImage::new([10, 10], vec![Color32::TRANSPARENT; 100]);
        for y in 0..10 {
            for x in 5..10 {
                if let Some(p) = image.pixels.get_mut(y * 10 + x) {
                    *p = olive;
                }
            }
        }
        let lands = |at: [f32; 2], want: [f32; 2]| {
            let got = stick(&image, at, 0.2);
            assert!(
                (got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6,
                "{at:?} went to {got:?}"
            );
        };
        lands([0.45, 0.55], [0.55, 0.55]);
        lands([0.15, 0.55], [0.15, 0.55]);
        lands([0.72, 0.31], [0.72, 0.31]);
    }
}
