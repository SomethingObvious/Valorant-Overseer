//! What an ability covers on the ground where a lineup throws it, drawn to
//! the map's scale. Sizes are in game units, a hundredth of a metre, from
//! Riot's wiki. An ability that covers no ground, like a dash or a gun, has
//! none, and its lineup is only its pins.

use egui::{Color32, Pos2, Shape, Stroke, Vec2};

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
    /// A trip wire across the narrowest gap through where it's put, from wall
    /// to wall, when that gap is no longer than this.
    Wire(f32),
    /// A wall through both pins from one side of the map to the other.
    Divide,
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
    ("Gravity Well", 475.0, 0.0),
    ("Nova Pulse", 475.0, 0.0),
    ("Aftershock", 300.0, 0.0),
    ("Meddle", 400.0, 0.0),
    ("GravNet", 650.0, 0.0),
    ("Seize", 658.0, 0.0),
    ("Thrash", 500.0, 0.0),
    ("Storm Surge", 600.0, 0.0),
    ("Relay Bolt", 500.0, 0.0),
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
    ("Fast Lane", 0.0, 4500.0, 350.0),
    ("Toxic Screen", 0.0, 6000.0, 0.0),
    ("High Tide", 0.0, 6000.0, 0.0),
    ("Blaze", 0.0, 2100.0, 0.0),
];

/// The ground `ability` covers, matched by name however it is spaced or
/// capitalised, since the game's own list has a double space in Nebula's.
pub(super) fn reach(ability: &str) -> Option<Reach> {
    let same = |name: &str| {
        let words = |s: &str| {
            s.split_whitespace()
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        };
        words(name) == words(ability)
    };
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
/// at `units` points to a game unit, finding walls with `run`. Anything
/// that goes the way it was thrown needs both pins apart, and isn't drawn
/// until it has them.
pub(super) fn draw(
    painter: &egui::Painter,
    at: (Option<Pos2>, Pos2),
    reach: (Reach, f32),
    (ink, run): (Color32, Run<'_>),
) {
    painter.extend(outline(at, reach, (ink, run)));
}

/// The shapes `draw` paints.
fn outline(
    (stand, land): (Option<Pos2>, Pos2),
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
            let mut out = vec![
                Shape::circle_filled(land, outer * units, fill),
                Shape::circle_stroke(land, outer * units, edge),
            ];
            if let Some(inner) = inner {
                out.push(Shape::circle_stroke(
                    land,
                    inner * units,
                    Stroke::new(1.0, ink.gamma_multiply(0.5)),
                ));
            }
            out
        }
        (Reach::Wire(length), _) => wire(land, length * units, run, line),
        (Reach::Cross(arm), _) => {
            let towards = ahead.map_or(0.0, Vec2::angle);
            let most = arm * units;
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
            let from = land - ahead;
            let (towards, half) = (ahead.angle(), degrees.to_radians() / 2.0);
            let mut points = vec![from];
            points.extend((0..=16_u8).map(|i| {
                let turn = (f32::from(i) / 16.0).mul_add(2.0 * half, towards - half);
                from + Vec2::angled(turn) * far * units
            }));
            vec![Shape::convex_polygon(points, fill, edge)]
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
        (Reach::Divide, Some(ahead)) => {
            let far = ENDLESS * units;
            vec![band(land, ahead.normalized(), (-far, far, 0.0), line)]
        }
        _ => Vec::new(),
    }
}

/// A trip wire through `at` across the narrowest gap between two walls no
/// wider than `most` points, tried every 5 degrees. Nothing when no gap that
/// narrow goes through it.
fn wire(at: Pos2, most: f32, run: Run<'_>, line: (Color32, Stroke)) -> Vec<Shape> {
    (0..36_u8)
        .filter_map(|i| {
            let way = Vec2::angled((f32::from(i) * 5.0).to_radians());
            let (on, back) = (run(at, way, most)?, run(at, -way, most)?);
            let span = on + back;
            (span > 1.0 && span <= most).then_some((span, way, on, back))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, way, on, back)| vec![band(at, way, (-back, on, 0.0), line)])
        .unwrap_or_default()
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
    fn same(found: &[Pos2], wanted: &[[f32; 2]]) -> bool {
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
            at,
            (Reach::Strip(800.0, 5600.0, 800.0), 0.01),
            ink,
        ));
        assert!(
            same(
                &strip,
                &[[8.0, -4.0], [64.0, -4.0], [64.0, 4.0], [8.0, 4.0]]
            ),
            "{strip:?}"
        );
        let sweep = corners(&outline(at, (Reach::Sweep(3200.0, 1200.0), 0.01), ink));
        assert!(
            same(
                &sweep,
                &[[100.0, -6.0], [132.0, -6.0], [132.0, 6.0], [100.0, 6.0]]
            ),
            "{sweep:?}"
        );
        let wall = corners(&outline(at, (Reach::Across(1040.0), 0.01), ink));
        assert!(
            same(
                &wall,
                &[[98.5, 5.2], [98.5, -5.2], [101.5, -5.2], [101.5, 5.2]]
            ),
            "{wall:?}"
        );
    }

    /// A trip goes straight across the corridor it's put in, wall to wall,
    /// whichever way it was placed from, and isn't drawn in open floor.
    #[test]
    fn a_trip_spans_the_gap_it_is_put_in() {
        let walled: (Color32, Run<'_>) = (Color32::WHITE, &corridor);
        let trip = corners(&outline(
            (Some(pos2(0.0, 0.0)), pos2(100.0, 0.0)),
            (Reach::Wire(1500.0), 0.01),
            walled,
        ));
        assert_eq!(trip.len(), 4, "{trip:?}");
        assert!(
            trip.iter()
                .all(|p| (p.x - 100.0).abs() <= THINNEST / 2.0 + 1e-3),
            "{trip:?}"
        );
        assert!(
            trip.iter().all(|p| (p.y.abs() - 5.0).abs() < 1e-3),
            "{trip:?}"
        );
        let floor: (Color32, Run<'_>) = (Color32::WHITE, &open);
        assert!(outline((None, pos2(0.0, 0.0)), (Reach::Wire(1500.0), 0.01), floor).is_empty());
    }

    /// Barrier Mesh is an X of four walls at 45 degrees to the throw, each
    /// cut short by a wall or else 10 metres long.
    #[test]
    fn a_mesh_is_an_x_cut_short_by_walls() {
        let at = (Some(pos2(0.0, 0.0)), pos2(100.0, 0.0));
        let floor: (Color32, Run<'_>) = (Color32::WHITE, &open);
        let mesh = outline(at, (Reach::Cross(1000.0), 0.01), floor);
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
        let short = corners(&outline(at, (Reach::Cross(1000.0), 0.01), walled));
        assert!(
            short.iter().all(|p| p.y.abs() <= 5.0 + THINNEST),
            "{short:?}"
        );
    }
}
