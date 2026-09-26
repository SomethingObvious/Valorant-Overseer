//! The Valorant Overseer design system: every value the interface is allowed
//! to use, shared by the window and the installer so the two are one product.
//!
//! An app looks generated when every screen was decided separately. The cure
//! is not more decoration, it is fewer decisions, so this module holds all of
//! them and the rest of the crate holds none. A literal in a widget is a
//! decision somebody made alone, and it is how two panels end up eight and
//! eleven points apart for no reason anybody can name.
//!
//! The reasoning, and what to do when none of these fit, is in
//! `crates/DESIGN.md`.

use egui::{Color32, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};

/// Type sizes, in points. Whole numbers because glyphs are rasterised, and a
/// 13.5 point body is a blurrier 13 point body.
///
/// The scale has five steps and the motion scale has four durations; both are
/// written out in `crates/DESIGN.md`. Only the steps with
/// a user live here, because a constant nothing calls is a decision nobody
/// needed, and the lint that says so is right.
pub mod size {
    /// The one number on screen that changes while you watch: the score.
    /// Nothing else is allowed to be this big, which is the whole point of
    /// having a step nothing else uses.
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

/// A four point grid. Anything not on it is a guess, and guesses do not line
/// up with each other.
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
    /// One player. Body type doubled and rounded onto the grid, which leaves
    /// five and a half points of air above and below a thirteen point glyph.
    pub const ROW: f32 = 24.0;
    /// One player, on a window too narrow to spend the extra four points.
    pub const ROW_TIGHT: f32 = 20.0;
    /// One result in a run of them: a win, a loss, a match in a session.
    ///
    /// Counted rather than read, so it has to be big enough to count at a
    /// glance and small enough that five of them are one shape. Defined
    /// here because the board and the panel both draw them, and two sizes
    /// of the same mark is the sort of thing nobody sees and everybody
    /// feels.
    pub const PIP: f32 = 9.0;
}

/// How long things take. Four durations, because a fifth would be a fifth
/// opinion about the same question, and the whole point of a system is that
/// the same question has one answer.
pub mod motion {
    /// A hover tint.
    ///
    /// The four durations below are the ones Riot's own interfaces ship:
    /// their working range is 150 to 250 milliseconds, two hundred
    /// decelerating is the default, and a bar that represents a measurement
    /// takes seven hundred so that it reads as being measured rather than
    /// set. Nothing in their bundles bounces, overshoots or springs, and
    /// nothing here does either.
    pub const INSTANT: f32 = 0.15;
    /// A selection moving, a value changing, a colour swapping.
    pub const QUICK: f32 = 0.20;
    /// Something arriving that was not there: a roster landing, a panel
    /// changing subject.
    pub const ARRIVE: f32 = 0.30;
    /// A bar filling. Long and decelerating, because the length of it is a
    /// number and the eye should have time to read it as one.
    pub const MEASURE: f32 = 0.70;
    /// How long after a row before the one under it moves, when ten of them
    /// arrive together.
    pub const STAGGER: f32 = 0.028;

    /// The same durations when the window is being careful: fast enough to
    /// still say what changed, short enough to cost nothing.
    pub const EFFICIENT: f32 = 0.0;

    /// How movement is shaped.
    ///
    /// Everything decelerates. A thing that starts fast and settles reads as
    /// physical; a thing that eases in at both ends reads as a slideshow.
    /// One curve everywhere, because two would be two opinions about the
    /// same question.
    #[must_use]
    pub fn eased(t: f32) -> f32 {
        egui::emath::easing::cubic_out(t.clamp(0.0, 1.0))
    }
}

/// Colour by the job it does, never by what it looks like.
///
/// The same values as `tui/src/theme.ts`, so the window and the terminal are
/// one product. `you` is bone rather than red on purpose: in game your own row
/// is the light one and the enemy is the red one, and a companion app that
/// swaps those is worse than one with no colour at all.
pub mod colour {
    use egui::Color32;

    /// Behind everything, and darker than anything drawn on it.
    ///
    /// Four surfaces rather than two. A flat interface is not calm, it is
    /// undesigned: without a tone difference nothing can sit on anything,
    /// every edge has to be a line, and a screen of lines reads as a
    /// spreadsheet. These are the four steps, and nothing is allowed a fifth.
    pub const VOID: Color32 = Color32::from_rgb(0x08, 0x0C, 0x12);
    /// The board's own surface, and the game's own menu tone.
    pub const BG: Color32 = Color32::from_rgb(0x0F, 0x19, 0x23);
    /// The window's background in overlay mode.
    ///
    /// The same colour as [`BG`], letting enough of the game through that you
    /// can tell it is an overlay and little enough that the text still reads
    /// at a glance. Only ever used as a clear colour, where the window itself
    /// is transparent. Premultiplied, because that is the only constructor
    /// that is const: each channel scaled by the 0xD8 alpha beside it.
    pub const BG_OVERLAY: Color32 = Color32::from_rgba_premultiplied(0x09, 0x0E, 0x15, 0xD8);
    /// A surface that sits above the board: a header, the detail panel.
    pub const BG_RAISED: Color32 = Color32::from_rgb(0x1A, 0x24, 0x2E);
    /// A surface above that: a chip, an input, a card inside the panel.
    pub const BG_INSET: Color32 = Color32::from_rgb(0x1C, 0x29, 0x37);
    /// The tint under the cursor.
    pub const BG_HOVER: Color32 = Color32::from_rgb(0x1B, 0x28, 0x36);
    /// The tint on the row you have chosen.
    pub const BG_SELECTED: Color32 = Color32::from_rgb(0x23, 0x35, 0x47);
    /// A rule that separates two things.
    pub const LINE: Color32 = Color32::from_rgb(0x2B, 0x3A, 0x47);
    /// A rule between rows, which should be felt rather than seen.
    pub const LINE_SOFT: Color32 = Color32::from_rgb(0x19, 0x24, 0x30);

    /// A name, a heading: the brightest text there is.
    pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xEC, 0xE8, 0xE1);
    /// Ordinary text.
    pub const TEXT: Color32 = Color32::from_rgb(0xCE, 0xD8, 0xE0);
    /// A label beside a value.
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8A, 0x99, 0xA5);
    /// Present, but not the point. Bright enough to read on [`BG`], which
    /// the old value was not: a column heading nobody can read is a column
    /// heading that is not there.
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x64, 0x74, 0x82);
    /// Your team.
    pub const ALLY: Color32 = Color32::from_rgb(0x18, 0xE5, 0xA7);
    /// The other team, and anything dangerous.
    pub const ENEMY: Color32 = Color32::from_rgb(0xFF, 0x46, 0x55);
    /// You.
    pub const YOU: Color32 = TEXT_STRONG;
    /// A good number.
    pub const GOOD: Color32 = ALLY;
    /// A measurement rather than an outcome.
    pub const INFO: Color32 = Color32::from_rgb(0x9A, 0xDE, 0xFF);
    /// Something worth a second look.
    pub const WARN: Color32 = Color32::from_rgb(0xFF, 0xB4, 0x54);
    /// A number that is neither good nor bad.
    ///
    /// Most of them. Colour spent on an ordinary value is colour taken from
    /// the one that matters, and a board where every figure is tinted is a
    /// board with no figure on it.
    pub const NEUTRAL: Color32 = Color32::from_rgb(0xCE, 0xD8, 0xE0);
    /// A bad number.
    pub const BAD: Color32 = Color32::from_rgb(0xFF, 0x80, 0x88);
}

/// One colour per rank group, in tier order.
///
/// Riot's own, byte for byte: the `color` field of every tier in
/// `valorant-api.com/v1/competitivetiers`, which is the file the game reads.
/// These were guessed from screenshots before, and the guesses were close
/// enough to look right and wrong enough that a Platinum badge in the app and
/// a Platinum badge in the game were not the same colour. The one encoding a
/// player already knows by heart is the one worth getting exactly right.
///
/// All three divisions of a tier share a colour, which is why there are ten
/// entries and not thirty: colour says the tier, the numeral says the
/// division.
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
        // Floor division on purpose: 12, 13 and 14 are all Gold. Written as
        // div_euclid because the intent is the flooring, not a quotient that
        // happens to be truncated.
        Some(t) if t >= 3 => *RANKS
            .get(t.div_euclid(3) as usize)
            .or_else(|| RANKS.last())
            .unwrap_or(&colour::TEXT),
        _ => colour::TEXT_FAINT,
    }
}

/// The colour a K/D is worth saying out loud in.
#[must_use]
pub fn kd(value: Option<f64>) -> Color32 {
    match value {
        None => colour::TEXT_FAINT,
        Some(v) if v >= 1.3 => colour::GOOD,
        Some(v) if v >= 1.0 => colour::INFO,
        Some(v) if v >= 0.8 => colour::TEXT_STRONG,
        Some(_) => colour::BAD,
    }
}

/// The shapes the interface is allowed to make.
///
/// Three, and all three are the same idea: a corner cut off at forty five
/// degrees. It is the game's own motif, it costs four points of a rectangle,
/// and it is the difference between a card and a div. A fourth ornament
/// would be decoration; these carry meaning, because only things that can be
/// acted on or that group other things get one.
pub mod shape {
    use egui::{Color32, Pos2, Rect, Shape, Stroke, pos2};

    /// How far the cut comes in from the corner.
    pub const CHAMFER: f32 = 7.0;

    /// A rectangle with its top left and bottom right corners cut away.
    ///
    /// Diagonally opposite rather than all four: two cuts read as a
    /// direction, four read as an octagon.
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

    /// A chamfered rectangle filled with a gradient across it.
    ///
    /// Flat fills are what make an interface look printed rather than lit.
    /// A wash costs the same two triangles the flat version costs, because
    /// the colour is per vertex and the tessellator interpolates it for
    /// free, and it is the difference between a coloured rectangle and a
    /// surface with a light source somewhere.
    ///
    /// Convex only, which the two-corner cut is, so a fan from the first
    /// vertex covers it without a triangulator.
    pub fn cut_wash(rect: Rect, cut: f32, from: Color32, to: Color32) -> Shape {
        let points = cut_corners(rect, cut);
        let mut mesh = egui::Mesh::default();
        for point in &points {
            let t = ((point.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            mesh.colored_vertex(*point, blend(from, to, t));
        }
        for i in 1..points.len().saturating_sub(1) {
            mesh.add_triangle(0, i as u32, i as u32 + 1);
        }
        Shape::mesh(mesh)
    }

    /// Two colours, mixed. Premultiplied, which is how epaint stores them,
    /// so a fade to nothing has to go to [`Color32::TRANSPARENT`] rather
    /// than to the same colour with the alpha taken off.
    #[must_use]
    pub fn blend(from: Color32, to: Color32, t: f32) -> Color32 {
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t) as u8;
        Color32::from_rgba_premultiplied(
            mix(from.r(), to.r()),
            mix(from.g(), to.g()),
            mix(from.b(), to.b()),
            mix(from.a(), to.a()),
        )
    }

    /// Which of the two text colours reads on a given fill.
    ///
    /// Agent colours run from near black to pale yellow, so a tile drawn in
    /// one of them cannot have its letter in a fixed colour: half the roster
    /// would be unreadable. Rec. 601 luma, because it is the one a person
    /// would recognise and the difference from a perceptual model at this
    /// size is nothing.
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

    /// A soft edge under or beside a surface, so it reads as being above
    /// what it covers.
    ///
    /// Three stacked rects rather than one, because the only blur here is
    /// an oversized anti-aliasing feather and it falls off linearly: one of
    /// them alone has a hard edge, three of them look like light.
    #[must_use]
    pub fn drop_shadow(rect: Rect, down: f32) -> Vec<Shape> {
        [(16.0_f32, 26_u8), (8.0, 34), (3.0, 40)]
            .into_iter()
            .map(|(blur, alpha)| {
                Shape::from(
                    egui::epaint::RectShape::filled(
                        rect.translate(egui::vec2(0.0, down)),
                        0,
                        Color32::from_black_alpha(alpha),
                    )
                    .with_blur_width(blur),
                )
            })
            .collect()
    }

    /// The mark before a section heading: a short upright bar.
    ///
    /// Two points wide and the height of a cap. It is the only thing in the
    /// interface allowed to use the accent colour without meaning danger,
    /// because it means "a section starts here" and nothing else does.
    pub fn tick(at: Pos2, height: f32, tint: Color32) -> Shape {
        Shape::rect_filled(Rect::from_min_size(at, egui::vec2(2.0, height)), 0, tint)
    }
}

/// One key, drawn as a key: a plate with a letter on it.
///
/// The display face is caps only and has no arrows, so anything that is not
/// a letter is set in the reading face. A keycap with a box on it is worse
/// than no keycap.
///
/// Shared, because the footer and the settings screen both list keys and
/// two drawings of the same keyboard is two drawings to keep in step.
#[must_use]
pub fn keycap(painter: &egui::Painter, at: egui::Pos2, key: &str) -> egui::Rect {
    let face = if key.is_ascii() {
        Face::Display.at(size::MICRO)
    } else {
        Face::Body.at(size::MICRO)
    };
    let galley = painter.layout_no_wrap(key.to_uppercase(), face, colour::TEXT_DIM);
    let plate = egui::Rect::from_min_size(
        egui::pos2(at.x, at.y - 8.0),
        egui::vec2((galley.size().x + space::MD).max(16.0), 16.0),
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

/// A colour the backend sent as a hex string, if it sent one that parses.
///
/// Agent colours arrive from Riot as `#RRGGBB`. A row tinted with the agent's
/// own colour is the cheapest texture this board can have, and it is real
/// information rather than decoration: you learn the lobby's composition
/// before you have read a word of it.
#[must_use]
pub fn hex(text: Option<&str>) -> Option<Color32> {
    let raw = text?.trim().trim_start_matches('#');
    if raw.len() < 6 {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(raw.get(at..at + 2)?, 16).ok();
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// The three faces, by the job each one does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Oswald at semibold: a tall condensed grotesque, always set in caps
    /// here. Every heading, every column label, the wordmark. This is the
    /// face that carries the character.
    Display,
    /// Inter at regular. Names, and anything read rather than scanned. It
    /// was drawn for interfaces at small sizes and it disappears, which is
    /// what a name wants.
    Body,
    /// `JetBrains Mono` at medium. Tabular by construction, so a column of
    /// K/D cannot drift, and drawn this decade, which Consolas was not.
    Number,
}

impl Face {
    /// The family name registered in [`install_fonts`].
    const fn key(self) -> &'static str {
        match self {
            Self::Display => "overseer-display",
            Self::Body => "overseer-body",
            Self::Number => "overseer-number",
        }
    }

    /// This face at a size.
    #[must_use]
    pub fn at(self, points: f32) -> FontId {
        FontId::new(points, FontFamily::Name(self.key().into()))
    }

    /// How to sit this face on the line, and which cut of it to use.
    ///
    /// All three files are variable, and the rasteriser takes the default
    /// instance unless it is told otherwise. Oswald's default is Regular,
    /// which is far too light to be a heading, so the weight axis is
    /// pinned; Inter has an optical size axis, which is what lets one file
    /// hold together at ten points and at thirty.
    ///
    /// A point is not a height. Three faces at one point size are three
    /// different heights on screen, and the display face is set in caps, so
    /// it needs a nudge to share a baseline with the value beside it.
    fn tweak(self) -> egui::FontTweak {
        let pinned = |axes: &[(&[u8; 4], f32)]| {
            let mut coords = egui::epaint::text::VariationCoords::default();
            for (tag, value) in axes {
                coords.push(*tag, *value);
            }
            coords
        };
        match self {
            Self::Display => egui::FontTweak {
                coords: pinned(&[(b"wght", 600.0)]),
                y_offset_factor: 0.03,
                ..egui::FontTweak::default()
            },
            Self::Body => egui::FontTweak {
                coords: pinned(&[(b"wght", 420.0), (b"opsz", 16.0)]),
                ..egui::FontTweak::default()
            },
            Self::Number => egui::FontTweak {
                coords: pinned(&[(b"wght", 500.0)]),
                scale: 0.94,
                ..egui::FontTweak::default()
            },
        }
    }
}

/// What each face is, shipped inside the binary.
///
/// Riot's own are Tungsten Bold for display and DIN Next W1G for the
/// interface, and both are commercial. These are the substitutes Riot
/// themselves fall back to: their site sets Oswald wherever Tungsten cannot
/// be used, and its DIN stack ends in Inter. So this is not an
/// approximation somebody picked off a list, it is the one the people who
/// designed the thing picked.
///
/// Shipped rather than loaded from Windows, which is what this used to do
/// and which was wrong twice over: Windows has no condensed display
/// grotesque worth using, and a design system whose type depends on which
/// machine it is running on is not a design system. About one and a
/// quarter megabytes, and it buys the app a voice.
const FACES: [(Face, &[u8]); 3] = [
    (Face::Display, include_bytes!("../assets/Oswald[wght].ttf")),
    (Face::Body, include_bytes!("../assets/Inter[opsz,wght].ttf")),
    (
        Face::Number,
        include_bytes!("../assets/JetBrainsMono[wght].ttf"),
    ),
];

/// Registers the three faces, each pinned to its own cut.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (face, bytes) in FACES {
        let key = face.key().to_owned();
        fonts.font_data.insert(
            key.clone(),
            std::sync::Arc::new(egui::FontData::from_static(bytes).tweak(face.tweak())),
        );
        // Behind the face itself: egui's bundled font, so a glyph the face
        // does not have still draws instead of becoming a box.
        let mut chain = vec![key.clone()];
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

/// The app's style: flat, sharp, dark, one accent.
///
/// Sharp corners are the game's language and they are also cheaper: a rounded
/// rectangle is a different mesh per radius, and this app draws a great many
/// rectangles.
#[must_use]
pub fn style() -> Style {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = colour::BG;
    visuals.window_fill = colour::BG;
    // Every text field in the app is this colour: the search box and both
    // note boxes. An input has to look like somewhere to put something,
    // which means a step darker than the surface it sits on and an edge.
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
    // egui picks these up for any widget that does not name a font, which is
    // the safety net under the tokens above rather than a second opinion.
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

/// A label, in the app's voice: caps, properly tracked.
///
/// Caps without tracking is the single thing that reads as unfinished, and
/// tracking done by pushing thin spaces between letters — which is what this
/// used to do — is worse than none: it is measured as text, so it breaks
/// kerning, it breaks truncation, and the trailing gap throws every centred
/// label off by half a space. `extra_letter_spacing` is laid out properly,
/// so widths, wrapping and hit testing all agree with what is on screen.
///
/// The amount is a fraction of the size rather than a fixed number of
/// points, because tracking is an optical correction and a ten point label
/// needs less of it than a twenty point one.
#[must_use]
pub fn caps(text: &str, font: FontId, tint: Color32) -> egui::text::LayoutJob {
    let tracking = tracking_for(font.size);
    egui::text::LayoutJob::single_section(
        text.to_uppercase(),
        egui::TextFormat {
            font_id: font,
            extra_letter_spacing: tracking,
            color: tint,
            ..egui::TextFormat::default()
        },
    )
}

/// How much air to put between capitals, at a given size.
///
/// Not a flat fraction. Small caps need the help and large ones do not:
/// Riot's own display type is tracked by about a hundredth of an em at
/// ninety six points, which at ten points would be invisible, while ten
/// points of caps set solid is a word nobody can read. So it runs from a
/// sixteenth of an em at the bottom of the scale to a fiftieth at the top.
#[must_use]
pub fn tracking_for(points: f32) -> f32 {
    let t = ((points - size::MICRO) / (size::HERO - size::MICRO)).clamp(0.0, 1.0);
    points * (0.065 - 0.045 * t)
}

/// Paints a caps label where most call sites want one: and that is all.
///
/// The twin of [`caps_text`], which also hands back the rectangle it drew
/// into. Two entry points rather than one, because a painting call whose
/// result is usually irrelevant should not have to be ignored at every site,
/// and a rectangle that is sometimes load bearing should not be droppable by
/// accident.
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

/// Paints a caps label and says where it landed.
///
/// The same shape as [`egui::Painter::text`] so a call site reads the same
/// either way. The trailing tracking after the last letter is taken back off
/// the width, so a label right against an edge sits against the edge and a
/// centred one is actually centred.
#[must_use]
pub fn caps_text(
    painter: &egui::Painter,
    pos: egui::Pos2,
    anchor: egui::Align2,
    text: &str,
    font: FontId,
    tint: Color32,
) -> egui::Rect {
    let trailing = tracking_for(font.size);
    let galley = painter.layout_job(caps(text, font, tint));
    let size = egui::vec2((galley.size().x - trailing).max(0.0), galley.size().y);
    let rect = anchor.anchor_size(pos, size);
    painter.galley(rect.min, galley, tint);
    rect
}

#[cfg(test)]
mod tests {
    use super::{Face, caps, colour, install_fonts, kd, rank, size, space, tracking_for};

    /// The three faces have to be bound to something, loaded or not: a family
    /// that is registered nowhere panics inside epaint the moment a glyph in
    /// it is laid out, and that is the whole interface.
    #[test]
    fn every_face_is_bound() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        // Fonts only exist after a frame has run, so the check runs inside
        // one. A family bound to nothing panics in epaint the moment a glyph
        // is laid out, and that glyph is the whole interface.
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx();
            for face in [Face::Display, Face::Body, Face::Number] {
                let galley = ctx.fonts_mut(|fonts| {
                    fonts.layout_no_wrap("W".to_owned(), face.at(size::BODY), colour::TEXT)
                });
                assert!(galley.size().x > 0.0, "{face:?} laid out nothing");
            }
        });
        // The frame produced a font atlas that nothing is going to upload.
        // Saying so is required; dropping it silently is a panic.
        let mut output = output;
        output.textures_delta.clear();
    }

    #[test]
    fn unranked_tiers_are_not_in_the_palette() {
        for tier in 0..3 {
            assert_eq!(rank(Some(tier)), colour::TEXT_FAINT);
        }
        assert_eq!(rank(None), colour::TEXT_FAINT);
    }

    /// One rank, one colour, and a tier past the table does not fall back to
    /// something unrelated. Radiant is 27, and nothing above it exists.
    #[test]
    fn every_group_has_its_own_colour() {
        assert_ne!(rank(Some(12)), rank(Some(15)));
        assert_eq!(rank(Some(12)), rank(Some(14)), "a group shares one colour");
        assert_eq!(rank(Some(27)), rank(Some(999)));
    }

    #[test]
    fn a_missing_kd_is_not_coloured_like_a_bad_one() {
        assert_eq!(kd(None), colour::TEXT_FAINT);
        assert_ne!(kd(Some(0.5)), colour::TEXT_FAINT);
    }

    /// The grid is the point. A value off it is a guess, and this is the only
    /// place that can catch one being added.
    #[test]
    fn every_space_is_on_the_four_point_grid() {
        for value in [
            space::SM,
            space::MD,
            space::LG,
            space::XL,
            space::XXL,
            space::ROW,
        ] {
            let steps = value / 4.0;
            assert!(steps.fract() == 0.0, "{value} is not on the grid");
        }
    }

    /// A type scale is a ratio, not a list of sizes somebody liked.
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
