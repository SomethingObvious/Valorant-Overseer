use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};
use overseer_core::{Atlas, Lineup, Plan};
use overseer_ui::{Face, art, caps_text, caps_width, colour, motion, size, space};

use super::areas::Reach;
use super::shapes::{self, KINDS, Square};
use super::{Mode, Place, SPIKE, View, ability, areas, face, sight, spike};
use crate::controls;
use crate::settings::MapTurn;

const NAMED: f32 = 440.0;
const MOST_POINTS: usize = 4000;
const REACH: f32 = 16.0;
const FACE: f32 = 13.0;
const RING: f32 = 11.0;
const SAME_SPOT: f32 = 100.0;
const DOT: f32 = 5.0;
const GRIP: f32 = 4.0;

pub(super) fn show(ui: &mut Ui, atlas: &Atlas, view: &mut View) -> bool {
    ui.add_space(space::LG);
    maps(ui, atlas, view);
    let Some(plan) = view
        .map
        .as_ref()
        .and_then(|m| atlas.maps.iter().find(|p| &p.name == m))
    else {
        return false;
    };
    redrawn(ui, atlas, view, plan);
    let room = ui
        .available_rect_before_wrap()
        .shrink2(vec2(space::XL, space::LG));
    let side = room.width().min(room.height());
    if side < 120.0 {
        return false;
    }
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
    let mapped = backdrop(&painter, square, plan);
    if side >= NAMED {
        callouts(&painter, square, plan);
    }
    pins(&painter, (square, ui.clip_rect()), atlas, view, plan);
    let retry = !mapped && {
        let plate = missing(&painter, square, &plan.name);
        let asked = ui.interact(plate, ui.id().with("minimap-retry"), Sense::click());
        asked.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Try the Minimap Again")
        });
        asked
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    };
    if view.export.is_some() {
        if view.shot.is_none() {
            view.shot = Some(*square);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    } else {
        prompt(&painter, square, view);
    }
    let response = response.on_hover_cursor(egui::CursorIcon::Crosshair);
    let scale = plan.scale.filter(|_| view.export.is_none());
    if view.ruler.over(ui, (&painter, square), &response, scale) {
        return retry;
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
        let under_pointer = response
            .hover_pos()
            .map(|at| under(atlas, view, square, at));
        if matches!(view.mode, Mode::Browse)
            && let Some(found) = under_pointer.filter(|f| !f.is_empty())
        {
            notes(&response, atlas, &found);
        }
    }
    retry
}

fn redrawn(ui: &mut Ui, atlas: &Atlas, view: &mut View, plan: &Plan) {
    let Some((key, said)) = redrawn_notice(atlas, view, plan) else {
        return;
    };
    ui.add_space(space::MD);
    let wide = 2.0f32.mul_add(-space::XL, ui.available_width()).max(240.0);
    ui.horizontal(|ui| {
        ui.add_space(space::XL);
        ui.vertical(|ui| {
            ui.set_width(wide);
            let (band, _) = ui.allocate_exact_size(vec2(wide, 30.0), Sense::hover());
            let painter = ui.painter();
            painter.add(crate::board::paint::slant(band, false, true, colour::WARN));
            let _drawn = caps_text(
                painter,
                pos2(band.left() + space::LG, band.center().y),
                Align2::LEFT_CENTER,
                &format!("{} Changed in a Game Update", plan.name),
                Face::Heavy.at(size::TITLE),
                colour::BG,
            );
            ui.add_space(space::SM);
            ui.label(
                egui::RichText::new(said)
                    .font(Face::Body.at(size::BODY))
                    .color(colour::TEXT_STRONG),
            );
            ui.add_space(space::SM);
            if controls::chip(ui, "Got It", true).clicked() {
                view.redraw_seen = Some(key);
            }
        });
    });
}

fn redrawn_notice(atlas: &Atlas, view: &View, plan: &Plan) -> Option<(String, String)> {
    if matches!(&view.mode, Mode::Draw(sketch) if sketch.scratch) {
        return None;
    }
    let at = plan.changed?;
    let key = format!("{} {at:.0}", plan.name);
    if view.redraws.redraws_seen.contains(&key) || view.redraw_seen.as_ref() == Some(&key) {
        return None;
    }
    let old: Vec<&str> = atlas
        .lineups
        .iter()
        .filter(|l| l.map == plan.name && l.saved.is_some_and(|saved| saved < at))
        .map(|l| l.title.as_str())
        .collect();
    Some((key, redrawn_words(&plan.name, &old)?))
}

fn redrawn_words(map: &str, old: &[&str]) -> Option<String> {
    let named = match old {
        [] => return None,
        [one] => {
            return Some(format!(
                "Your lineup {one} was saved before {map} changed.\nCheck it still lands where it should."
            ));
        }
        [rest @ .., last] if old.len() <= 4 => format!("{} and {last}", rest.join(", ")),
        _ => format!(
            "{} and {} more",
            old.get(..3).unwrap_or(old).join(", "),
            old.len() - 3
        ),
    };
    Some(format!(
        "These {} lineups were saved before {map} changed: {named}.\nCheck each still lands where it should.",
        old.len()
    ))
}

fn maps(ui: &mut Ui, atlas: &Atlas, view: &mut View) {
    let (names, turn) = ("Names (T)", "Turn (R)");
    let tools = controls::chip_width(ui, names) + controls::chip_width(ui, turn) + space::SM;
    let room = space::XL.mul_add(-2.0, ui.available_width() - tools);
    if room < TOOLS_BESIDE {
        let wide = ui.available_width();
        ui.allocate_ui(vec2(wide, 0.0), |ui| map_chips(ui, atlas, view));
        ui.add_space(space::SM);
        ui.allocate_ui_with_layout(
            vec2(wide, space::ROW),
            egui::Layout::right_to_left(egui::Align::Min),
            |ui| map_tools(ui, view, (names, turn)),
        );
        return;
    }
    ui.horizontal_top(|ui| {
        ui.allocate_ui(vec2(room, 0.0), |ui| map_chips(ui, atlas, view));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            map_tools(ui, view, (names, turn));
        });
    });
}

const TOOLS_BESIDE: f32 = 160.0;

fn map_chips(ui: &mut Ui, atlas: &Atlas, view: &mut View) {
    let browsing = matches!(view.mode, Mode::Browse);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(space::XL);
        ui.spacing_mut().item_spacing = vec2(space::SM, space::SM);
        for plan in &atlas.maps {
            let on = view.map.as_ref() == Some(&plan.name);
            if controls::chip(ui, &plan.name, on).clicked() && browsing && !on {
                view.map = Some(plan.name.clone());
                view.selected = None;
                view.confirm = None;
            }
        }
    });
}

fn map_tools(ui: &mut Ui, view: &mut View, (names, turn): (&str, &str)) {
    ui.add_space(space::XL);
    ui.spacing_mut().item_spacing = vec2(space::SM, space::SM);
    let shown = view.naming();
    if controls::chip(ui, names, shown).clicked() {
        view.names = Some(!shown);
    }
    if controls::chip(ui, turn, view.turned != 0).clicked() {
        view.turned = (view.turned + 1) % 4;
    }
}

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
    let down = [dy, dx, -dy, -dx];
    down.iter()
        .zip(0..4u8)
        .max_by(|(a, _), (b, _)| a.total_cmp(b))
        .map_or(0, |(_, turn)| turn)
}

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

fn backdrop(painter: &egui::Painter, square: Square, plan: &Plan) -> bool {
    painter.rect_filled(*square, 0, colour::VOID);
    match plan
        .minimap
        .as_deref()
        .and_then(|path| art::file(painter.ctx(), path))
    {
        Some(texture) => {
            turned_image(painter, square, texture.id(), Color32::from_gray(118));
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
        None => return false,
    }
    true
}

fn missing(painter: &egui::Painter, square: Square, name: &str) -> Rect {
    let mut job = egui::text::LayoutJob::simple(
        format!("The {name} minimap hasn't downloaded. Click here to try again."),
        Face::Body.at(size::BODY),
        colour::TEXT,
        square.width() * 0.7,
    );
    job.halign = egui::Align::Center;
    let words = painter.layout_job(job);
    let plate = Rect::from_center_size(
        square.center(),
        words.size() + vec2(space::XXL * 2.0, space::XL),
    );
    painter.add(crate::board::paint::slant(
        plate,
        true,
        true,
        colour::BG_RAISED,
    ));
    painter.galley(
        pos2(plate.center().x, plate.center().y - words.size().y / 2.0),
        words,
        colour::TEXT,
    );
    plate
}

const STICKY: f32 = 200.0;

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

fn plantable(pixel: Color32) -> bool {
    let [r, g, b, a] = pixel.to_srgba_unmultiplied();
    a > 200 && r.abs_diff(g) < 6 && r > b.saturating_add(20)
}

fn callouts(painter: &egui::Painter, square: Square, plan: &Plan) {
    let font = Face::Display.at(size::MICRO);
    let count = plan.callouts.len();
    let spots = spots(painter, square, plan, &font);
    for (index, (callout, spot)) in plan.callouts.iter().zip(spots).enumerate() {
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

const PLATE: f32 = 14.0;
const CELLS: u16 = 256;
/// How far a name may move from Riot's spot to find floor, in cells, about
/// a tenth of the map.
const NEAR: i32 = 26;
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

struct Label {
    /// Riot's spot for it.
    at: [f32; 2],
    size: [f32; 2],
    letter: bool,
}

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

/// The walls of a map's minimap, traced once it is on disk.
fn walls_of(ctx: &egui::Context, plan: &Plan) -> Option<std::sync::Arc<sight::Walls>> {
    let id = egui::Id::new(("walls", &plan.name));
    if let Some(kept) = ctx.data(|store| store.get_temp::<std::sync::Arc<sight::Walls>>(id)) {
        return Some(kept);
    }
    let image = plan
        .minimap
        .as_deref()
        .and_then(|path| art::file_pixels(ctx, path))?;
    let walls = std::sync::Arc::new(sight::walls(&image));
    ctx.data_mut(|store| store.insert_temp(id, std::sync::Arc::clone(&walls)));
    Some(walls)
}

type Cells = [i32; 4];

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

    fn upto(&self, x: i32, y: i32) -> u32 {
        let row = i32::from(CELLS) + 1;
        usize::try_from(y * row + x)
            .ok()
            .and_then(|at| self.sums.get(at))
            .copied()
            .unwrap_or(0)
    }

    fn wall(&self, x: i32, y: i32) -> bool {
        self.count([x, y, x + 1, y + 1]) == 0
    }

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

    fn count(&self, [x0, y0, x1, y1]: Cells) -> u32 {
        (self.upto(x1, y1) + self.upto(x0, y0))
            .saturating_sub(self.upto(x0, y1) + self.upto(x1, y0))
    }

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

fn prompt(painter: &egui::Painter, square: Square, view: &View) {
    let drawing;
    let words = match &view.mode {
        _ if view.ruler.on => "Drag across the map to measure, or click to clear it",
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
                    "Click the wall to bend it there, or a bend to take it off"
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
                "stamp" => "Click the map to put it down, or drag one to move it".to_owned(),
                _ => format!(
                    "Drag to draw a {name}, drag a dot to move one, or a square to resize it"
                ),
            };
            &drawing
        }
    };
    let Some((font, wide)) = [size::BODY + 2.0, size::LABEL]
        .into_iter()
        .map(|at| {
            let font = Face::Body.at(at);
            let shown = painter
                .layout_no_wrap(words.to_owned(), font.clone(), colour::TEXT_STRONG)
                .size()
                .x;
            (font, space::XL.mul_add(2.0, shown))
        })
        .find(|(_, wide)| *wide <= 2.0f32.mul_add(-super::ruler::ROOM, square.width()))
    else {
        return;
    };
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
    let _words = painter.text(
        plate.center(),
        Align2::CENTER_CENTER,
        words,
        font,
        colour::TEXT_STRONG,
    );
}

fn draw(ui: &Ui, painter: &egui::Painter, square: Square, response: &Response, view: &mut View) {
    let map = view.map.clone().unwrap_or_default();
    let Mode::Draw(sketch) = &mut view.mode else {
        return;
    };
    if sketch.tool() == "erase" {
        if erase(sketch, square, response) && !sketch.scratch {
            view.unsaved.insert(map, sketch.shapes.clone());
        }
        return;
    }
    let mut changed = false;
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        take_hold(sketch, square, origin);
    }
    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        if grip_on(square, sketch, at).is_some() {
        } else if let Some(index) = dot_at(square, &sketch.shapes, at) {
            sketch.pick(index);
        } else if sketch.stamping() || (sketch.lettering() && !sketch.words.trim().is_empty()) {
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
        let from = if sketch.stamping() { end } else { start };
        shapes::paint(painter, square, &sketch.drawing(from, end));
        if response.drag_stopped() {
            if sketch.stamping() || shapes::long_enough(square, start, end) {
                sketch.shapes.push(sketch.drawing(from, end));
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
        sketch.confirm = None;
        if !sketch.scratch {
            view.unsaved.insert(map, sketch.shapes.clone());
        }
    }
}

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
    sketch.confirm = None;
    true
}

fn dots(painter: &egui::Painter, square: Square, sketch: &super::Sketch) {
    for (index, shape) in sketch.shapes.iter().enumerate() {
        let picked = sketch.picked == Some(index);
        if shape.kind == "stamp" {
            if picked {
                painter.rect_stroke(
                    shapes::stamp(square, shapes::at(square, shape.a)).expand(4.0),
                    0,
                    Stroke::new(1.5, colour::TEXT_STRONG),
                    egui::StrokeKind::Outside,
                );
            }
            continue;
        }
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
                Rect::from_center_size(shapes::at(square, grip), vec2(GRIP * 2.0, GRIP * 2.0));
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

fn dot_at(square: Square, shapes: &[overseer_core::Drawing], at: Pos2) -> Option<usize> {
    shapes.iter().rposition(|s| {
        if s.kind == "stamp" {
            shapes::hit(square, s, at)
        } else {
            shapes::handle(square, s).distance(at) <= DOT + 4.0
        }
    })
}

fn grip_on(square: Square, sketch: &super::Sketch, at: Pos2) -> Option<(usize, usize)> {
    let index = sketch.picked?;
    let shape = sketch.shapes.get(index)?;
    shapes::grips(shape)
        .iter()
        .position(|&g| shapes::at(square, g).distance(at) <= GRIP + 8.0)
        .map(|grip| (index, grip))
}

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
    if let Some(to) = now
        && sketch.stroke.len() < MOST_POINTS
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

fn grip_cursor(shape: Option<&overseer_core::Drawing>, grip: usize) -> egui::CursorIcon {
    let Some(shape) = shape.filter(|s| matches!(s.kind.as_str(), "rect" | "oval" | "textbox"))
    else {
        return egui::CursorIcon::Move;
    };
    let ([ax, ay], [bx, by]) = (shape.a, shape.b);
    let leans = (bx - ax) * (by - ay) >= 0.0;
    if grip.is_multiple_of(2) == leans {
        egui::CursorIcon::ResizeNwSe
    } else {
        egui::CursorIcon::ResizeNeSw
    }
}

fn tint(side: Option<&str>) -> Color32 {
    crate::board::paint::side_tint(side, colour::TEXT_DIM)
}

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
    let run = |from: Pos2, way: Vec2, most: f32| floor.run(square, (from, way), most);
    let walls = walls_of(painter.ctx(), plan);
    let inside = painter.with_clip_rect(square.rect.intersect(painter.clip_rect()));
    let areas = |lineup: &Lineup, ink: Color32| {
        area(
            &inside,
            square,
            lineup,
            scale,
            (ink, &run, walls.as_deref()),
        );
    };
    let dimmed = if dim { 0.45 } else { 0.9 };
    let faint = |lineup: &Lineup| tint(lineup.side.as_deref()).gamma_multiply(dimmed);
    let shade = |l: &Lineup| view.area.unwrap_or_else(|| tint(l.side.as_deref()));
    for lineup in &rest {
        areas(lineup, shade(lineup).gamma_multiply(dimmed));
    }
    for drawing in view.shapes(atlas) {
        shapes::paint(painter, square, drawing);
    }
    for lineup in &picked {
        areas(lineup, shade(lineup));
    }
    if let Some(lineup) = writing {
        areas(lineup, view.area.unwrap_or(colour::TEXT_STRONG));
    }
    for lineup in &rest {
        throw(painter, square, lineup, faint(lineup));
    }
    for lineup in &rest {
        marks(painter, square, atlas, lineup, (faint(lineup), main));
    }
    let shown: Vec<&Lineup> = rest.iter().chain(&picked).copied().chain(writing).collect();
    let mut taken = covered(square, &shown);
    let painter = &egui::Painter::new(painter.ctx().clone(), painter.layer_id(), room);
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
    badges(painter, square, (atlas, view), rest.iter().chain(&picked));
    if let Some(lineup) = writing {
        pin(painter, square, atlas, lineup, (colour::TEXT_STRONG, main));
        handles(painter, square, lineup, scale);
    }
}

pub(super) fn badge(painter: &egui::Painter, at: Pos2) {
    painter.circle_filled(at, 7.0, colour::WARN);
    painter.circle_stroke(at, 7.0, Stroke::new(1.5, colour::VOID));
    painter.text(
        at,
        Align2::CENTER_CENTER,
        "!",
        Face::Heavy.at(size::LABEL),
        colour::VOID,
    );
}

fn handles(painter: &egui::Painter, square: Square, lineup: &Lineup, scale: Option<f32>) {
    let units = scale.map(|s| s * square.width());
    let stand = lineup.stand.map(|p| shapes::at(square, p));
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
            lineup
                .points
                .iter()
                .for_each(|p| dot(shapes::at(square, *p)));
        }
        Some(Reach::Bent(length)) => {
            if let (Some(from), Some(to), Some(units)) =
                (stand, lineup.land.map(|p| shapes::at(square, p)), units)
            {
                let bends: Vec<Pos2> = lineup
                    .points
                    .iter()
                    .map(|p| shapes::at(square, *p))
                    .collect();
                if bends.is_empty() {
                    let path = areas::bent(from, &bends, to, length * units);
                    path.get(path.len() >> 1).copied().into_iter().for_each(dot);
                }
                bends.into_iter().for_each(dot);
            }
        }
        _ => {}
    }
}

fn area(
    painter: &egui::Painter,
    square: Square,
    lineup: &Lineup,
    scale: Option<f32>,
    (ink, run, walls): (Color32, areas::Run<'_>, Option<&sight::Walls>),
) {
    let reach = if lineup.agent == SPIKE {
        Some(areas::DEFUSE)
    } else {
        lineup.ability.as_deref().and_then(areas::reach)
    };
    let (Some(land), Some(reach), Some(scale)) = (lineup.land, reach, scale) else {
        return;
    };
    let stand = lineup.stand.map(|s| shapes::at(square, s));
    let points: Vec<Pos2> = lineup
        .points
        .iter()
        .map(|p| shapes::at(square, *p))
        .collect();
    // A turret's view is worked out on the traced walls once there are any.
    if let (Reach::Aim(degrees), Some(walls), Some(aim)) = (reach, walls, points.first()) {
        let at = shapes::at(square, land);
        let bits = |v: f32| v.to_bits();
        let slot = egui::Id::new(("turret-sight", &lineup.id));
        let asked = egui::Id::new((
            [at.x, at.y, aim.x, aim.y, degrees].map(bits),
            [square.rect.min.x, square.rect.min.y, square.rect.width()].map(bits),
            square.turn,
            std::ptr::from_ref(walls) as usize,
        ));
        let ctx = painter.ctx();
        let kept = ctx.data(|d| d.get_temp::<(egui::Id, (Pos2, Vec<Pos2>))>(slot));
        let (stands, rim) = match kept {
            Some((was, cast)) if was == asked => cast,
            _ => {
                let cast = sight::cone(walls, square, (at, *aim), degrees);
                ctx.data_mut(|d| d.insert_temp(slot, (asked, cast.clone())));
                cast
            }
        };
        painter.extend(areas::seen(stands, rim, ink));
        return;
    }
    let afar = areas::spots(lineup.ability.as_deref()).is_some();
    let (lands, own): (Vec<Pos2>, &[Pos2]) = if afar {
        (
            std::iter::once(shapes::at(square, land))
                .chain(points.iter().copied())
                .collect(),
            &[],
        )
    } else {
        (vec![shapes::at(square, land)], &points)
    };
    for at in lands {
        areas::draw(
            painter,
            (stand, at, own),
            (reach, scale * square.width()),
            (ink, run),
        );
    }
}

fn more_lands(lineup: &Lineup) -> &[[f32; 2]] {
    if areas::spots(lineup.ability.as_deref()).is_some() {
        &lineup.points
    } else {
        &[]
    }
}

fn badges<'a>(
    painter: &egui::Painter,
    square: Square,
    (atlas, view): (&Atlas, &View),
    lineups: impl Iterator<Item = &'a &'a Lineup>,
) {
    for lineup in lineups {
        if let Some(at) = lineup
            .stand
            .or(lineup.land)
            .filter(|_| view.marked(atlas, lineup))
        {
            badge(painter, shapes::at(square, at) + vec2(FACE, -FACE));
        }
    }
}

fn pin(
    painter: &egui::Painter,
    square: Square,
    atlas: &Atlas,
    lineup: &Lineup,
    (ink, main): (Color32, Option<&str>),
) {
    throw(painter, square, lineup, ink);
    marks(painter, square, atlas, lineup, (ink, main));
}

fn throw(painter: &egui::Painter, square: Square, lineup: &Lineup, ink: Color32) {
    let stand = lineup.stand.map(|p| shapes::at(square, p));
    let land = lineup.land.map(|p| shapes::at(square, p));
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
}

fn marks(
    painter: &egui::Painter,
    square: Square,
    atlas: &Atlas,
    lineup: &Lineup,
    (ink, main): (Color32, Option<&str>),
) {
    let stand = lineup.stand.map(|p| shapes::at(square, p));
    let land = lineup.land.map(|p| shapes::at(square, p));
    if let Some(to) = land {
        land_mark(painter, to, RING, atlas, lineup, ink);
    }
    for more in more_lands(lineup) {
        land_mark(painter, shapes::at(square, *more), RING, atlas, lineup, ink);
    }
    if let Some(from) = stand {
        stand_mark(painter, from, FACE, lineup, (ink, main));
    }
}

pub(super) fn land_mark(
    painter: &egui::Painter,
    at: Pos2,
    radius: f32,
    atlas: &Atlas,
    lineup: &Lineup,
    ink: Color32,
) {
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

fn label(
    painter: &egui::Painter,
    (square, room): (Square, Rect),
    lineup: &Lineup,
    (taken, ink): (&mut Vec<Rect>, Color32),
) {
    let Some(at) = lineup.land.or(lineup.stand).map(|p| shapes::at(square, p)) else {
        return;
    };
    let font = Face::Display.at(size::LABEL);
    let wide = space::MD.mul_add(
        2.0,
        overseer_ui::typed_width(painter, &lineup.title, font.clone()),
    ) + BAR;
    let plate = clear_of(
        taken,
        Rect::from_center_size(at - vec2(0.0, RING + 14.0), vec2(wide, 20.0)),
        room,
    );
    taken.push(plate);
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
    let _title = overseer_ui::typed_text(
        painter,
        pos2(plate.center().x + BAR / 2.0, plate.center().y),
        Align2::CENTER_CENTER,
        &lineup.title,
        font,
        ink,
    );
}

fn covered(square: Square, lineups: &[&Lineup]) -> Vec<Rect> {
    let mut out = Vec::new();
    for lineup in lineups {
        if let Some(stand) = lineup.stand {
            out.push(Rect::from_center_size(
                shapes::at(square, stand),
                Vec2::splat(FACE * 2.0),
            ));
        }
        let Some(land) = lineup.land else {
            continue;
        };
        out.push(Rect::from_center_size(
            shapes::at(square, land),
            Vec2::splat(RING * 2.0),
        ));
    }
    out
}

const BAR: f32 = 3.0;

const APART: f32 = 4.0;

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

fn drag(ui: &Ui, atlas: &Atlas, view: &mut View, square: Square, response: &Response) {
    if let Mode::Edit(draft) = &mut view.mode {
        let held = match draft.grab {
            Some(Place::Point(index)) => Some(index),
            _ => None,
        };
        let scale = scale_of(atlas, &draft.lineup.map);
        settle(&mut draft.lineup, scale, held);
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

fn grab(atlas: &Atlas, view: &mut View, square: Square, origin: Pos2) {
    let Mode::Edit(draft) = &mut view.mode else {
        return;
    };
    let on =
        |p: Option<[f32; 2]>| p.is_some_and(|p| shapes::at(square, p).distance(origin) <= REACH);
    // A trip's ends sit close to where it lands, so they come first.
    let own = draft
        .lineup
        .points
        .iter()
        .position(|p| shapes::at(square, *p).distance(origin) <= REACH);
    let scale = scale_of(atlas, &draft.lineup.map);
    draft.thrown_from = None;
    draft.grab = Some(if let Some(index) = own {
        Place::Point(index)
    } else if on(draft.lineup.land) {
        Place::Land
    } else if on(draft.lineup.stand) {
        Place::Stand
    } else if draft.lineup.agent == SPIKE {
        draft.lineup.land = Some(shapes::spot(square, origin));
        Place::Land
    } else if let Some(at) = on_wall(&draft.lineup, square, scale, origin)
        && draft.lineup.points.len() < BENDS
    {
        draft.lineup.points.insert(at, shapes::spot(square, origin));
        Place::Point(at)
    } else {
        let spot = shapes::spot(square, origin);
        draft.lineup.stand = Some(snapped(atlas, &draft.lineup, spot, Place::Stand));
        draft.thrown_from = draft.lineup.stand;
        Place::Land
    });
}

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
        // Only a new throw. Moving a pin or turning a turret can leave the
        // two close together on purpose.
        if draft.thrown_from.take().is_some()
            && let (Some(from), Some(to)) = (draft.lineup.stand, draft.lineup.land)
            && shapes::at(square, from).distance(shapes::at(square, to)) < FACE
        {
            draft.lineup.land = None;
        }
    }
}

fn bend(draft: &mut super::Draft, scale: Option<f32>, square: Square, at: Pos2) -> bool {
    if !matches!(
        draft.lineup.ability.as_deref().and_then(areas::reach),
        Some(Reach::Bent(_))
    ) {
        return false;
    }
    let points = &draft.lineup.points;
    if let Some(index) = points
        .iter()
        .position(|p| shapes::at(square, *p).distance(at) <= REACH)
    {
        draft.lineup.points.remove(index);
        return true;
    }
    match on_wall(&draft.lineup, square, scale, at) {
        Some(index) if points.len() < BENDS => {
            draft.lineup.points.insert(index, shapes::spot(square, at));
            true
        }
        _ => false,
    }
}

fn scale_of(atlas: &Atlas, map: &str) -> Option<f32> {
    atlas
        .maps
        .iter()
        .find(|p| p.name == map)
        .and_then(|p| p.scale)
}

fn on_wall(lineup: &Lineup, square: Square, scale: Option<f32>, at: Pos2) -> Option<usize> {
    let Some(Reach::Bent(length)) = lineup.ability.as_deref().and_then(areas::reach) else {
        return None;
    };
    let (Some(from), Some(to), Some(scale)) = (lineup.stand, lineup.land, scale) else {
        return None;
    };
    let bends: Vec<Pos2> = lineup
        .points
        .iter()
        .map(|p| shapes::at(square, *p))
        .collect();
    areas::wall(
        shapes::at(square, from),
        &bends,
        shapes::at(square, to),
        length * scale * square.width(),
    )
    .into_iter()
    .map(|(p, stretch)| (p.distance(at), stretch))
    .filter(|(far, _)| *far <= REACH)
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, stretch)| stretch)
}

const BENDS: usize = 8;

const TRIP_DOT: f32 = 300.0;

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
            lineup.points.truncate(BENDS);
            for _ in 0..4 {
                let (Some(stand), Some(land)) = (lineup.stand, lineup.land) else {
                    break;
                };
                let bends: Vec<Pos2> = lineup.points.iter().map(|p| pos2(p[0], p[1])).collect();
                let path = areas::wall(
                    pos2(stand[0], stand[1]),
                    &bends,
                    pos2(land[0], land[1]),
                    length * scale,
                );
                if let Some(&(end, stretch)) = path.last()
                    && end.distance(pos2(land[0], land[1])) > 1e-6
                {
                    lineup.points.truncate(stretch);
                    lineup.land = Some([end.x, end.y]);
                }
            }
        }
        Some(Reach::Aim(..) | Reach::Bounce(_)) => {
            lineup.points.truncate(1);
            if let (true, Some(land)) = (lineup.points.is_empty(), lineup.land) {
                let at = pos2(land[0], land[1]);
                let from = lineup.stand.map_or(at - Vec2::X, |s| pos2(s[0], s[1]));
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

fn within(from: [f32; 2], p: [f32; 2], most: f32) -> [f32; 2] {
    let way = pos2(p[0], p[1]) - pos2(from[0], from[1]);
    if way.length() <= most {
        return p;
    }
    let at = pos2(from[0], from[1]) + way.normalized() * most;
    [at.x, at.y]
}

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

fn under<'a>(atlas: &'a Atlas, view: &View, square: Square, at: Pos2) -> Vec<&'a Lineup> {
    let mut found: Vec<(f32, &Lineup)> = view
        .shown(atlas)
        .into_iter()
        .filter_map(|l| {
            let reach = [l.stand, l.land]
                .into_iter()
                .flatten()
                .chain(more_lands(l).iter().copied())
                .map(|p| shapes::at(square, p).distance(at))
                .fold(f32::INFINITY, f32::min);
            (reach <= REACH).then_some((reach, l))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, l)| l).collect()
}

fn notes(response: &Response, atlas: &Atlas, found: &[&Lineup]) {
    let _shown = response.clone().on_hover_ui_at_pointer(|ui| {
        for (index, lineup) in found.iter().enumerate() {
            if index > 0 {
                ui.add_space(space::SM);
                ui.separator();
            }
            note(ui, atlas, lineup);
        }
        if found.len() > 1 {
            ui.add_space(space::SM);
            ui.label(
                egui::RichText::new("Click again for the next one.")
                    .font(Face::Body.at(size::MICRO + 1.0))
                    .color(colour::TEXT_DIM),
            );
        }
    });
}

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
        ui.label(
            egui::RichText::new(notes)
                .font(Face::Body.at(size::MICRO + 1.0))
                .color(colour::TEXT),
        );
    }
}

fn click(atlas: &Atlas, view: &mut View, square: Square, at: Pos2) {
    let spot = shapes::spot(square, at);
    if let Mode::Edit(draft) = &mut view.mode
        && bend(draft, scale_of(atlas, &draft.lineup.map), square, at)
    {
        return;
    }
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
                    .position(|p| shapes::at(square, *p).distance(at) <= REACH)
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
    view.confirm = None;
    let ids: Vec<String> = under(atlas, view, square, at)
        .iter()
        .filter_map(|l| l.id.clone())
        .collect();
    let mut ring = ids.clone();
    ring.sort_unstable();
    view.selected = ring
        .iter()
        .position(|id| view.selected.as_ref() == Some(id))
        .map_or_else(
            || ids.first().cloned(),
            |on| ring.get((on + 1) % ring.len()).cloned(),
        );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Way {
    Up,
    Down,
    Left,
    Right,
}

pub(super) fn stepped(atlas: &Atlas, view: &View, way: Way) -> Option<String> {
    let plan = atlas
        .maps
        .iter()
        .find(|p| Some(&p.name) == view.map.as_ref())?;
    let turn = (turn_for(plan, view.turn_to) + view.turned) % 4;
    let across = matches!(way, Way::Left | Way::Right);
    let place = |lineup| place_of(lineup, turn, across);
    let order = |a: &([f32; 4], &str), b: &([f32; 4], &str)| {
        a.0.iter()
            .zip(&b.0)
            .map(|(x, y)| x.total_cmp(y))
            .find(|o| o.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(b.1))
    };
    let mut places: Vec<([f32; 4], &str)> =
        view.shown(atlas).into_iter().filter_map(place).collect();
    places.sort_by(order);
    let from = atlas
        .lineups
        .iter()
        .find(|l| l.id.is_some() && l.id == view.selected && Some(&l.map) == view.map.as_ref())
        .and_then(place);
    let forward = matches!(way, Way::Down | Way::Right);
    let next = match (from, forward) {
        (None, true) => places.first(),
        (None, false) => places.last(),
        (Some(at), true) => places.iter().find(|p| order(p, &at).is_gt()),
        (Some(at), false) => places.iter().rev().find(|p| order(p, &at).is_lt()),
    };
    next.map(|(_, id)| (*id).to_owned())
}

fn place_of(lineup: &Lineup, turn: u8, across: bool) -> Option<([f32; 4], &str)> {
    let id = lineup.id.as_deref()?;
    let land = shapes::turned(lineup.land.or(lineup.stand)?, turn);
    let stand = shapes::turned(lineup.stand.or(lineup.land)?, turn);
    let at = if across {
        [land[0], land[1], stand[0], stand[1]]
    } else {
        [land[1], land[0], stand[1], stand[0]]
    };
    Some((at, id))
}

#[cfg(test)]
mod tests {
    use super::{
        APART, Mode, Place, Square, View, Way, clear_of, click, follow, grab, plantable, settle,
        stepped, stick, turn_for,
    };
    use crate::settings::MapTurn;
    use egui::{Color32, Rect, pos2, vec2};
    use overseer_core::Plan;
    use overseer_core::{Atlas, Lineup};

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

    #[test]
    fn a_drag_places_a_lineup_and_moves_its_pins() {
        let square = Square {
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)),
            turn: 0,
        };
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

    #[test]
    fn a_landing_spot_snaps_onto_one_a_metre_away() {
        let square = Square {
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1000.0)),
            turn: 0,
        };
        let atlas = Atlas {
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

    #[test]
    fn the_arrows_step_through_every_lineup_either_way_and_mix() {
        let lineup = |id: Option<&str>, map: &str, land: [f32; 2], stand: [f32; 2]| Lineup {
            id: id.map(ToOwned::to_owned),
            map: map.to_owned(),
            land: Some(land),
            stand: Some(stand),
            ..Lineup::default()
        };
        let atlas = Atlas {
            maps: vec![Plan {
                name: "Ascent".to_owned(),
                ..Plan::default()
            }],
            lineups: vec![
                lineup(Some("d"), "Ascent", [0.5, 0.6], [0.1, 0.6]),
                lineup(Some("b"), "Ascent", [0.8, 0.3], [0.5, 0.9]),
                lineup(Some("a"), "Ascent", [0.2, 0.1], [0.2, 0.9]),
                lineup(Some("c"), "Ascent", [0.5, 0.6], [0.9, 0.6]),
                lineup(Some("elsewhere"), "Bind", [0.0, 0.0], [0.0, 0.0]),
                lineup(None, "Ascent", [0.0, 0.0], [0.0, 0.0]),
            ],
            ..Atlas::default()
        };
        let mut view = View {
            map: Some("Ascent".to_owned()),
            ..View::default()
        };
        let walk = |view: &mut View, way: Way| {
            view.selected = None;
            let mut seen = Vec::new();
            while let Some(next) = stepped(&atlas, view, way) {
                seen.push(next.clone());
                view.selected = Some(next);
            }
            seen.join("")
        };
        assert_eq!(walk(&mut view, Way::Down), "abdc", "top to bottom");
        assert_eq!(walk(&mut view, Way::Up), "cdba", "bottom to top");
        assert_eq!(walk(&mut view, Way::Right), "adcb", "left to right");
        assert_eq!(walk(&mut view, Way::Left), "bcda", "right to left");

        view.selected = Some("b".to_owned());
        let mut mixed = String::new();
        for way in [Way::Left, Way::Up, Way::Right] {
            view.selected = stepped(&atlas, &view, way);
            mixed.push_str(view.selected.as_deref().unwrap_or("-"));
        }
        assert_eq!(mixed, "cdc", "each arrow goes on from where the pick is");
        assert_eq!(
            stepped(&atlas, &view, Way::Down),
            None,
            "the bottom stays put"
        );

        view.turned = 1;
        assert_eq!(
            walk(&mut view, Way::Down),
            "adcb",
            "a quarter turn turns the arrows"
        );
    }

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
        let face = Rect::from_center_size(pos2(200.0, 200.0), vec2(26.0, 26.0));
        let moved = clear_of(&[face], name, map);
        assert!(!moved.intersects(face), "{moved:?} still covers the face");
        let column = Rect::from_min_max(pos2(150.0, 60.0), pos2(250.0, 340.0));
        let beside = clear_of(&[column], name, map);
        assert!(!beside.intersects(column), "{beside:?}");
        assert!(
            (beside.center().y - 200.0).abs() < 1.0,
            "{beside:?} left its row"
        );
    }

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
