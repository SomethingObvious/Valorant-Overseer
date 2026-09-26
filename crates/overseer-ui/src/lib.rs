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
    /// The wordmark and the state of the game.
    pub const DISPLAY: f32 = 20.0;
    /// A panel's subject: a player's name, a team.
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
}

/// How long things take. Four durations, because a fifth would be a fifth
/// opinion about the same question, and the whole point of a system is that
/// the same question has one answer.
pub mod motion {
    /// A hover tint. Fast enough to feel like the cursor rather than an
    /// animation somebody wrote.
    pub const INSTANT: f32 = 0.06;
    /// A selection moving, a value changing.
    pub const QUICK: f32 = 0.12;

    /// The same durations when the window is being careful: fast enough to
    /// still say what changed, short enough to cost nothing.
    pub const EFFICIENT: f32 = 0.0;
}

/// Colour by the job it does, never by what it looks like.
///
/// The same values as `tui/src/theme.ts`, so the window and the terminal are
/// one product. `you` is bone rather than red on purpose: in game your own row
/// is the light one and the enemy is the red one, and a companion app that
/// swaps those is worse than one with no colour at all.
pub mod colour {
    use egui::Color32;

    /// The window's background.
    pub const BG: Color32 = Color32::from_rgb(0x0B, 0x11, 0x19);
    /// The window's background in overlay mode.
    ///
    /// The same colour as [`BG`], letting enough of the game through that you
    /// can tell it is an overlay and little enough that the text still reads
    /// at a glance. Only ever used as a clear colour, where the window itself
    /// is transparent. Premultiplied, because that is the only constructor
    /// that is const: each channel scaled by the 0xD8 alpha beside it.
    pub const BG_OVERLAY: Color32 = Color32::from_rgba_premultiplied(0x09, 0x0E, 0x15, 0xD8);
    /// A surface that sits above the background: a header, a panel.
    pub const BG_RAISED: Color32 = Color32::from_rgb(0x11, 0x1A, 0x24);
    /// The tint under the cursor.
    pub const BG_HOVER: Color32 = Color32::from_rgb(0x18, 0x24, 0x31);
    /// The tint on the row you have chosen.
    pub const BG_SELECTED: Color32 = Color32::from_rgb(0x1E, 0x2C, 0x3A);
    /// A rule that separates two things.
    pub const LINE: Color32 = Color32::from_rgb(0x2A, 0x39, 0x47);
    /// A rule between rows, which should be felt rather than seen.
    pub const LINE_SOFT: Color32 = Color32::from_rgb(0x18, 0x21, 0x2B);

    /// A name, a heading: the brightest text there is.
    pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xEC, 0xE8, 0xE1);
    /// Ordinary text.
    pub const TEXT: Color32 = Color32::from_rgb(0xD6, 0xDD, 0xE3);
    /// A label beside a value.
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x7E, 0x8C, 0x92);
    /// Present, but not the point.
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x55, 0x63, 0x6D);
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
    /// A bad number.
    pub const BAD: Color32 = Color32::from_rgb(0xFF, 0x80, 0x88);
}

/// One colour per rank group, in tier order, as the game shows them.
const RANKS: [Color32; 10] = [
    Color32::from_rgb(0x4A, 0x4A, 0x4A),
    Color32::from_rgb(0x5A, 0x57, 0x51),
    Color32::from_rgb(0xBB, 0x8F, 0x5A),
    Color32::from_rgb(0xAE, 0xB2, 0xB2),
    Color32::from_rgb(0xC5, 0xBA, 0x3F),
    Color32::from_rgb(0x18, 0xA7, 0xB9),
    Color32::from_rgb(0xD8, 0x64, 0xC7),
    Color32::from_rgb(0x18, 0x94, 0x52),
    Color32::from_rgb(0xDD, 0x44, 0x44),
    Color32::from_rgb(0xFF, 0xFD, 0xCD),
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

/// The three faces, by the job each one does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// A DIN, which is the family the game's own interface uses. Headings and
    /// labels, in caps.
    Display,
    /// The system's reading face. It disappears, which is what a name wants.
    Body,
    /// Tabular by construction, so a column of numbers cannot drift. This is
    /// the whole reason a third face exists.
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
}

/// Where Windows keeps the faces above, and what to fall back to.
///
/// Loaded from the system rather than vendored: all three ship with Windows 11,
/// which is the only platform this app runs on, so vendoring them would add a
/// megabyte to the repository and a licence file to maintain in order to
/// deliver a file that is already on the disk. A machine missing one falls back
/// to egui's bundled font, which is why every load is allowed to fail.
const FACES: [(Face, &str); 3] = [
    (Face::Display, "bahnschrift.ttf"),
    (Face::Body, "segoeui.ttf"),
    (Face::Number, "consola.ttf"),
];

/// Registers the faces, falling back quietly to what egui ships.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var_os("SystemRoot").map_or_else(
        || std::path::PathBuf::from("C:/Windows"),
        std::path::PathBuf::from,
    );
    for (face, file) in FACES {
        let path = dir.join("Fonts").join(file);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let key = face.key().to_owned();
        fonts.font_data.insert(
            key.clone(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
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
    // A face that failed to load still needs its family to exist, or every
    // call to Face::at draws nothing at all. The fallback list is read out
    // first because inserting into the map borrows it.
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (face, _) in FACES {
        fonts
            .families
            .entry(FontFamily::Name(face.key().into()))
            .or_insert_with(|| fallback.clone());
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
    visuals.extreme_bg_color = Color32::from_rgb(0x07, 0x0C, 0x12);
    visuals.faint_bg_color = colour::BG_RAISED;
    visuals.override_text_color = Some(colour::TEXT);
    visuals.window_stroke = Stroke::new(1.0, colour::LINE);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colour::LINE);
    visuals.widgets.inactive.bg_fill = colour::BG_RAISED;
    visuals.widgets.hovered.bg_fill = colour::BG_HOVER;
    visuals.widgets.active.bg_fill = colour::INFO;
    visuals.selection.bg_fill = colour::BG_SELECTED;
    visuals.selection.stroke = Stroke::new(1.0, colour::INFO);
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

/// A label, in the app's voice: caps, letterspaced, dim.
///
/// egui has no letterspacing, so the spaces are put in by hand. That is a
/// small ugliness in one function rather than at every call site, and caps
/// without tracking is the thing that reads as unfinished.
#[must_use]
pub fn label_text(text: &str) -> String {
    let upper = text.to_uppercase();
    let mut out = String::with_capacity(upper.len() * 2);
    for (index, ch) in upper.chars().enumerate() {
        if index > 0 {
            out.push('\u{2009}');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Face, colour, install_fonts, kd, label_text, rank, size, space};

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
        assert_eq!(label_text("rank"), "R\u{2009}A\u{2009}N\u{2009}K");
        assert_eq!(label_text(""), "");
    }
}
