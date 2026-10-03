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
    /// A wall this long standing across the throw where it lands.
    Across(f32),
    /// A trip wire from where it's stuck back toward where it was placed
    /// from, at most this long, since that is the way the wall it's on faces.
    Wire(f32),
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
/// run and how wide they are. A wall is a metre wide.
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
    ("Toxic Screen", 0.0, 6000.0, 100.0),
    ("High Tide", 0.0, 6000.0, 100.0),
    ("Blaze", 0.0, 2100.0, 100.0),
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
        ("Barrier Mesh", Reach::Across(2000.0)),
        ("Trapwire", Reach::Wire(1500.0)),
    ]
    .into_iter()
    .find(|r| same(r.0))
    .map(|r| r.1)
}

/// The thinnest a strip or wall is drawn, in points, so a metre-wide wall
/// still shows on a small map.
const THINNEST: f32 = 3.0;

/// Draws `reach` for a lineup thrown from `stand` that comes down at `land`,
/// at `units` points to a game unit. Anything with a direction needs both
/// pins apart, and isn't drawn until it has them.
pub(super) fn draw(
    painter: &egui::Painter,
    at: (Option<Pos2>, Pos2),
    reach: (Reach, f32),
    ink: Color32,
) {
    painter.extend(outline(at, reach, ink));
}

/// The shapes `draw` paints.
fn outline(
    (stand, land): (Option<Pos2>, Pos2),
    (reach, units): (Reach, f32),
    ink: Color32,
) -> Vec<Shape> {
    let (fill, edge) = (
        ink.gamma_multiply(0.14),
        Stroke::new(1.5, ink.gamma_multiply(0.7)),
    );
    let Some(ahead) = stand.map(|s| land - s).filter(|d| d.length() > 1.0) else {
        return match reach {
            Reach::Round(..) => outline((Some(land - Vec2::X * 2.0), land), (reach, units), ink),
            _ => Vec::new(),
        };
    };
    let from = land - ahead;
    match reach {
        Reach::Round(outer, inner) => {
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
        Reach::Strip(start, length, wide) => {
            vec![band(
                from,
                ahead.normalized(),
                (start * units, (start + length) * units, wide * units),
                (fill, edge),
            )]
        }
        Reach::Fan(far, degrees) => {
            let (towards, half) = (ahead.y.atan2(ahead.x), degrees.to_radians() / 2.0);
            let mut points = vec![from];
            points.extend((0..=16_u8).map(|i| {
                let turn = (f32::from(i) / 16.0).mul_add(2.0 * half, towards - half);
                from + Vec2::angled(turn) * far * units
            }));
            vec![Shape::convex_polygon(points, fill, edge)]
        }
        Reach::Across(length) => {
            let half = length * units / 2.0;
            vec![band(
                land,
                ahead.normalized().rot90(),
                (-half, half, 150.0 * units),
                (fill, edge),
            )]
        }
        Reach::Wire(length) => {
            let reaches = (length * units).min(ahead.length());
            vec![band(
                land,
                -ahead.normalized(),
                (0.0, reaches, 0.0),
                (edge.color, edge),
            )]
        }
    }
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
        assert_eq!(reach("Updraft"), None);
    }

    /// The corners of every polygon drawn, rounded to a point.
    fn corners(shapes: &[Shape]) -> Vec<(i32, i32)> {
        shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Path(path) => Some(path.points.clone()),
                _ => None,
            })
            .flatten()
            .map(|p| (p.x.round() as i32, p.y.round() as i32))
            .collect()
    }

    /// A strip runs out from where it's thrown, a wire back from where it
    /// sticks, and a wall stands across, each the size the game says, here
    /// at a point to a metre.
    #[test]
    fn each_shape_points_the_way_it_is_thrown() {
        let at = (Some(pos2(0.0, 0.0)), pos2(100.0, 0.0));
        let units = 0.01;
        let ink = Color32::WHITE;
        let strip = outline(at, (Reach::Strip(800.0, 5600.0, 800.0), units), ink);
        assert_eq!(corners(&strip), [(8, -4), (64, -4), (64, 4), (8, 4)]);
        let wire = outline(at, (Reach::Wire(1500.0), units), ink);
        assert_eq!(corners(&wire), [(100, 2), (85, 2), (85, -2), (100, -2)]);
        let wall = outline(at, (Reach::Across(1040.0), units), ink);
        assert_eq!(corners(&wall), [(99, 5), (99, -5), (102, -5), (102, 5)]);
        assert!(outline((None, pos2(0.0, 0.0)), (Reach::Wire(1500.0), units), ink).is_empty());
    }
}
