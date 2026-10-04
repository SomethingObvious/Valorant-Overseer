//! What a turret sees. The minimap's walls are traced once into straight
//! lines, and the shape seen from a point is found by aiming at every corner
//! of them, which keeps corners exact and the shape still as the turret turns.
//! A fan of rays at set angles cuts across any corner that falls between two.
//!
//! Abyss drops away into a void, which is as clear on its minimap as the
//! inside of a wall. Its minimap draws the edge of a drop in grey and a
//! wall in white, so a clear patch whose edge is mostly grey is seen across.

use std::collections::HashMap;

use egui::{ColorImage, Pos2, Vec2, pos2, vec2};

use super::shapes::{self, Square};

/// The minimap's walls as straight lines, in its own pixels, with the corners
/// they meet at and a grid of which lines pass through which part of it.
#[derive(Debug, Default)]
pub(super) struct Walls {
    /// Each wall, end to end.
    lines: Vec<[Pos2; 2]>,
    /// Every end of a line, once.
    corners: Vec<Pos2>,
    /// The lines through each square of the grid, row by row.
    grid: Vec<Vec<usize>>,
    /// How many squares the grid is across and down.
    cells: (i64, i64),
    /// How big the minimap is, in pixels.
    size: [f32; 2],
    /// Which pixels can't be seen through, row by row.
    blocked: Vec<bool>,
}

/// How many pixels a square of the grid is each way.
const CELL: f32 = 16.0;
/// How far a traced wall may stray from the pixels' own edge when it is
/// straightened, in pixels.
const STRAY: f32 = 0.4;
/// How far either side of a corner the rays that pass it are aimed, in
/// radians.
const PAST: f32 = 1e-4;
/// How close to a line's end still hits it, as a share of the line. Without
/// it a ray aimed right at a corner where two lines meet can slip between them.
const SNUG: f32 = 1e-5;
/// How much of a clear patch's edge has to be drawn as a drop for it to
/// count as void. Abyss's drops are half grey or more, and no patch on any
/// other map is over a sixth.
const VOID: f32 = 0.3;
/// How far a turret dropped in a wall is moved to stand beside it, in
/// pixels.
const STEP_OUT: i64 = 16;

/// The walls of `image`: where the pixels that can't be seen through meet
/// the rest.
pub(super) fn walls(image: &ColorImage) -> Walls {
    let [wide, tall] = image.size;
    let blocked = blocked(image);
    let lines: Vec<[Pos2; 2]> = trace(&blocked, image.size)
        .into_iter()
        .flat_map(|run| {
            let run = straighten(&run);
            run.iter()
                .zip(run.iter().skip(1))
                .map(|(a, b)| [*a, *b])
                .collect::<Vec<_>>()
        })
        .collect();
    let mut corners: Vec<Pos2> = lines.iter().flatten().copied().collect();
    corners.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    corners.dedup();
    let cells = (
        cell_of(wide as f32 + 1.0) + 1,
        cell_of(tall as f32 + 1.0) + 1,
    );
    let mut grid = vec![Vec::new(); usize::try_from(cells.0 * cells.1).unwrap_or(0)];
    for (index, [a, b]) in lines.iter().enumerate() {
        for row in cell_of(a.y.min(b.y))..=cell_of(a.y.max(b.y)) {
            for col in cell_of(a.x.min(b.x))..=cell_of(a.x.max(b.x)) {
                if let Some(square) = usize::try_from(row * cells.0 + col)
                    .ok()
                    .and_then(|i| grid.get_mut(i))
                {
                    square.push(index);
                }
            }
        }
    }
    Walls {
        lines,
        corners,
        grid,
        cells,
        size: [wide as f32, tall as f32],
        blocked,
    }
}

/// Which pixels of `image` can't be seen through, row by row: the clear
/// ones off the floor, unless they are void, and the white edge of a wall
/// that stands at the void.
fn blocked(image: &ColorImage) -> Vec<bool> {
    let [wide, tall] = image.size;
    let clear: Vec<bool> = image.pixels.iter().map(|p| p.a() <= 128).collect();
    let near = |i: usize| {
        let (x, y) = (i % wide, i.div_euclid(wide));
        [
            (x > 0).then(|| i - 1),
            (x + 1 < wide).then(|| i + 1),
            (y > 0).then(|| i - wide),
            (y + 1 < tall).then(|| i + wide),
        ]
        .into_iter()
        .flatten()
    };
    let shade = |i: usize, test: fn(u8, u8, u8) -> bool| {
        image
            .pixels
            .get(i)
            .is_some_and(|p| p.a() > 128 && test(p.r(), p.g(), p.b()))
    };
    // A drop's edge is a light grey, a little yellow under a spike site.
    let grey = |r: u8, g: u8, b: u8| {
        r.abs_diff(222) <= 18
            && r.abs_diff(g) < 6
            && (r.saturating_sub(16)..=r.saturating_add(5)).contains(&b)
    };
    let white = |r: u8, g: u8, b: u8| r.min(g).min(b) >= 250;
    // Each clear patch, joined edge to edge, and how many of the floor
    // pixels on its edge there are and how many are grey.
    let mut patch = vec![usize::MAX; clear.len()];
    let mut edges: Vec<(u32, u32)> = Vec::new();
    for start in 0..clear.len() {
        if !clear.get(start).copied().unwrap_or(false) || patch.get(start) != Some(&usize::MAX) {
            continue;
        }
        let id = edges.len();
        let mut count = (0, 0);
        let mut stack = vec![start];
        if let Some(own) = patch.get_mut(start) {
            *own = id;
        }
        while let Some(i) = stack.pop() {
            for n in near(i) {
                if !clear.get(n).copied().unwrap_or(false) {
                    count.0 += 1;
                    count.1 += u32::from(shade(n, grey));
                } else if let Some(other) = patch.get_mut(n)
                    && *other == usize::MAX
                {
                    *other = id;
                    stack.push(n);
                }
            }
        }
        edges.push(count);
    }
    let void: Vec<bool> = edges
        .iter()
        .map(|&(all, greys)| all > 0 && greys as f32 >= VOID * all as f32)
        .collect();
    let in_void = |i: usize| {
        patch
            .get(i)
            .and_then(|id| void.get(*id))
            .copied()
            .unwrap_or(false)
    };
    (0..clear.len())
        .map(|i| {
            if clear.get(i).copied().unwrap_or(false) {
                !in_void(i)
            } else {
                shade(i, white) && near(i).any(in_void)
            }
        })
        .collect()
}

/// Which square of the grid a pixel coordinate is in. The grid starts a pixel
/// before the minimap, where its traced edge can run.
fn cell_of(at: f32) -> i64 {
    ((at + 1.0) / CELL).floor() as i64
}

/// The edges of the `blocked` pixels as runs of points, in pixels. Each two by
/// two block of pixel middles adds the piece of edge crossing it, and pieces
/// that share an end are joined. Off the minimap counts as blocked.
fn trace(blocked: &[bool], [wide, tall]: [usize; 2]) -> Vec<Vec<Pos2>> {
    let solid = |x: i64, y: i64| {
        let (Ok(col), Ok(row)) = (usize::try_from(x), usize::try_from(y)) else {
            return true;
        };
        col >= wide || row >= tall || blocked.get(row * wide + col).is_none_or(|b| *b)
    };
    let mut pieces: Vec<[(i64, i64); 2]> = Vec::new();
    for y in -1..i64::try_from(tall).unwrap_or(0) {
        for x in -1..i64::try_from(wide).unwrap_or(0) {
            let corners = [
                solid(x, y),
                solid(x + 1, y),
                solid(x + 1, y + 1),
                solid(x, y + 1),
            ];
            // In half pixels, so the ends of pieces meet exactly.
            let (top, right) = ((2 * x + 2, 2 * y + 1), (2 * x + 3, 2 * y + 2));
            let (bottom, left) = ((2 * x + 2, 2 * y + 3), (2 * x + 1, 2 * y + 2));
            // Walls touching only at a corner are kept joined, so no ray
            // slips between them.
            let found: &[[(i64, i64); 2]] = match corners {
                [false, false, false, false] | [true, true, true, true] => &[],
                [false, false, false, true] | [true, true, true, false] => &[[left, bottom]],
                [false, false, true, false] | [true, true, false, true] => &[[bottom, right]],
                [false, false, true, true] | [true, true, false, false] => &[[left, right]],
                [false, true, false, false] | [true, false, true, true] => &[[top, right]],
                [false, true, true, false] | [true, false, false, true] => &[[top, bottom]],
                [false, true, true, true] | [true, false, false, false] => &[[top, left]],
                [false, true, false, true] => &[[top, left], [bottom, right]],
                [true, false, true, false] => &[[top, right], [left, bottom]],
            };
            pieces.extend_from_slice(found);
        }
    }
    join(&pieces)
}

/// `pieces` joined end to end into runs of points, in pixels.
fn join(pieces: &[[(i64, i64); 2]]) -> Vec<Vec<Pos2>> {
    let mut ends: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (index, piece) in pieces.iter().enumerate() {
        for end in piece {
            ends.entry(*end).or_default().push(index);
        }
    }
    let mut used = vec![false; pieces.len()];
    let mut runs = Vec::new();
    for (first, &[a, b]) in pieces.iter().enumerate() {
        if used.get(first).copied().unwrap_or(true) {
            continue;
        }
        if let Some(seen) = used.get_mut(first) {
            *seen = true;
        }
        let ahead = walk(pieces, &ends, &mut used, b);
        let mut run = walk(pieces, &ends, &mut used, a);
        run.reverse();
        run.extend([a, b]);
        run.extend(ahead);
        runs.push(
            run.into_iter()
                .map(|(x, y)| pos2(x as f32 / 2.0, y as f32 / 2.0))
                .collect(),
        );
    }
    runs
}

/// The ends met walking on from `at` through each unused piece in turn,
/// marking them used.
fn walk(
    pieces: &[[(i64, i64); 2]],
    ends: &HashMap<(i64, i64), Vec<usize>>,
    used: &mut [bool],
    mut at: (i64, i64),
) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    while let Some(next) = ends.get(&at).and_then(|list| {
        list.iter()
            .copied()
            .find(|i| !used.get(*i).copied().unwrap_or(true))
    }) {
        if let Some(seen) = used.get_mut(next) {
            *seen = true;
        }
        let Some(&[one, other]) = pieces.get(next) else {
            break;
        };
        at = if one == at { other } else { one };
        out.push(at);
    }
    out
}

/// `run` with every point that strays less than `STRAY` from a straight line
/// between its neighbours left out, so a wall is a few long lines rather than
/// a staircase of short ones.
fn straighten(run: &[Pos2]) -> Vec<Pos2> {
    let Some(&last) = run.last() else {
        return Vec::new();
    };
    let mut keep = vec![false; run.len()];
    if let Some(first) = keep.first_mut() {
        *first = true;
    }
    if let Some(end) = keep.last_mut() {
        *end = true;
    }
    let mut spans = vec![(0, run.len() - 1)];
    while let Some((from, to)) = spans.pop() {
        let (Some(&a), Some(&b)) = (run.get(from), run.get(to)) else {
            continue;
        };
        let farthest = (from + 1..to)
            .filter_map(|i| run.get(i).map(|p| (i, off_line(*p, a, b))))
            .max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((index, gap)) = farthest
            && gap > STRAY
        {
            if let Some(flag) = keep.get_mut(index) {
                *flag = true;
            }
            spans.push((from, index));
            spans.push((index, to));
        }
    }
    let mut out: Vec<Pos2> = run
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect();
    if out.len() < 2 {
        out.push(last);
    }
    out
}

/// How far `p` is from the line through `a` and `b`.
fn off_line(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let along = b - a;
    if along.length() <= 0.0 {
        return p.distance(a);
    }
    cross(along, p - a).abs() / along.length()
}

impl Walls {
    /// Where a turret put down at `at` stands: the middle of its pixel, or of
    /// the nearest one it can see from when `at` is in a wall, since a wall a
    /// few pixels thick is easy to drop it on, and from inside one it would
    /// see only a sliver along the inside of the wall.
    fn standing(&self, at: Pos2) -> Pos2 {
        let [wide, tall] = self.size;
        let (x, y) = (at.x.floor() as i64, at.y.floor() as i64);
        let open = |col: i64, row: i64| {
            (0..wide as i64).contains(&col)
                && (0..tall as i64).contains(&row)
                && usize::try_from(row * wide as i64 + col)
                    .ok()
                    .and_then(|i| self.blocked.get(i))
                    .is_some_and(|b| !*b)
        };
        let mut best: Option<(f32, Pos2)> = None;
        for row in y - STEP_OUT..=y + STEP_OUT {
            for col in x - STEP_OUT..=x + STEP_OUT {
                let middle = pos2(col as f32 + 0.5, row as f32 + 0.5);
                let gap = middle.distance_sq(at);
                if open(col, row) && best.is_none_or(|(nearest, _)| gap < nearest) {
                    best = Some((gap, middle));
                }
            }
        }
        best.map_or(at, |(_, middle)| middle)
    }

    /// What can be seen from `from` between the angles `low` and `high`, as
    /// the points round its edge from `low` to `high`, in the minimap's
    /// pixels. Nothing further than `far` is looked for.
    fn seen(&self, from: Pos2, (low, high): (f32, f32), far: f32) -> Vec<Pos2> {
        let wide = high - low;
        let mut turns = vec![low, high];
        for corner in &self.corners {
            let past = ((*corner - from).angle() - low).rem_euclid(std::f32::consts::TAU);
            if past < wide {
                turns.extend([low + past - PAST, low + past, low + past + PAST]);
            }
        }
        turns.retain(|t| (low..=high).contains(t));
        turns.sort_by(f32::total_cmp);
        turns
            .into_iter()
            .map(|turn| {
                let way = Vec2::angled(turn);
                from + way * self.cast(from, way, far)
            })
            .collect()
    }

    /// How far a ray from `from` along `way` goes before it meets a wall, up to
    /// `far`. It walks the grid square by square and stops once a wall it has
    /// met is nearer than the next square.
    fn cast(&self, from: Pos2, way: Vec2, far: f32) -> f32 {
        let mut best = far;
        let start = from + vec2(1.0, 1.0);
        let (mut col, mut row) = (cell_of(from.x), cell_of(from.y));
        let step = (
            if way.x > 0.0 { 1 } else { -1 },
            if way.y > 0.0 { 1 } else { -1 },
        );
        let edge = |at: f32, cell: i64, d: f32| {
            if d.abs() < f32::EPSILON {
                f32::INFINITY
            } else if d > 0.0 {
                ((cell + 1) as f32).mul_add(CELL, -at) / d
            } else {
                (cell as f32).mul_add(-CELL, at) / -d
            }
        };
        let (mut next_x, mut next_y) = (edge(start.x, col, way.x), edge(start.y, row, way.y));
        let (each_x, each_y) = ((CELL / way.x).abs(), (CELL / way.y).abs());
        while (0..self.cells.0).contains(&col) && (0..self.cells.1).contains(&row) {
            let square = usize::try_from(row * self.cells.0 + col)
                .ok()
                .and_then(|i| self.grid.get(i));
            for line in square
                .into_iter()
                .flatten()
                .filter_map(|i| self.lines.get(*i))
            {
                if let Some(t) = hit(from, way, *line) {
                    best = best.min(t);
                }
            }
            let leave = next_x.min(next_y);
            if best <= leave {
                break;
            }
            if next_x < next_y {
                (col, next_x) = (col + step.0, next_x + each_x);
            } else {
                (row, next_y) = (row + step.1, next_y + each_y);
            }
        }
        best
    }
}

/// The cross product of `a` and `b`, how far `b` turns from `a`.
fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x.mul_add(b.y, -(a.y * b.x))
}

/// How far along a ray from `from` along `way` it meets `line`, if it does.
fn hit(from: Pos2, way: Vec2, [a, b]: [Pos2; 2]) -> Option<f32> {
    let along = b - a;
    let across = cross(way, along);
    if across.abs() < 1e-9 {
        return None;
    }
    let gap = a - from;
    let t = cross(gap, along) / across;
    let s = cross(gap, way) / across;
    (t >= 0.0 && (-SNUG..=1.0 + SNUG).contains(&s)).then_some(t)
}

/// Where a turret at `at` on `square`, aimed at `aim`, stands and what it
/// sees in a cone `degrees` wide, as the points round its edge, on screen.
pub(super) fn cone(
    walls: &Walls,
    square: Square,
    (at, aim): (Pos2, Pos2),
    degrees: f32,
) -> (Pos2, Vec<Pos2>) {
    let [wide, tall] = walls.size;
    let to_image = |p: Pos2| {
        let shown = (p - square.min) / square.width();
        let [x, y] = shapes::turned([shown.x, shown.y], 4 - square.turn % 4);
        pos2(x * wide, y * tall)
    };
    let to_screen = |p: Pos2| {
        let [x, y] = shapes::turned([p.x / wide, p.y / tall], square.turn % 4);
        square.min + vec2(x, y) * square.width()
    };
    let from = walls.standing(to_image(at));
    let towards = (to_image(aim) - from).angle();
    let half = degrees.to_radians() / 2.0;
    let far = wide.hypot(tall) * 2.0;
    let rim = walls
        .seen(from, (towards - half, towards + half), far)
        .into_iter()
        .map(to_screen)
        .collect();
    (to_screen(from), rim)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A floor 64 pixels square with a clear block from 30 to 34 each way,
    /// and the turret at (10, 32) looking along it.
    fn room() -> Walls {
        let mut image = ColorImage::new([64, 64], vec![egui::Color32::WHITE; 64 * 64]);
        for y in 30..34 {
            for x in 30..34 {
                if let Some(p) = image.pixels.get_mut(y * 64 + x) {
                    *p = egui::Color32::TRANSPARENT;
                }
            }
        }
        walls(&image)
    }

    /// The block stops the ray straight at it at its face, its corners are
    /// on the edge of what is seen, and past them the floor runs on to the
    /// minimap's edge, with no ray left between.
    #[test]
    fn a_turret_sees_up_to_a_block_and_round_its_corners() {
        let walls = room();
        let from = pos2(10.0, 32.0);
        let ahead = walls.cast(from, vec2(1.0, 0.0), 500.0);
        assert!((ahead - 20.0).abs() <= STRAY, "{ahead}");
        let rim = walls.seen(from, (-0.5, 0.5), 500.0);
        for corner in [pos2(30.0, 30.0), pos2(30.0, 34.0)] {
            assert!(
                rim.iter().any(|p| p.distance(corner) < 0.7),
                "{corner:?} {rim:?}"
            );
        }
        assert!(
            rim.iter()
                .all(|p| p.x <= 64.5 && p.y >= -0.5 && p.y <= 64.5),
            "{rim:?}"
        );
    }

    /// A floor 64 pixels square with a clear patch from 20 to 44 each way,
    /// edged in `edge` and, on its far side at 44, in `far`.
    fn hole(edge: egui::Color32, far: egui::Color32) -> Walls {
        let mut image = ColorImage::new([64, 64], vec![egui::Color32::from_gray(118); 64 * 64]);
        for y in 19..45 {
            for x in 19..45 {
                let shade = if (20..44).contains(&x) && (20..44).contains(&y) {
                    egui::Color32::TRANSPARENT
                } else if x == 44 {
                    far
                } else {
                    edge
                };
                if let Some(p) = image.pixels.get_mut(y * 64 + x) {
                    *p = shade;
                }
            }
        }
        walls(&image)
    }

    /// A clear patch edged in a drop's grey is void, seen across to whatever
    /// is past it, a white wall standing at its edge still stops the view,
    /// and a patch edged in white all round is a wall.
    #[test]
    fn the_void_is_seen_across_but_not_a_wall_beside_it() {
        let grey = egui::Color32::from_gray(225);
        let from = pos2(10.5, 32.5);
        let ahead = vec2(1.0, 0.0);
        let across = hole(grey, grey).cast(from, ahead, 500.0);
        assert!(
            (across - 53.5).abs() <= STRAY,
            "to the minimap's edge, {across}"
        );
        let walled = hole(grey, egui::Color32::WHITE).cast(from, ahead, 500.0);
        assert!(
            (walled - 33.5).abs() <= STRAY,
            "to the wall past it, {walled}"
        );
        let solid = hole(egui::Color32::WHITE, egui::Color32::WHITE).cast(from, ahead, 500.0);
        assert!((solid - 9.5).abs() <= STRAY, "to the patch, {solid}");
    }

    /// A turret dropped inside a wall four pixels thick stands on the floor
    /// beside it and sees along it, where from inside it saw a sliver of the
    /// wall's own inside. One put down on the floor stands where it was put.
    #[test]
    fn a_turret_dropped_in_a_thin_wall_stands_beside_it() {
        let mut image = ColorImage::new([64, 64], vec![egui::Color32::from_gray(118); 64 * 64]);
        for y in 10..54 {
            for x in 30..34 {
                if let Some(p) = image.pixels.get_mut(y * 64 + x) {
                    *p = egui::Color32::TRANSPARENT;
                }
            }
        }
        let walls = walls(&image);
        assert_eq!(walls.standing(pos2(10.2, 10.9)), pos2(10.5, 10.5));
        let stands = walls.standing(pos2(31.2, 32.5));
        assert_eq!(stands, pos2(29.5, 32.5));
        let up = -std::f32::consts::FRAC_PI_2;
        let rim = walls.seen(stands, (up - 0.5, up + 0.5), 500.0);
        assert!(
            rim.iter().any(|p| p.y <= STRAY),
            "the top is out of sight {rim:?}"
        );
        let in_wall = |p: &&Pos2| (30.5..33.5).contains(&p.x) && (10.5..53.5).contains(&p.y);
        assert_eq!(rim.iter().find(in_wall), None);
    }

    /// Turning the cone a little moves only its edges, and every point in
    /// between stays exactly where it was.
    #[test]
    fn turning_moves_only_the_edges() {
        let walls = room();
        let from = pos2(10.0, 32.0);
        let before = walls.seen(from, (-0.5, 0.5), 500.0);
        let after = walls.seen(from, (-0.45, 0.55), 500.0);
        let inside: Vec<&Pos2> = after
            .iter()
            .skip(1)
            .take(after.len().saturating_sub(2))
            .collect();
        assert!(!inside.is_empty());
        for p in inside {
            let turn = (*p - from).angle();
            if turn < 0.5 - PAST {
                assert!(before.iter().any(|q| q.distance(*p) < 1e-4), "{p:?}");
            }
        }
    }
}
