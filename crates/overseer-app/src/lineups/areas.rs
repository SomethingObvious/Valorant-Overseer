//! What an ability covers on the ground where a lineup throws it, drawn to
//! the map's scale. Sizes are in game units, a hundredth of a metre, from
//! Riot's wiki. An ability that covers no ground, like a dash or a gun, has
//! none, and its lineup is only its pins.

use egui::{Color32, Pos2, Shape, Stroke, Vec2};
use overseer_ui::colour;

/// The ground an ability covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Reach {
    /// A circle where it lands: its edge, and the inner circle where it does
    /// full damage when there is one.
    Round(f32, Option<f32>),
    /// A strip out from where it's thrown toward where it lands: how far out
    /// it starts, how long it runs and how wide it is.
    Strip(f32, f32, f32),
    /// A fan out from where it's thrown toward where it lands: how far it
    /// reaches and how wide it opens, in degrees.
    Fan(f32, f32),
    /// A strip from where it lands on in the way it was thrown: how long it
    /// runs and how wide it is.
    Sweep(f32, f32),
    /// A wall this long standing across the throw where it lands.
    Across(f32),
    /// Four walls out from where it lands in an X across the throw, each
    /// running until it meets a wall or reaches this far.
    Cross(f32),
    /// A trip wire between its two ends, which can be this far apart.
    Wire(f32),
    /// A wall from where it's cast to where it ends, bent through one point
    /// halfway along, and cut off at this long.
    Bent(f32),
    /// A wall through both pins from one side of the map to the other.
    Divide,
    /// Two lines out from where it's cast toward where it lands, this far
    /// apart, each running this far behind and this far ahead or until it
    /// meets a wall.
    Lanes(f32, f32, f32),
    /// A cone this many degrees wide out from where it lands, pointing at
    /// the lineup's point, as far as it can see before the walls.
    Aim(f32),
    /// A circle this big where it lands and another where it bounces, which
    /// is the lineup's point.
    Bounce(f32),
}

/// Circles where they land: the edge, and the inner circle, 0 for none.
const ROUND: &[(&str, f32, f32)] = &[
    ("Incendiary", 450.0, 0.0),
    ("Snake Bite", 450.0, 0.0),
    ("Nanoswarm", 450.0, 0.0),
    ("Hot Hands", 450.0, 0.0),
    ("Poison Cloud", 450.0, 0.0),
    ("FRAG/ment", 400.0, 100.0),
    ("Mosh Pit", 620.0, 550.0),
    ("Paint Shells", 550.0, 250.0),
    ("Showstopper", 700.0, 400.0),
    ("Blast Pack", 500.0, 200.0),
    ("Shock Bolt", 400.0, 150.0),
    ("Special Delivery", 525.0, 125.0),
    ("Guided Salvo", 450.0, 250.0),
    ("Orbital Strike", 900.0, 0.0),
    ("Sky Smoke", 415.0, 0.0),
    ("Dark Cover", 410.0, 0.0),
    ("Nebula / Dissipate", 475.0, 0.0),
    ("Ruse", 400.0, 0.0),
    ("Cyber Cage", 372.0, 0.0),
    ("Cove", 460.0, 0.0),
    ("Cloudburst", 335.0, 0.0),
    ("Waveform", 472.0, 0.0),
    ("Viper's Pit", 900.0, 0.0),
    ("Stim Beacon", 600.0, 0.0),
    ("Slow Orb", 800.0, 0.0),
    ("Gravity Well", 475.0, 0.0),
    ("Nova Pulse", 475.0, 0.0),
    ("Aftershock", 300.0, 0.0),
    ("Meddle", 400.0, 0.0),
    ("GravNet", 650.0, 0.0),
    ("Seize", 658.0, 0.0),
    ("Thrash", 500.0, 0.0),
    ("Storm Surge", 600.0, 0.0),
    ("M-pulse", 550.0, 0.0),
    ("Saturate", 600.0, 0.0),
    ("Razorvine", 625.0, 0.0),
    ("Chokehold", 658.0, 0.0),
    ("Trademark", 600.0, 0.0),
    ("Tour De Force", 600.0, 0.0),
    ("ALARMBOT", 550.0, 0.0),
    ("ZERO/point", 1500.0, 0.0),
    ("Regrowth", 1800.0, 0.0),
    ("Rendezvous", 1800.0, 0.0),
    ("Interceptor", 1800.0, 0.0),
    ("Steel Garden", 2800.0, 0.0),
    ("Haunt", 3000.0, 0.0),
    ("Recon Bolt", 3000.0, 0.0),
    ("Lockdown", 3250.0, 0.0),
    ("NULL/cmd", 4250.0, 0.0),
];

/// Strips out from where they're thrown: where they start, how long they
/// run and how wide they are. A wall the wiki gives no thickness for is 0,
/// and drawn as a line.
const STRIP: &[(&str, f32, f32, f32)] = &[
    ("Fault Line", 800.0, 5600.0, 800.0),
    ("Rolling Thunder", 800.0, 3200.0, 1800.0),
    ("Reckoning", 800.0, 3400.0, 2100.0),
    ("Convergent Paths", 300.0, 3600.0, 1350.0),
    ("Paranoia", 0.0, 2500.0, 860.0),
    ("Nightfall", 0.0, 4000.0, 2000.0),
    ("Annihilation", 0.0, 4000.0, 600.0),
    ("Undercut", 0.0, 3487.5, 600.0),
    ("Kill Contract", 0.0, 3600.0, 1500.0),
    ("Hunter's Fury", 0.0, 6600.0, 352.0),
    ("Toxic Screen", 0.0, 6000.0, 0.0),
];

/// Abilities put down from afar rather than thrown: how many places one
/// lineup can put them in all, and how far from where you stand they reach,
/// with no limit for Astra's stars, which go anywhere on the map.
const SPOTS: &[(&str, usize, Option<f32>)] = &[
    ("Sky Smoke", 3, Some(5500.0)),
    ("Dark Cover", 2, Some(8000.0)),
    ("Ruse", 2, Some(6000.0)),
    ("Guided Salvo", 2, Some(4500.0)),
    ("Orbital Strike", 1, Some(6000.0)),
    ("Armageddon", 1, Some(5250.0)),
    ("Nebula / Dissipate", 5, None),
    ("Gravity Well", 5, None),
    ("Nova Pulse", 5, None),
];

/// Whether two ability names are the same however they are spaced or
/// capitalised, since the game's own list has a double space in Nebula's.
fn same(a: &str, b: &str) -> bool {
    let words = |s: &str| {
        s.split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    words(a) == words(b)
}

/// How many places `ability` can be put down in one lineup, and how far
/// from where you stand, for one put down from afar.
pub(super) fn spots(ability: Option<&str>) -> Option<(usize, Option<f32>)> {
    let ability = ability?;
    SPOTS
        .iter()
        .find(|r| same(r.0, ability))
        .map(|&(_, most, range)| (most, range))
}

/// The ground `ability` covers, matched by name however it is spaced.
pub(super) fn reach(ability: &str) -> Option<Reach> {
    let same = |name: &str| same(name, ability);
    if let Some(&(_, edge, inner)) = ROUND.iter().find(|r| same(r.0)) {
        return Some(Reach::Round(edge, (inner > 0.0).then_some(inner)));
    }
    if let Some(&(_, start, length, wide)) = STRIP.iter().find(|r| same(r.0)) {
        return Some(Reach::Strip(start, length, wide));
    }
    [
        ("Bassquake", Reach::Fan(4000.0, 60.0)),
        ("Barrier Orb", Reach::Across(1040.0)),
        ("Barrier Mesh", Reach::Cross(1000.0)),
        ("Trapwire", Reach::Wire(1500.0)),
        ("High Tide", Reach::Bent(6000.0)),
        ("Fast Lane", Reach::Lanes(150.0, 4500.0, 350.0)),
        ("TURRET", Reach::Aim(100.0)),
        ("Relay Bolt", Reach::Bounce(500.0)),
        ("Blaze", Reach::Bent(2100.0)),
        ("Armageddon", Reach::Sweep(3200.0, 1200.0)),
        ("Astral Form / Cosmic Divide", Reach::Divide),
    ]
    .into_iter()
    .find(|r| same(r.0))
    .map(|r| r.1)
}

/// The thinnest a strip or wall is drawn, in points, so a metre-wide wall
/// still shows on a small map.
const THINNEST: f32 = 3.0;

/// How far the floor runs from a point along a way before it meets a wall,
/// in points, when it does within the most given.
pub(super) type Run<'a> = &'a dyn Fn(Pos2, Vec2, f32) -> Option<f32>;

/// How far each way Cosmic Divide is drawn, in game units. It has no end, and
/// this is past the edge of every map.
const ENDLESS: f32 = 50_000.0;

/// Draws `reach` for a lineup thrown from `stand` that comes down at `land`,
/// with its own `points`, at `units` points to a game unit, finding walls
/// with `run`. Anything that goes the way it was thrown needs both pins
/// apart, and isn't drawn until it has them.
pub(super) fn draw(
    painter: &egui::Painter,
    at: (Option<Pos2>, Pos2, &[Pos2]),
    reach: (Reach, f32),
    (ink, run): (Color32, Run<'_>),
) {
    painter.extend(outline(at, reach, (ink, run)));
}

/// The shapes `draw` paints.
fn outline(
    (stand, land, points): (Option<Pos2>, Pos2, &[Pos2]),
    (reach, units): (Reach, f32),
    (ink, run): (Color32, Run<'_>),
) -> Vec<Shape> {
    let (fill, edge) = (
        ink.gamma_multiply(0.14),
        Stroke::new(1.5, ink.gamma_multiply(0.7)),
    );
    let line = (edge.color, edge);
    let ahead = stand.map(|s| land - s).filter(|d| d.length() > 1.0);
    match (reach, ahead) {
        (Reach::Round(outer, inner), _) => {
            round(land, (outer * units, inner.map(|i| i * units)), ink)
        }
        (Reach::Wire(length), _) => match points {
            [a, b, ..] if a.distance(*b) > 0.0 => wire((*a, *b), length * units, run, line),
            _ => Vec::new(),
        },
        (Reach::Bent(length), _) => stand.map_or_else(Vec::new, |from| {
            let path = bent(from, points.first().copied(), land, length * units);
            vec![Shape::line(path, Stroke::new(THINNEST, edge.color))]
        }),
        (Reach::Cross(arm), _) => cross(land, ahead, arm * units, (line, run)),
        (Reach::Strip(start, length, wide), Some(ahead)) => vec![band(
            land - ahead,
            ahead.normalized(),
            (start * units, (start + length) * units, wide * units),
            (fill, edge),
        )],
        (Reach::Sweep(length, wide), Some(ahead)) => vec![band(
            land,
            ahead.normalized(),
            (0.0, length * units, wide * units),
            (fill, edge),
        )],
        (Reach::Fan(far, degrees), Some(ahead)) => {
            vec![fan(
                land - ahead,
                ahead,
                (far * units, degrees),
                (fill, edge),
            )]
        }
        (Reach::Across(length), Some(ahead)) => {
            let half = length * units / 2.0;
            vec![band(
                land,
                ahead.normalized().rot90(),
                (-half, half, 150.0 * units),
                (fill, edge),
            )]
        }
        (Reach::Bounce(radius), _) => points
            .first()
            .into_iter()
            .chain([&land])
            .flat_map(|at| round(*at, (radius * units, None), ink))
            .collect(),
        (Reach::Aim(degrees), _) => points
            .first()
            .filter(|p| p.distance(land) > 0.0)
            .map_or_else(Vec::new, |p| {
                sight(
                    (land, *p - land),
                    (degrees, ENDLESS * units),
                    run,
                    (fill, edge),
                )
            }),
        (Reach::Lanes(behind, far, apart), Some(ahead)) => lanes(
            (land - ahead, ahead),
            (behind * units, far * units, apart * units),
            (line, run),
        ),
        (Reach::Divide, Some(ahead)) => {
            let far = ENDLESS * units;
            vec![band(land, ahead.normalized(), (-far, far, 0.0), line)]
        }
        _ => Vec::new(),
    }
}

/// The points along a wall from `from` to `to` that goes through `bend`
/// halfway along, or straight without one, cut off `most` along it. It is a
/// curve with its pull set so that it meets `bend` at its middle, which is
/// where the point dragged to bend it should be.
pub(super) fn bent(from: Pos2, bend: Option<Pos2>, to: Pos2, most: f32) -> Vec<Pos2> {
    let middle = from.lerp(to, 0.5);
    let pull = bend.map_or(middle, |b| b + (b - middle));
    let mut out = vec![from];
    let mut gone = 0.0;
    for i in 1..=STEPS {
        let t = f32::from(i) / f32::from(STEPS);
        let u = 1.0 - t;
        let next =
            (from.to_vec2() * (u * u) + pull.to_vec2() * (2.0 * u * t) + to.to_vec2() * (t * t))
                .to_pos2();
        let last = out.last().copied().unwrap_or(from);
        let step = last.distance(next);
        if step > 0.0 && gone + step >= most {
            out.push(last + (next - last) * ((most - gone) / step));
            return out;
        }
        gone += step;
        out.push(next);
    }
    out
}

/// How many straight pieces a bent wall is drawn in.
const STEPS: u8 = 32;

/// A circle `outer` round at `land`, with the inner circle of full damage
/// when there is one.
fn round(land: Pos2, (outer, inner): (f32, Option<f32>), ink: Color32) -> Vec<Shape> {
    let edge = Stroke::new(1.5, ink.gamma_multiply(0.7));
    let mut out = vec![
        Shape::circle_filled(land, outer, ink.gamma_multiply(0.14)),
        Shape::circle_stroke(land, outer, edge),
    ];
    if let Some(inner) = inner {
        out.push(Shape::circle_stroke(
            land,
            inner,
            Stroke::new(1.0, ink.gamma_multiply(0.5)),
        ));
    }
    out
}

/// Barrier Mesh's four walls out from `land` in an X across the way it was
/// thrown, each stopping at a wall or at `most` points.
fn cross(
    land: Pos2,
    ahead: Option<Vec2>,
    most: f32,
    (line, run): ((Color32, Stroke), Run<'_>),
) -> Vec<Shape> {
    let towards = ahead.map_or(0.0, Vec2::angle);
    (0..4_u8)
        .map(|i| {
            let turn = f32::from(i).mul_add(90.0, 45.0).to_radians();
            let way = Vec2::angled(towards + turn);
            band(
                land,
                way,
                (0.0, run(land, way, most).unwrap_or(most), 0.0),
                line,
            )
        })
        .collect()
}

/// Fast Lane's two lines from `from` toward `ahead`, `apart` points apart,
/// each running `behind` back and `far` on, or until it meets a wall.
fn lanes(
    (from, ahead): (Pos2, Vec2),
    (behind, far, apart): (f32, f32, f32),
    (line, run): ((Color32, Stroke), Run<'_>),
) -> Vec<Shape> {
    let way = ahead.normalized();
    let side = way.rot90() * (apart / 2.0);
    [from + side, from - side]
        .into_iter()
        .map(|start| {
            let back = run(start, -way, behind).unwrap_or(behind);
            let on = run(start, way, far).unwrap_or(far);
            band(start, way, (-back, on, 0.0), line)
        })
        .collect()
}

/// A trip wire through its ends `a` and `b`, run on out to the wall each
/// side when the whole of it fits in `most` points and no wall stands between
/// the ends. Otherwise it can't be put there, and the part between the ends
/// is red and dashed.
fn wire((a, b): (Pos2, Pos2), most: f32, run: Run<'_>, line: (Color32, Stroke)) -> Vec<Shape> {
    let (way, between) = ((b - a).normalized(), a.distance(b));
    let spare = most - between;
    // A wall between the ends blocks it too. An end put on a wall line meets
    // it just short of the end, which is fine.
    let clear = run(a, way, between).is_none_or(|t| t >= 2.0f32.mul_add(-THINNEST, between));
    match (run(a, -way, spare), run(b, way, spare)) {
        (Some(back), Some(on)) if clear && back + on <= spare => {
            vec![band(a, way, (-back, between + on, 0.0), line)]
        }
        _ => Shape::dashed_line(&[a, b], Stroke::new(THINNEST, colour::ENEMY), 6.0, 4.0),
    }
}

/// How many rays a turret's sight is traced in.
const RAYS: u8 = 96;

/// What a turret at `from` facing `ahead` sees in a cone `degrees` wide:
/// each ray runs until it meets a wall, or `far` points when none comes. It
/// shoots at any range it can see, so there's no other limit.
fn sight(
    (from, ahead): (Pos2, Vec2),
    (degrees, far): (f32, f32),
    run: Run<'_>,
    (fill, edge): (Color32, Stroke),
) -> Vec<Shape> {
    let (towards, half) = (ahead.angle(), degrees.to_radians() / 2.0);
    let ends: Vec<Pos2> = (0..=RAYS)
        .map(|i| {
            let way =
                Vec2::angled((f32::from(i) / f32::from(RAYS)).mul_add(2.0 * half, towards - half));
            from + way * run(from, way, far).unwrap_or(far)
        })
        .collect();
    // One mesh with no softened edges, since softening each thin slice on
    // its own leaves spikes where they meet. The rim line smooths the edge.
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(from, fill);
    for end in &ends {
        mesh.colored_vertex(*end, fill);
    }
    for i in 1..=u32::from(RAYS) {
        mesh.add_triangle(0, i, i + 1);
    }
    let mut rim = vec![from];
    rim.extend(ends);
    vec![Shape::mesh(mesh), Shape::closed_line(rim, edge)]
}

/// A cone out from `from` toward `ahead`, `far` points long and `degrees`
/// wide.
fn fan(
    from: Pos2,
    ahead: Vec2,
    (far, degrees): (f32, f32),
    (fill, edge): (Color32, Stroke),
) -> Shape {
    let (towards, half) = (ahead.angle(), degrees.to_radians() / 2.0);
    let mut points = vec![from];
    points.extend((0..=16_u8).map(|i| {
        let turn = (f32::from(i) / 16.0).mul_add(2.0 * half, towards - half);
        from + Vec2::angled(turn) * far
    }));
    Shape::convex_polygon(points, fill, edge)
}

/// A rectangle out from `origin` along `way`, from `near` to `far` points
/// out and `wide` points across.
fn band(
    origin: Pos2,
    way: Vec2,
    (near, far, wide): (f32, f32, f32),
    (fill, edge): (Color32, Stroke),
) -> Shape {
    let side = way.rot90() * wide.max(THINNEST) / 2.0;
    let (a, b) = (origin + way * near, origin + way * far);
    Shape::convex_polygon(vec![a + side, b + side, b - side, a - side], fill, edge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    /// Sizes come back as the game has them, by name however it is spaced.
    #[test]
    fn every_kind_of_area_reads_from_its_name() {
        assert_eq!(reach("Incendiary"), Some(Reach::Round(450.0, None)));
        assert_eq!(reach("FRAG/ment"), Some(Reach::Round(400.0, Some(100.0))));
        assert_eq!(
            reach("Nebula  / Dissipate"),
            Some(Reach::Round(475.0, None))
        );
        assert_eq!(
            reach("fault line"),
            Some(Reach::Strip(800.0, 5600.0, 800.0))
        );
        assert_eq!(reach("Trapwire"), Some(Reach::Wire(1500.0)));
        assert_eq!(reach("Bassquake"), Some(Reach::Fan(4000.0, 60.0)));
        assert_eq!(reach("Astral Form / Cosmic Divide"), Some(Reach::Divide));
        assert_eq!(reach("Barrier Mesh"), Some(Reach::Cross(1000.0)));
        assert_eq!(reach("High Tide"), Some(Reach::Bent(6000.0)));
        assert_eq!(spots(Some("Sky Smoke")), Some((3, Some(5500.0))));
        assert_eq!(spots(Some("Nebula  / Dissipate")), Some((5, None)));
        assert_eq!(spots(Some("Incendiary")), None);
        assert_eq!(reach("Updraft"), None);
    }

    /// The corners of every polygon drawn.
    fn corners(shapes: &[Shape]) -> Vec<Pos2> {
        shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Path(path) => Some(path.points.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Whether two lists of corners are the same, past float noise.
    fn same_corners(found: &[Pos2], wanted: &[[f32; 2]]) -> bool {
        found.len() == wanted.len()
            && found
                .iter()
                .zip(wanted)
                .all(|(f, w)| (f.x - w[0]).abs() < 1e-3 && (f.y - w[1]).abs() < 1e-3)
    }

    /// Open floor everywhere.
    fn open(_: Pos2, _: Vec2, _: f32) -> Option<f32> {
        None
    }

    /// A corridor 10 points wide, between walls along y = -5 and y = 5.
    fn corridor(from: Pos2, way: Vec2, most: f32) -> Option<f32> {
        if way.y.abs() < 1e-6 {
            return None;
        }
        let wall = if way.y > 0.0 { 5.0 } else { -5.0 };
        Some((wall - from.y) / way.y).filter(|t| *t <= most)
    }

    /// A strip runs out from where it's thrown, Armageddon on from where it
    /// lands, and a wall stands across, each the size the game says, here at
    /// a point to a metre.
    #[test]
    fn each_shape_points_the_way_it_is_thrown() {
        let at = (Some(pos2(0.0, 0.0)), pos2(100.0, 0.0));
        let ink: (Color32, Run<'_>) = (Color32::WHITE, &open);
        let strip = corners(&outline(
            (at.0, at.1, &[]),
            (Reach::Strip(800.0, 5600.0, 800.0), 0.01),
            ink,
        ));
        assert!(
            same_corners(
                &strip,
                &[[8.0, -4.0], [64.0, -4.0], [64.0, 4.0], [8.0, 4.0]]
            ),
            "{strip:?}"
        );
        let sweep = corners(&outline(
            (at.0, at.1, &[]),
            (Reach::Sweep(3200.0, 1200.0), 0.01),
            ink,
        ));
        assert!(
            same_corners(
                &sweep,
                &[[100.0, -6.0], [132.0, -6.0], [132.0, 6.0], [100.0, 6.0]]
            ),
            "{sweep:?}"
        );
        let wall = corners(&outline(
            (at.0, at.1, &[]),
            (Reach::Across(1040.0), 0.01),
            ink,
        ));
        assert!(
            same_corners(
                &wall,
                &[[98.5, 5.2], [98.5, -5.2], [101.5, -5.2], [101.5, 5.2]]
            ),
            "{wall:?}"
        );
    }

    /// A trip runs on from its two ends to the walls either side, and one
    /// that can't reach a wall each way within its 15 metres is red and
    /// dashed between its ends.
    #[test]
    fn a_trip_reaches_the_walls_or_turns_red() {
        let ends = [pos2(100.0, -2.0), pos2(100.0, 2.0)];
        let walled: (Color32, Run<'_>) = (Color32::WHITE, &corridor);
        let trip = outline(
            (None, pos2(100.0, 0.0), &ends),
            (Reach::Wire(1500.0), 0.01),
            walled,
        );
        let span = corners(&trip);
        assert!(
            same_corners(
                &span,
                &[[101.5, -5.0], [101.5, 5.0], [98.5, 5.0], [98.5, -5.0]]
            ),
            "{span:?}"
        );
        // Nine metres of wire can't cross a ten metre corridor.
        let short = outline(
            (None, pos2(100.0, 0.0), &ends),
            (Reach::Wire(900.0), 0.01),
            walled,
        );
        assert!(
            short.iter().all(
                |s| matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == colour::ENEMY)
            ),
            "{short:?}"
        );
        let floor: (Color32, Run<'_>) = (Color32::WHITE, &open);
        let loose = outline(
            (None, pos2(100.0, 0.0), &ends),
            (Reach::Wire(1500.0), 0.01),
            floor,
        );
        assert!(
            !loose.is_empty() && loose.iter().all(|s| matches!(s, Shape::LineSegment { .. })),
            "{loose:?}"
        );
        assert!(
            outline(
                (None, pos2(0.0, 0.0), &[]),
                (Reach::Wire(1500.0), 0.01),
                floor
            )
            .is_empty()
        );
    }

    /// A wall bent through a point goes through it halfway along, and one
    /// longer than the game allows stops at the most it can be.
    #[test]
    fn a_bent_wall_meets_its_bend_and_stops_at_its_length() {
        let (from, to) = (pos2(0.0, 0.0), pos2(20.0, 0.0));
        let path = bent(from, Some(pos2(10.0, 5.0)), to, 100.0);
        let halfway = path.get(usize::from(STEPS >> 1)).copied().unwrap_or(from);
        assert!(halfway.distance(pos2(10.0, 5.0)) < 1e-3, "{halfway:?}");
        assert!(
            path.last().is_some_and(|p| p.distance(to) < 1e-3),
            "{path:?}"
        );
        let cut = bent(from, None, to, 12.5);
        let long: f32 = cut
            .iter()
            .zip(cut.iter().skip(1))
            .map(|(a, b)| a.distance(*b))
            .sum();
        assert!((long - 12.5).abs() < 1e-3, "{long}");
        assert!(
            cut.last()
                .is_some_and(|p| p.distance(pos2(12.5, 0.0)) < 1e-3),
            "{cut:?}"
        );
    }

    /// Fast Lane's two lines stop where they meet a wall, Relay Bolt splashes
    /// where it bounces and where it lands, and the turret's cone points at
    /// its dot.
    #[test]
    fn lanes_stop_at_walls_and_a_bolt_splashes_twice() {
        let at = (
            Some(pos2(0.0, 0.0)),
            pos2(10.0, 0.0),
            &[pos2(10.0, 20.0)][..],
        );
        let short = |_: Pos2, way: Vec2, _: f32| (way.x > 0.0).then_some(20.0);
        let lanes = corners(&outline(
            at,
            (Reach::Lanes(150.0, 4500.0, 350.0), 0.01),
            (Color32::WHITE, &short),
        ));
        assert_eq!(lanes.len(), 8, "{lanes:?}");
        assert!(
            lanes
                .iter()
                .all(|p| (-1.5 - 1e-3..=20.0 + 1e-3).contains(&p.x)),
            "{lanes:?}"
        );
        let floor: (Color32, Run<'_>) = (Color32::WHITE, &open);
        let bolt = outline(at, (Reach::Bounce(500.0), 0.01), floor);
        assert_eq!(bolt.len(), 4);
        // With no wall it sees to the end of the map and past, straight at
        // its dot, and a wall stops each ray at it.
        let cone = outline(at, (Reach::Aim(100.0), 0.01), floor);
        let rim = cone.last().and_then(|s| match s {
            Shape::Path(path) => path.points.get(usize::from((RAYS >> 1) + 1)).copied(),
            _ => None,
        });
        let middle = rim.unwrap_or_default();
        assert!(
            (middle.x - 10.0).abs() < 1e-3 && middle.y >= 500.0 - 1e-2,
            "{middle:?}"
        );
        let walled: (Color32, Run<'_>) = (Color32::WHITE, &corridor);
        let boxed = outline(at, (Reach::Aim(100.0), 0.01), walled);
        let rim = match boxed.last() {
            Some(Shape::Path(path)) => path.points.clone(),
            _ => Vec::new(),
        };
        assert!(rim.iter().all(|p| p.y <= 5.0 + 1e-3), "{rim:?}");
    }

    /// Barrier Mesh is an X of four walls at 45 degrees to the throw, each
    /// cut short by a wall or else 10 metres long.
    #[test]
    fn a_mesh_is_an_x_cut_short_by_walls() {
        let at = (Some(pos2(0.0, 0.0)), pos2(100.0, 0.0));
        let floor: (Color32, Run<'_>) = (Color32::WHITE, &open);
        let mesh = outline((at.0, at.1, &[]), (Reach::Cross(1000.0), 0.01), floor);
        assert_eq!(mesh.len(), 4);
        let reach: Vec<f32> = corners(&mesh)
            .iter()
            .map(|p| p.distance(pos2(100.0, 0.0)))
            .collect();
        let longest = reach.iter().copied().fold(0.0, f32::max);
        assert!(
            (longest - 10.0_f32.hypot(THINNEST / 2.0)).abs() < 1e-3,
            "{reach:?}"
        );
        let walled: (Color32, Run<'_>) = (Color32::WHITE, &corridor);
        let short = corners(&outline(
            (at.0, at.1, &[]),
            (Reach::Cross(1000.0), 0.01),
            walled,
        ));
        assert!(
            short.iter().all(|p| p.y.abs() <= 5.0 + THINNEST),
            "{short:?}"
        );
    }
}
