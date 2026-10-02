//! The design system: one set of values for the window and the installer, so
//! two panels do not end up eight and eleven points apart.

use egui::{Color32, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};

pub mod art;
mod art_assets;
pub mod chrome;

/// Type sizes in points, whole because a 13.5 point body rasterises as a
/// blurrier 13.
pub mod size {
    /// The score, and nothing else is ever this big.
    pub const HERO: f32 = 30.0;
    /// A screen's own title, and the subject of the panel.
    pub const DISPLAY: f32 = 20.0;
    /// A section's subject: a team, a heading with weight behind it.
    pub const TITLE: f32 = 15.0;
    /// Everything that is read rather than scanned.
    pub const BODY: f32 = 13.0;
    /// Column headings and section labels. Caps, letterspaced.
    pub const LABEL: f32 = 11.0;
    /// Footnotes and counters that only matter when looked for.
    pub const MICRO: f32 = 10.0;
}

/// A four point grid, since values off it do not line up with each other.
pub mod space {
    /// Between two things that belong to each other.
    pub const SM: f32 = 4.0;
    /// The default gap, and the gutter between columns.
    pub const MD: f32 = 8.0;
    /// A panel's inner padding.
    pub const LG: f32 = 12.0;
    /// Between two blocks that are not related.
    pub const XL: f32 = 16.0;
    /// Around something that wants to be alone.
    pub const XXL: f32 = 24.0;
    /// One player: body type doubled and rounded onto the grid.
    pub const ROW: f32 = 28.0;
    /// One player, on a window too narrow to spend the extra four points.
    pub const ROW_TIGHT: f32 = 24.0;
}

/// How long things take, in seconds, after Riot's own interfaces: mostly 150
/// to 250 ms, 700 for a bar that measures something, and nothing bounces.
pub mod motion {
    /// A hover tint.
    pub const INSTANT: f32 = 0.15;
    /// A selection moving, a value changing, a colour swapping.
    pub const QUICK: f32 = 0.20;
    /// A bar filling, slow enough that its length reads as a number.
    pub const MEASURE: f32 = 0.70;

    /// Every duration in the efficient tier, where nothing animates.
    pub const EFFICIENT: f32 = 0.0;

    /// Something that has just loaded coming in.
    pub const ENTER: f32 = 0.32;
    /// How far below its place it starts, in points.
    pub const RISE: f32 = 10.0;
    /// The most a list waits before its last row comes in, however long the
    /// list, so a long one doesn't trickle.
    pub const STAGGER_MOST: f32 = 0.25;

    /// The one easing curve, decelerating, because easing in at both ends
    /// reads as a slideshow.
    #[must_use]
    pub fn eased(t: f32) -> f32 {
        egui::emath::easing::cubic_out(t.clamp(0.0, 1.0))
    }

    /// Where the flag [`set_still`] keeps is stored.
    fn still_key() -> egui::Id {
        egui::Id::new("motion-still")
    }

    /// Says whether this frame is in the efficient tier, where nothing comes
    /// in and everything is just there. Until something says otherwise it is,
    /// so a picture of a screen is never caught halfway.
    pub fn set_still(ctx: &egui::Context, still: bool) {
        ctx.data_mut(|store| store.insert_temp(still_key(), still));
    }

    /// How far `id` has come in, eased from 0 to 1, starting `delay` seconds
    /// after the first frame it was drawn. One that stops being drawn starts
    /// over when it comes back.
    #[must_use]
    pub fn enter(ctx: &egui::Context, id: egui::Id, delay: f32) -> f32 {
        if ctx.data(|store| store.get_temp::<bool>(still_key()).unwrap_or(true)) {
            return 1.0;
        }
        let now = ctx.input(|i| i.time);
        let pass = ctx.cumulative_pass_nr();
        let key = id.with("motion-enter");
        let since = ctx.data_mut(|store| {
            let seen = store.get_temp_mut_or_insert_with(key, || (now, pass));
            // Two passes of slack, since egui sometimes runs a frame twice.
            if seen.1.saturating_add(2) < pass {
                *seen = (now, pass);
            }
            seen.1 = pass;
            seen.0
        });
        let t = (((now - since) as f32 - delay) / ENTER).clamp(0.0, 1.0);
        if t < 1.0 {
            ctx.request_repaint();
        }
        eased(t)
    }

    /// The wait before the `index`th row of `count` comes in, spread so the
    /// last waits [`STAGGER_MOST`] at most.
    #[must_use]
    pub fn stagger(index: usize, count: usize) -> f32 {
        let step = (STAGGER_MOST / count.max(1) as f32).min(0.04);
        index as f32 * step
    }

    /// Draws `add` coming in: faded and a little below its place on the
    /// frames after `id` is first drawn, then just as it is.
    pub fn arrive<R>(
        ui: &mut egui::Ui,
        id: egui::Id,
        delay: f32,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let t = enter(ui.ctx(), id, delay);
        if t >= 1.0 {
            return add(ui);
        }
        ui.scope(|ui| {
            ui.multiply_opacity(t);
            ui.add_space(RISE * (1.0 - t));
            add(ui)
        })
        .inner
    }
}

/// Colour by the job it does, with the same values as `tui/src/theme.ts`.
pub mod colour {
    use egui::Color32;

    /// Below the ground: shadows, input wells, and dark ink on a pale fill.
    pub const VOID: Color32 = Color32::from_rgb(0x05, 0x06, 0x08);
    /// The ground, a neutral near black, because a broadcast is printed on
    /// black and not on the game's menu slate.
    pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0B, 0x0E);
    /// A slab sitting on the ground: a row, a card, a plate.
    pub const BG_RAISED: Color32 = Color32::from_rgb(0x15, 0x17, 0x1C);
    /// A slab on a slab: a chip, an input, the panel's inner cards.
    pub const BG_INSET: Color32 = Color32::from_rgb(0x1D, 0x20, 0x27);
    /// A slab under the pointer.
    pub const BG_HOVER: Color32 = Color32::from_rgb(0x1C, 0x1F, 0x26);
    /// The slab you chose.
    pub const BG_SELECTED: Color32 = Color32::from_rgb(0x25, 0x29, 0x33);
    /// A rule, where a rule is really needed.
    pub const LINE: Color32 = Color32::from_rgb(0x2A, 0x2E, 0x36);

    /// Names and numerals: the broadcast's cream. 15.6:1 on a slab.
    pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xF2, 0xEF, 0xE8);
    /// A value. 11.3:1 on a slab.
    pub const TEXT: Color32 = Color32::from_rgb(0xC9, 0xCE, 0xD6);
    /// Secondary text. 6.9:1 on a slab.
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9A, 0xA1, 0xAB);
    /// A label. 5.7:1 on a slab, past the 4.5:1 that small text needs.
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x8B, 0x92, 0x9C);
    /// The words in an empty text box, about 2:1 on a slab, so they're only
    /// there for anyone looking and never read as something typed.
    pub const HINT: Color32 = Color32::from_rgb(0x47, 0x4C, 0x54);
    /// Your team.
    pub const ALLY: Color32 = Color32::from_rgb(0x18, 0xE5, 0xA7);
    /// The other team. The one colour that owns a whole slab.
    pub const ENEMY: Color32 = Color32::from_rgb(0xFF, 0x46, 0x55);
    /// You, in cream rather than a team colour, because in game your own row
    /// is the light one.
    pub const YOU: Color32 = TEXT_STRONG;
    /// Good for you: your own rating going up, a match you won.
    pub const GOOD: Color32 = ALLY;
    /// A measurement rather than an outcome.
    pub const INFO: Color32 = Color32::from_rgb(0x9A, 0xDE, 0xFF);
    /// Worth a look. Amber, because red already means the enemy.
    pub const WARN: Color32 = Color32::from_rgb(0xFF, 0xC8, 0x45);
    /// Bad for you.
    pub const BAD: Color32 = Color32::from_rgb(0xFF, 0x80, 0x88);

    /// The other team's parties, cyan then blue. Each side has its own pair so
    /// a duo on each team never reads as the same duo.
    pub const PARTY_THEIRS: [Color32; 2] = [
        Color32::from_rgb(0x5C, 0xE1, 0xE6),
        Color32::from_rgb(0x7A, 0xA2, 0xFF),
    ];

    /// Your team's parties: violet, then pink.
    pub const PARTY_OURS: [Color32; 2] = [
        Color32::from_rgb(0xC5, 0x8B, 0xFF),
        Color32::from_rgb(0xF2, 0x8D, 0xD5),
    ];

    /// An enemy's number as a threat, from neutral at 0 through amber to red
    /// at 1, since a good number on their side is bad news on yours.
    #[must_use]
    pub fn threat(t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        if t < 0.5 {
            super::shape::blend(TEXT, WARN, t * 2.0)
        } else {
            super::shape::blend(WARN, ENEMY, (t - 0.5) * 2.0)
        }
    }

    /// The same idea for your own side: neutral up to your team's green.
    #[must_use]
    pub fn strength(t: f32) -> Color32 {
        super::shape::blend(TEXT, ALLY, t.clamp(0.0, 1.0))
    }
}

/// One colour per rank group, byte for byte the `color` of each tier in
/// `valorant-api.com/v1/competitivetiers`, so a badge here matches the game's.
const RANKS: [Color32; 10] = [
    Color32::from_rgb(0x4A, 0x4A, 0x4A),
    Color32::from_rgb(0x86, 0x89, 0x86),
    Color32::from_rgb(0xA5, 0x85, 0x5D),
    Color32::from_rgb(0xBB, 0xC2, 0xC2),
    Color32::from_rgb(0xEC, 0xCF, 0x56),
    Color32::from_rgb(0x59, 0xA9, 0xB6),
    Color32::from_rgb(0xB4, 0x89, 0xC4),
    Color32::from_rgb(0x6A, 0xE2, 0xAF),
    Color32::from_rgb(0xBB, 0x3D, 0x65),
    Color32::from_rgb(0xFF, 0xFF, 0xAA),
];

/// The colour for a rank tier. Tier 0 to 2 is unranked, then three per group.
#[must_use]
pub fn rank(tier: Option<u32>) -> Color32 {
    match tier {
        // 12, 13 and 14 are all Gold.
        Some(t) if t >= 3 => *RANKS
            .get(t.div_euclid(3) as usize)
            .or_else(|| RANKS.last())
            .unwrap_or(&colour::TEXT),
        _ => colour::TEXT_FAINT,
    }
}

/// What Riot calls the group a tier belongs to, in lowercase.
#[must_use]
pub fn rank_group(tier: u32) -> &'static str {
    const NAMES: [&str; 10] = [
        "unranked",
        "iron",
        "bronze",
        "silver",
        "gold",
        "platinum",
        "diamond",
        "ascendant",
        "immortal",
        "radiant",
    ];
    NAMES
        .get(tier.div_euclid(3) as usize)
        .or_else(|| NAMES.last())
        .unwrap_or(&"unranked")
}

/// The shapes the interface is allowed to make, most of them built on the
/// game's own 45 degree cut corner.
pub mod shape {
    use egui::{Color32, Pos2, Rect, Shape, Stroke, pos2};

    /// A rectangle with its top left and bottom right corners cut, since all
    /// four would read as an octagon.
    #[must_use]
    pub fn cut_corners(rect: Rect, cut: f32) -> Vec<Pos2> {
        let cut = cut.min(rect.width() * 0.5).min(rect.height() * 0.5);
        vec![
            pos2(rect.left() + cut, rect.top()),
            pos2(rect.right(), rect.top()),
            pos2(rect.right(), rect.bottom() - cut),
            pos2(rect.right() - cut, rect.bottom()),
            pos2(rect.left(), rect.bottom()),
            pos2(rect.left(), rect.top() + cut),
        ]
    }

    /// Fills a chamfered rectangle.
    pub fn cut_filled(rect: Rect, cut: f32, fill: Color32) -> Shape {
        Shape::convex_polygon(cut_corners(rect, cut), fill, Stroke::NONE)
    }

    /// Triangulates the first `corners` vertices of `mesh`. A fan from the
    /// first vertex is enough because the cut rectangle is convex.
    fn fan(mesh: &mut egui::Mesh, corners: usize) {
        for i in 1..corners.saturating_sub(1) {
            mesh.add_triangle(0, i as u32, i as u32 + 1);
        }
    }

    /// Mixes two premultiplied colours, so a fade out has to end at
    /// [`Color32::TRANSPARENT`] and not the same colour with no alpha.
    #[must_use]
    pub fn blend(from: Color32, to: Color32, t: f32) -> Color32 {
        let mix = |a: u8, b: u8| (f32::from(b) - f32::from(a)).mul_add(t, f32::from(a)) as u8;
        Color32::from_rgba_premultiplied(
            mix(from.r(), to.r()),
            mix(from.g(), to.g()),
            mix(from.b(), to.b()),
            mix(from.a(), to.a()),
        )
    }

    /// Dark or light ink for text on `fill`, by Rec. 601 luma, since agent
    /// colours run from near black to pale yellow.
    #[must_use]
    pub fn ink_on(fill: Color32) -> Color32 {
        let luma = 0.299f32.mul_add(
            f32::from(fill.r()),
            0.587f32.mul_add(f32::from(fill.g()), 0.114 * f32::from(fill.b())),
        );
        if luma > 140.0 {
            super::colour::VOID
        } else {
            super::colour::TEXT_STRONG
        }
    }

    /// One result, as a block of colour lit from the top. It is too small for
    /// a hairline, so the light is the gradient itself.
    pub fn pip(rect: Rect, tint: Color32) -> Shape {
        Shape::gradient_rect(
            rect,
            egui::Direction::TopDown,
            [
                blend(tint, super::colour::TEXT_STRONG, 0.28),
                tint.gamma_multiply(0.78),
            ],
        )
    }

    /// A chamfered surface with a wash down its face, a light hairline along
    /// the top and a dark one along the bottom, which is what makes it read as
    /// raised.
    pub fn lit(rect: Rect, cut: f32, top: Color32, bottom: Color32) -> Shape {
        let points = cut_corners(rect, cut);
        let mut mesh = egui::Mesh::default();
        for point in &points {
            let t = ((point.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
            mesh.colored_vertex(*point, blend(top, bottom, t));
        }
        fan(&mut mesh, points.len());
        let inset = cut.min(rect.width() * 0.5).min(rect.height() * 0.5);
        Shape::Vec(vec![
            Shape::mesh(mesh),
            Shape::line_segment(
                [
                    pos2(rect.left() + inset, rect.top() + 0.5),
                    pos2(rect.right() - 0.5, rect.top() + 0.5),
                ],
                Stroke::new(1.0, super::colour::TEXT_STRONG.gamma_multiply(0.11)),
            ),
            Shape::line_segment(
                [
                    pos2(rect.left() + 0.5, rect.bottom() - 0.5),
                    pos2(rect.right() - inset, rect.bottom() - 0.5),
                ],
                Stroke::new(1.0, super::colour::VOID.gamma_multiply(0.55)),
            ),
        ])
    }

    /// The upright bar before a section heading, two points wide.
    pub fn tick(at: Pos2, height: f32, tint: Color32) -> Shape {
        Shape::rect_filled(Rect::from_min_size(at, egui::vec2(2.0, height)), 0, tint)
    }
}

/// The app saying something, as a washed band in `tint` indented by the
/// screen's own `gutter`.
pub fn say(ui: &mut egui::Ui, gutter: f32, tint: Color32, message: &str) {
    let (rect, _response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), space::ROW),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().clone();
    let band = egui::Rect::from_min_max(
        egui::pos2(rect.left() + gutter, rect.top()),
        egui::pos2(rect.right() - gutter, rect.bottom() - space::SM),
    );
    painter.add(shape::cut_filled(band, 4.0, tint.gamma_multiply(0.10)));
    painter.rect_filled(
        egui::Rect::from_min_size(band.min, egui::vec2(2.0, band.height())),
        0,
        tint,
    );
    painter.text(
        egui::pos2(band.left() + space::LG, band.center().y),
        egui::Align2::LEFT_CENTER,
        message,
        Face::Body.at(size::MICRO),
        tint,
    );
}

/// A row that can be switched on, true when clicked.
///
/// `one_of` draws it as one choice among siblings rather than a switch that
/// stands alone. `still`, for the window's efficient tier, makes the hover land
/// at once instead of fading in.
pub fn choice(
    ui: &mut egui::Ui,
    name: &str,
    about: &str,
    on: bool,
    one_of: bool,
    still: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_CHOICE),
        egui::Sense::click(),
    );
    ui.add_space(space::SM);
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    let painter = ui.painter().clone();
    let lift = motion::eased(ui.ctx().animate_bool_with_time(
        response.id,
        response.hovered(),
        if still {
            motion::EFFICIENT
        } else {
            motion::INSTANT
        },
    ));
    let slab = egui::Rect::from_min_max(
        egui::pos2(rect.left() + space::LG, rect.top()),
        egui::pos2(rect.right() - space::LG, rect.bottom()),
    );
    // Only a chosen sibling lights its row. A dozen lit switches would say
    // nothing.
    let ground = if on && one_of {
        colour::BG_SELECTED
    } else {
        colour::BG_RAISED
    };
    painter.rect_filled(slab, 0, shape::blend(ground, colour::BG_HOVER, lift * 0.7));
    painter.hline(
        slab.x_range(),
        slab.top() + 0.5,
        (
            1.0,
            colour::TEXT_STRONG.gamma_multiply(0.05f32.mul_add(lift, 0.05)),
        ),
    );
    let pip = egui::Rect::from_center_size(
        egui::pos2(slab.left() + space::XL + 6.0, slab.center().y),
        egui::vec2(12.0, 16.0),
    );
    mark(&painter, pip, on, one_of);
    let drawn = caps_text(
        &painter,
        egui::pos2(pip.right() + space::LG, slab.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        Face::Display.at(size::BODY + 1.0),
        if on || lift > 0.0 {
            colour::TEXT_STRONG
        } else {
            colour::TEXT_DIM
        },
    );
    // Cut to what is left of the row rather than run off the end of it: two
    // of these side by side leave half the width each, and the longest
    // explanation is wider than half.
    let at = drawn.right().max(slab.left() + 190.0) + space::LG;
    let mut job = egui::text::LayoutJob::simple_singleline(
        about.to_owned(),
        Face::Body.at(size::LABEL + 1.0),
        colour::TEXT_FAINT,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: slab.right() - space::LG - at,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(at, slab.center().y - galley.size().y / 2.0),
        galley,
        colour::TEXT_FAINT,
    );
    response.clicked()
}

/// The slanted pip: solid for a switch, and an outline that fills for a
/// choice, so a set of choices does not look like switches you can pick two of.
fn mark(painter: &egui::Painter, pip: egui::Rect, on: bool, one_of: bool) {
    // The same lean at every size, so the fill sits parallel in its outline.
    let slanted = |r: egui::Rect| {
        let run = r.height() * 0.21;
        vec![
            egui::pos2(r.left() + run, r.top()),
            egui::pos2(r.right(), r.top()),
            egui::pos2(r.right() - run, r.bottom()),
            egui::pos2(r.left(), r.bottom()),
        ]
    };
    if one_of {
        painter.add(egui::Shape::closed_line(
            slanted(pip.shrink(0.75)),
            Stroke::new(
                1.5,
                if on {
                    colour::TEXT_STRONG
                } else {
                    colour::TEXT_FAINT
                },
            ),
        ));
        if on {
            painter.add(egui::Shape::convex_polygon(
                slanted(pip.shrink(3.0)),
                colour::TEXT_STRONG,
                Stroke::NONE,
            ));
        }
    } else {
        painter.add(egui::Shape::convex_polygon(
            slanted(pip),
            if on {
                colour::TEXT_STRONG
            } else {
                colour::LINE
            },
            Stroke::NONE,
        ));
    }
}

/// How tall a switch's slab is.
const ROW_CHOICE: f32 = 32.0;

/// One key, drawn as a plate with its letter on it.
#[must_use]
pub fn keycap(painter: &egui::Painter, at: egui::Pos2, key: &str) -> egui::Rect {
    let galley = painter.layout_no_wrap(key.to_uppercase(), keycap_face(key), colour::TEXT_DIM);
    let plate = egui::Rect::from_min_size(
        egui::pos2(at.x, at.y - 8.0),
        egui::vec2(keycap_width(painter, key), 16.0),
    );
    painter.add(egui::Shape::gradient_rect(
        plate,
        egui::Direction::TopDown,
        [colour::BG_HOVER, colour::BG_INSET],
    ));
    painter.rect_stroke(
        plate,
        0,
        Stroke::new(1.0, colour::LINE),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        egui::pos2(
            plate.center().x - galley.size().x / 2.0,
            plate.center().y - galley.size().y / 2.0,
        ),
        galley,
        colour::TEXT_DIM,
    );
    plate
}

/// How wide [`keycap`] will draw a key, so a row of hints can check the next
/// one fits before drawing it.
#[must_use]
pub fn keycap_width(painter: &egui::Painter, key: &str) -> f32 {
    let galley = painter.layout_no_wrap(key.to_uppercase(), keycap_face(key), colour::TEXT_DIM);
    (galley.size().x + space::MD).max(16.0)
}

/// Barlow has no arrows, so a key that is not ASCII is set in Inter rather
/// than drawn as a box.
fn keycap_face(key: &str) -> FontId {
    if key.is_ascii() {
        Face::Display.at(size::MICRO)
    } else {
        Face::Body.at(size::MICRO)
    }
}

/// A colour sent as `#RRGGBB`, the way Riot sends agent colours, if it parses.
#[must_use]
pub fn hex(text: Option<&str>) -> Option<Color32> {
    let raw = text?.trim().trim_start_matches('#');
    if raw.len() < 6 {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(raw.get(at..at + 2)?, 16).ok();
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// The four faces, by the job each one does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Barlow Condensed Bold, for names, labels and headings set in caps.
    Display,
    /// Barlow Condensed at extra bold italic, for the numerals, team titles,
    /// score and map.
    Heavy,
    /// Barlow Condensed at semibold. Secondary numbers and the quieter labels.
    Number,
    /// Inter, for sentences, the only text here read word by word.
    Body,
}

impl Face {
    /// The family name registered in [`install_fonts`].
    const fn key(self) -> &'static str {
        match self {
            Self::Display => "overseer-display",
            Self::Heavy => "overseer-heavy",
            Self::Number => "overseer-number",
            Self::Body => "overseer-body",
        }
    }

    /// This face at a size.
    #[must_use]
    pub fn at(self, points: f32) -> FontId {
        FontId::new(points, FontFamily::Name(self.key().into()))
    }

    /// Brings Barlow's caps down a touch to share a centre line with Inter,
    /// and pins Inter's weight and optical size.
    fn tweak(self) -> egui::FontTweak {
        match self {
            Self::Display | Self::Heavy | Self::Number => egui::FontTweak {
                y_offset_factor: 0.02,
                ..egui::FontTweak::default()
            },
            Self::Body => {
                let mut coords = egui::epaint::text::VariationCoords::default();
                coords.push(*b"wght", 440.0);
                coords.push(*b"opsz", 16.0);
                egui::FontTweak {
                    coords,
                    ..egui::FontTweak::default()
                }
            }
        }
    }
}

/// The font file behind each face, bound into the binary.
const FACES: [(Face, &[u8]); 4] = [
    (
        Face::Display,
        include_bytes!("../assets/BarlowCondensed-Bold.ttf"),
    ),
    (
        Face::Heavy,
        include_bytes!("../assets/BarlowCondensed-ExtraBoldItalic.ttf"),
    ),
    (
        Face::Number,
        include_bytes!("../assets/BarlowCondensed-SemiBold.ttf"),
    ),
    (Face::Body, include_bytes!("../assets/Inter[opsz,wght].ttf")),
];

/// Registers the four faces and their fallbacks.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Barlow has no Cyrillic, and egui's own face looks like a caption beside
    // Barlow Bold, so Inter at 700 goes behind it first.
    let mut bold = egui::epaint::text::VariationCoords::default();
    bold.push(*b"wght", 700.0);
    bold.push(*b"opsz", 16.0);
    fonts.font_data.insert(
        BOLD_FALLBACK.to_owned(),
        std::sync::Arc::new(
            egui::FontData::from_static(include_bytes!("../assets/Inter[opsz,wght].ttf")).tweak(
                egui::FontTweak {
                    coords: bold,
                    ..egui::FontTweak::default()
                },
            ),
        ),
    );
    for (face, bytes) in FACES {
        let key = face.key().to_owned();
        fonts.font_data.insert(
            key.clone(),
            std::sync::Arc::new(egui::FontData::from_static(bytes).tweak(face.tweak())),
        );
        // Inter behind the caps faces, then egui's bundled font, so a glyph
        // the face lacks still draws instead of becoming a box.
        let mut chain = vec![key.clone()];
        if face != Face::Body {
            chain.push(BOLD_FALLBACK.to_owned());
        }
        chain.extend(
            fonts
                .families
                .get(&FontFamily::Proportional)
                .cloned()
                .unwrap_or_default(),
        );
        fonts.families.insert(FontFamily::Name(key.into()), chain);
    }
    ctx.set_fonts(fonts);
}

/// Inter at 700, registered behind the Barlow faces.
const BOLD_FALLBACK: &str = "overseer-bold-fallback";

/// Windows' own bold faces for Hangul, kana, Han and Thai names, read from its
/// font folder because bundling them would add fifty megabytes to the binary.
const SCRIPTS: [Script; 4] = [
    (
        "malgunbd.ttf",
        0,
        |c| matches!(c, '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{AC00}'..='\u{D7AF}'),
    ),
    (
        "YuGothB.ttc",
        0,
        |c| matches!(c, '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}'),
    ),
    (
        "msyhbd.ttc",
        0,
        |c| matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'),
    ),
    ("LeelawUI.ttf", 0, |c| matches!(c, '\u{0E00}'..='\u{0E7F}')),
];

/// A system face: its file, its index in the file, and the characters it is
/// loaded for.
type Script = (&'static str, u32, fn(char) -> bool);

/// Loads the system face for any script in `text` that has none yet, on its
/// own thread because the files are tens of megabytes.
pub fn cover(ctx: &egui::Context, text: &str) {
    for (file, index) in wanted(ctx, text) {
        let ctx = ctx.clone();
        let _loading = std::thread::Builder::new()
            .name("overseer-font".to_owned())
            .spawn(move || {
                if add_system_font(&ctx, file, index) {
                    ctx.request_repaint();
                }
            });
    }
}

/// [`cover`], on this thread, for a test that has to see the result.
pub fn cover_now(ctx: &egui::Context, text: &str) {
    for (file, index) in wanted(ctx, text) {
        let _added = add_system_font(ctx, file, index);
    }
}

/// The faces `text` needs that nobody has asked for yet, marked as asked for
/// so each one loads once.
fn wanted(ctx: &egui::Context, text: &str) -> Vec<(&'static str, u32)> {
    let asked = egui::Id::new("overseer-scripts");
    let mut out = Vec::new();
    for (file, index, needs) in SCRIPTS {
        if !text.chars().any(needs) {
            continue;
        }
        let key = asked.with(file);
        if ctx.data(|d| d.get_temp::<bool>(key)).unwrap_or(false) {
            continue;
        }
        ctx.data_mut(|d| d.insert_temp(key, true));
        out.push((file, index));
    }
    out
}

/// Reads a face from Windows' font folder and puts it at the back of every
/// family. False when the file is not there, which leaves the boxes.
fn add_system_font(ctx: &egui::Context, file: &str, index: u32) -> bool {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let Ok(bytes) = std::fs::read(std::path::Path::new(&root).join("Fonts").join(file)) else {
        return false;
    };
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    let families = [Face::Display, Face::Heavy, Face::Number, Face::Body]
        .iter()
        .map(|face| FontFamily::Name(face.key().into()))
        .chain([FontFamily::Proportional, FontFamily::Monospace])
        .map(|family| egui::epaint::text::InsertFontFamily {
            family,
            priority: egui::epaint::text::FontPriority::Lowest,
        })
        .collect();
    ctx.add_font(egui::epaint::text::FontInsert::new(file, data, families));
    true
}

/// The app's style: flat, dark, one accent and square corners, like the game.
#[must_use]
pub fn style() -> Style {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = colour::BG;
    visuals.window_fill = colour::BG;
    // Text fields sit a step darker than the surface under them, so they look
    // like somewhere to type.
    visuals.extreme_bg_color = colour::VOID;
    visuals.faint_bg_color = colour::BG_RAISED;
    visuals.override_text_color = Some(colour::TEXT);
    visuals.window_stroke = Stroke::new(1.0, colour::LINE);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colour::LINE);
    visuals.widgets.inactive.bg_fill = colour::BG_INSET;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, colour::LINE);
    visuals.widgets.inactive.weak_bg_fill = colour::BG_INSET;
    visuals.widgets.hovered.bg_fill = colour::BG_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, colour::TEXT_FAINT);
    visuals.widgets.hovered.weak_bg_fill = colour::BG_HOVER;
    visuals.widgets.active.bg_fill = colour::BG_SELECTED;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, colour::ENEMY);
    visuals.widgets.active.weak_bg_fill = colour::BG_SELECTED;
    visuals.selection.bg_fill = colour::ENEMY.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, colour::TEXT_STRONG);
    visuals.text_cursor.stroke = Stroke::new(2.0, colour::ENEMY);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::ZERO;
    }
    visuals.window_corner_radius = egui::CornerRadius::ZERO;
    visuals.menu_corner_radius = egui::CornerRadius::ZERO;

    let mut style = Style {
        visuals,
        ..Style::default()
    };
    style.spacing.item_spacing = egui::vec2(space::MD, space::SM);
    style.spacing.window_margin = egui::Margin::ZERO;
    style.interaction.selectable_labels = false;
    // For any widget that does not name its own font.
    style.text_styles = [
        (TextStyle::Heading, Face::Display.at(size::TITLE)),
        (TextStyle::Body, Face::Body.at(size::BODY)),
        (TextStyle::Button, Face::Display.at(size::LABEL)),
        (TextStyle::Small, Face::Body.at(size::MICRO)),
        (TextStyle::Monospace, Face::Number.at(size::BODY)),
    ]
    .into();
    style
}

/// A label in caps, tracked with `extra_letter_spacing` so widths, wrapping
/// and hit testing agree with what is drawn.
#[must_use]
pub fn caps(text: &str, font: FontId, tint: Color32) -> egui::text::LayoutJob {
    let tracking = tracking_for(font.size);
    let base = egui::TextFormat {
        font_id: font,
        extra_letter_spacing: tracking,
        color: tint,
        ..egui::TextFormat::default()
    };
    let upper = text.to_uppercase();
    let mut job = egui::text::LayoutJob::default();
    // Runs of Hangul, kana, Han and Thai are set solid, because tracking
    // between syllable blocks pulls a name apart into letters.
    let mut start = 0;
    let mut chars = upper.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        let solid = unspaced(c);
        if chars
            .peek()
            .is_none_or(|(_, next)| unspaced(*next) != solid)
        {
            let end = chars.peek().map_or(upper.len(), |(next, _)| *next);
            let format = if solid {
                egui::TextFormat {
                    extra_letter_spacing: 0.0,
                    ..base.clone()
                }
            } else {
                base.clone()
            };
            job.append(upper.get(start..end).unwrap_or_default(), 0.0, format);
            start = end;
        }
    }
    job
}

/// Whether a character belongs to a script that is set without tracking.
fn unspaced(c: char) -> bool {
    SCRIPTS.iter().any(|(_, _, needs)| needs(c))
}

/// The tracking left after the last character of a caps label, which is
/// not part of its width.
fn trailing(text: &str, points: f32) -> f32 {
    if text.chars().last().is_some_and(unspaced) {
        0.0
    } else {
        tracking_for(points)
    }
}

/// The air between capitals at a size, from 6.5% of it at [`size::MICRO`] to
/// 2% at [`size::HERO`], because small caps need the help and large ones do not.
#[must_use]
pub fn tracking_for(points: f32) -> f32 {
    let t = ((points - size::MICRO) / (size::HERO - size::MICRO)).clamp(0.0, 1.0);
    points * 0.045f32.mul_add(-t, 0.065)
}

/// Paints a caps label, for call sites that do not need where it landed.
pub fn caps_at(
    painter: &egui::Painter,
    pos: egui::Pos2,
    anchor: egui::Align2,
    text: &str,
    font: FontId,
    tint: Color32,
) {
    let _drawn = caps_text(painter, pos, anchor, text, font, tint);
}

/// How wide [`caps_text`] will set a label, before it is drawn.
#[must_use]
pub fn caps_width(painter: &egui::Painter, text: &str, font: FontId) -> f32 {
    let trailing = trailing(text, font.size);
    let galley = painter.layout_job(caps(text, font, colour::TEXT_DIM));
    (galley.size().x - trailing).max(0.0)
}

/// Paints a caps label like [`egui::Painter::text`] and returns where it landed,
/// less the tracking after the last letter so edges and centres line up.
#[must_use]
pub fn caps_text(
    painter: &egui::Painter,
    pos: egui::Pos2,
    anchor: egui::Align2,
    text: &str,
    font: FontId,
    tint: Color32,
) -> egui::Rect {
    let trailing = trailing(text, font.size);
    let galley = painter.layout_job(caps(text, font, tint));
    let size = egui::vec2((galley.size().x - trailing).max(0.0), galley.size().y);
    let rect = anchor.anchor_size(pos, size);
    painter.galley(rect.min, galley, tint);
    rect
}

#[cfg(test)]
mod tests {
    use super::{Face, caps, choice, colour, install_fonts, rank, size, space, tracking_for};

    /// A switch in the efficient tier lights at once under the pointer, so
    /// once the pointer is there nothing asks for frames. The rich tier is
    /// the control: it fades in, and a fade asks for frames.
    #[test]
    fn a_still_switch_does_not_fade_in() {
        for still in [true, false] {
            let ctx = egui::Context::default();
            install_fonts(&ctx);
            // Draws a frame with the pointer wherever it is sent, and says
            // where the middle of the switch was.
            let frame = |pointer: Option<egui::Pos2>| {
                let input = egui::RawInput {
                    events: pointer.map(egui::Event::PointerMoved).into_iter().collect(),
                    ..Default::default()
                };
                let mut row = egui::Pos2::ZERO;
                let mut output = ctx.run_ui(input, |ui| {
                    row = ui.cursor().min + egui::vec2(space::XXL * 4.0, space::XL);
                    let _clicked = choice(ui, "Overlay", "In a corner", false, false, still);
                });
                output.textures_delta.clear();
                row
            };
            frame(None);
            let row = frame(None);
            frame(Some(row));
            // egui itself asks for two more frames after the pointer moves,
            // so the check looks past them.
            frame(None);
            frame(None);
            assert_eq!(
                ctx.has_requested_repaint(),
                !still,
                "still: {still}, and the hover asked for frames it should not have, or none it should"
            );
        }
    }

    /// A family registered nowhere panics inside epaint the first time a glyph
    /// in it is laid out.
    #[test]
    fn every_face_is_bound() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        // Fonts only exist once a frame has run, so the check runs inside one.
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx();
            for face in [Face::Display, Face::Heavy, Face::Number, Face::Body] {
                let galley = ctx.fonts_mut(|fonts| {
                    fonts.layout_no_wrap("W".to_owned(), face.at(size::BODY), colour::TEXT)
                });
                assert!(galley.size().x > 0.0, "{face:?} laid out nothing");
            }
        });
        // Nothing uploads the font atlas this frame made, and egui panics if
        // it is dropped without being cleared.
        output.textures_delta.clear();
    }

    #[test]
    fn unranked_tiers_are_not_in_the_palette() {
        for tier in 0..3 {
            assert_eq!(rank(Some(tier)), colour::TEXT_FAINT);
        }
        assert_eq!(rank(None), colour::TEXT_FAINT);
    }

    /// Radiant is tier 27, and anything past it gets Radiant's colour rather
    /// than something unrelated.
    #[test]
    fn every_group_has_its_own_colour() {
        assert_ne!(rank(Some(12)), rank(Some(15)));
        assert_eq!(rank(Some(12)), rank(Some(14)), "a group shares one colour");
        assert_eq!(rank(Some(27)), rank(Some(999)));
    }

    /// The only place that catches a value off the grid being added.
    #[test]
    fn every_space_is_on_the_four_point_grid() {
        for value in [
            space::SM,
            space::MD,
            space::LG,
            space::XL,
            space::XXL,
            space::ROW,
            space::ROW_TIGHT,
        ] {
            let steps = value / 4.0;
            assert!(steps.fract() == 0.0, "{value} is not on the grid");
        }
    }

    #[test]
    fn the_type_scale_only_grows() {
        let scale = [
            size::MICRO,
            size::LABEL,
            size::BODY,
            size::TITLE,
            size::DISPLAY,
        ];
        for pair in scale.windows(2) {
            let (small, large) = (pair.first().copied(), pair.get(1).copied());
            let (Some(small), Some(large)) = (small, large) else {
                continue;
            };
            assert!(large > small, "{large} does not follow {small}");
            assert!(
                large.fract() == 0.0,
                "{large} is not a whole number of points"
            );
        }
    }

    /// Tracking between Hangul syllable blocks reads as separate letters.
    #[test]
    fn a_hangul_name_is_not_spaced_out() {
        let job = caps(
            "gg \u{BE5B}\u{B098}",
            Face::Display.at(size::LABEL),
            colour::TEXT,
        );
        let spacing: Vec<f32> = job
            .sections
            .iter()
            .map(|s| s.format.extra_letter_spacing)
            .collect();
        assert_eq!(
            spacing,
            [tracking_for(size::LABEL), 0.0],
            "one tracked run and one solid one"
        );
    }

    /// Your parties and theirs never share a colour, and none of them is a
    /// colour that already means something.
    #[test]
    fn each_side_has_its_own_party_colours() {
        for ours in colour::PARTY_OURS {
            assert!(
                !colour::PARTY_THEIRS.contains(&ours),
                "{ours:?} is on both sides"
            );
        }
        for tint in colour::PARTY_OURS.iter().chain(colour::PARTY_THEIRS.iter()) {
            for taken in [colour::ENEMY, colour::ALLY, colour::WARN] {
                assert_ne!(
                    *tint, taken,
                    "a party wears a colour that means something else"
                );
            }
        }
    }

    #[test]
    fn a_label_is_caps_and_tracked() {
        let job = caps("rank", Face::Display.at(size::MICRO), colour::TEXT);
        assert_eq!(job.text, "RANK", "a label has to be in caps");
        let tracking = job
            .sections
            .first()
            .map_or(0.0, |s| s.format.extra_letter_spacing);
        assert!(tracking > 0.0, "a label has to be tracked");
        // Small caps need the help and large caps do not, so one constant
        // cannot be both.
        assert!(
            tracking_for(size::MICRO) / size::MICRO > tracking_for(size::HERO) / size::HERO,
            "tracking has to loosen as the type gets smaller"
        );
        assert!(
            caps("", Face::Display.at(size::BODY), colour::TEXT)
                .text
                .is_empty(),
            "nothing should lay out as nothing"
        );
    }
}
