//! The palette, shared with the terminal front end so the two read as one app.
//!
//! These are the same hex values as `tui/src/theme.ts`. When one moves, both
//! move: a rank that is gold in one window and yellow in the other is the sort
//! of thing that makes an app feel like two apps.

use egui::{Color32, CornerRadius, Stroke, Style, Visuals};

/// VALORANT red, and the app's one accent.
pub(crate) const RED: Color32 = Color32::from_rgb(0xFF, 0x46, 0x55);
/// Your team, and anything won.
pub(crate) const ALLY: Color32 = Color32::from_rgb(0x18, 0xE5, 0xA7);
/// Cold blue, for measurements rather than outcomes.
pub(crate) const ICE: Color32 = Color32::from_rgb(0x9A, 0xDE, 0xFF);
/// The brightest text, for a name.
pub(crate) const BONE: Color32 = Color32::from_rgb(0xEC, 0xE8, 0xE1);
/// Something worth noticing, such as a flag.
pub(crate) const GOLD: Color32 = Color32::from_rgb(0xFF, 0xB4, 0x54);
/// Ordinary text.
pub(crate) const TEXT: Color32 = Color32::from_rgb(0xD6, 0xDD, 0xE3);
/// A label beside a value.
pub(crate) const DIM: Color32 = Color32::from_rgb(0x7E, 0x8C, 0x92);
/// Present but not the point.
pub(crate) const FAINT: Color32 = Color32::from_rgb(0x55, 0x63, 0x6D);
/// Borders and rules.
pub(crate) const LINE: Color32 = Color32::from_rgb(0x2A, 0x39, 0x47);
/// The background, and the colour text sits on inside a bright button.
pub(crate) const INK: Color32 = Color32::from_rgb(0x0B, 0x11, 0x19);

/// One colour per rank group, in tier order, as the game shows them.
///
/// The tier is the key rather than the name, for the reason written into
/// `tui/src/theme.ts`: the name is not always sent, and a rank drawn in two
/// colours depending on which code path filled it in is unmissable once seen.
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
pub(crate) fn rank(tier: Option<u32>) -> Color32 {
    match tier {
        // Floor division on purpose: three tiers per group, so 12, 13 and 14 are
        // all Gold. Written as div_euclid rather than `/` because the intent is
        // the flooring, not a quotient that happens to be truncated.
        Some(t) if t >= 3 => *RANKS
            .get(t.div_euclid(3) as usize)
            .or_else(|| RANKS.last())
            .unwrap_or(&TEXT),
        _ => FAINT,
    }
}

/// The colour a K/D is worth saying out loud in.
#[must_use]
pub(crate) fn kd(value: Option<f64>) -> Color32 {
    match value {
        None => FAINT,
        Some(v) if v >= 1.3 => ALLY,
        Some(v) if v >= 1.0 => ICE,
        Some(v) if v >= 0.8 => BONE,
        Some(_) => Color32::from_rgb(0xFF, 0x80, 0x88),
    }
}

/// The app's style: flat, sharp, dark, one accent.
///
/// Sharp corners are not only the game's language, they are cheaper: a rounded
/// rectangle is a different mesh per radius, and this app draws a great many
/// rectangles.
#[must_use]
pub(crate) fn style() -> Style {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = INK;
    visuals.window_fill = INK;
    visuals.extreme_bg_color = Color32::from_rgb(0x07, 0x0C, 0x12);
    visuals.faint_bg_color = Color32::from_rgb(0x12, 0x1A, 0x24);
    visuals.override_text_color = Some(TEXT);
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x1B, 0x27, 0x33);
    visuals.widgets.hovered.bg_fill = LINE;
    visuals.widgets.active.bg_fill = ICE;
    visuals.selection.bg_fill = Color32::from_rgb(0x1E, 0x2C, 0x3A);
    visuals.selection.stroke = Stroke::new(1.0, ICE);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::ZERO;
    }
    visuals.window_corner_radius = CornerRadius::ZERO;
    visuals.menu_corner_radius = CornerRadius::ZERO;

    let mut style = Style {
        visuals,
        ..Style::default()
    };
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(0);
    style.interaction.selectable_labels = false;
    style
}

#[cfg(test)]
mod tests {
    use super::{FAINT, RANKS, kd, rank};

    #[test]
    fn unranked_tiers_are_not_in_the_palette() {
        for tier in 0..3 {
            assert_eq!(rank(Some(tier)), FAINT);
        }
        assert_eq!(rank(None), FAINT);
    }

    /// One rank, one colour, and a tier past the table does not fall back to
    /// something unrelated. Radiant is 27, and nothing above it exists.
    #[test]
    fn every_group_has_its_own_colour() {
        let gold = rank(Some(12));
        let plat = rank(Some(15));
        assert_ne!(gold, plat);
        assert_eq!(gold, rank(Some(14)), "a group shares one colour");
        assert_eq!(rank(Some(27)), *RANKS.last().unwrap());
        assert_eq!(rank(Some(999)), *RANKS.last().unwrap());
    }

    #[test]
    fn a_missing_kd_is_not_coloured_like_a_bad_one() {
        assert_eq!(kd(None), FAINT);
        assert_ne!(kd(Some(0.5)), FAINT);
    }
}
