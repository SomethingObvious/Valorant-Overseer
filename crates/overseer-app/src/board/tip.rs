//! The card a tag opens into when the pointer rests on it: what sort of tag
//! it is, the word, what it means, and the numbers it came from, with a bar
//! when those numbers are a share of something.

use egui::{Color32, Ui, pos2, vec2};
use overseer_ui::{Face, caps_text, colour, size, space};

use super::paint;

/// Everything one card says.
pub(crate) struct Card<'a> {
    /// What sort of tag it is: weapon, economy, duels, and so on.
    pub(crate) kind: &'a str,
    /// The tag itself.
    pub(crate) word: &'a str,
    /// What the word means, for somebody meeting it for the first time.
    pub(crate) means: &'a str,
    /// What it was read from, with its numbers.
    pub(crate) evidence: &'a str,
    /// The colour down its edge.
    pub(crate) accent: Color32,
}

/// The card for a tag the backend worked out, in the colour of whose it is.
pub(crate) fn auto(ui: &mut Ui, tag: &overseer_core::AutoTag, accent: Color32) {
    show(
        ui,
        &Card {
            kind: tag.kind.as_deref().unwrap_or("Read"),
            word: tag.tag.as_deref().unwrap_or(""),
            means: tag.means.as_deref().unwrap_or(""),
            evidence: tag.why.as_deref().unwrap_or(""),
            accent,
        },
    );
}

/// How wide a card's words run before they wrap.
const WIDE: f32 = 250.0;

/// One full-width line of the card, `tall` points high.
fn row(ui: &mut Ui, tall: f32) -> egui::Rect {
    let (rect, _response) = ui.allocate_exact_size(
        vec2(2.0f32.mul_add(space::LG, WIDE), tall),
        egui::Sense::hover(),
    );
    rect
}

/// Draws a card into a tooltip's ui.
pub(crate) fn show(ui: &mut Ui, card: &Card<'_>) {
    ui.set_width(2.0f32.mul_add(space::LG, WIDE));
    let top = ui.cursor().top();
    ui.add_space(space::MD);
    let rect = row(ui, 14.0);
    let _kind = caps_text(
        ui.painter(),
        pos2(rect.left() + space::LG, rect.center().y),
        egui::Align2::LEFT_CENTER,
        card.kind,
        Face::Display.at(size::MICRO),
        colour::TEXT_FAINT,
    );
    let rect = row(ui, 26.0);
    let _word = caps_text(
        ui.painter(),
        pos2(rect.left() + space::LG, rect.center().y),
        egui::Align2::LEFT_CENTER,
        card.word,
        Face::Heavy.at(22.0),
        colour::TEXT_STRONG,
    );
    if !card.means.is_empty() {
        paragraph(
            ui,
            card.means,
            &Face::Body.at(size::BODY),
            colour::TEXT,
            None,
        );
    }
    if !card.evidence.is_empty() {
        evidence(ui, card);
    }
    ui.add_space(space::MD);
    // The edge runs flush with the tooltip's frame, its full height. The
    // tooltip's margin sits outside this ui, so the edge is drawn on the
    // layer rather than clipped to the content.
    let margin = ui.style().spacing.menu_margin;
    let bottom = ui.cursor().top();
    let edge = egui::Rect::from_min_max(
        pos2(
            ui.max_rect().left() - f32::from(margin.left),
            top - f32::from(margin.top),
        ),
        pos2(
            ui.max_rect().left() - f32::from(margin.left) + 3.0,
            bottom + f32::from(margin.bottom),
        ),
    );
    ui.ctx()
        .layer_painter(ui.layer_id())
        .rect_filled(edge, 0, card.accent);
}

/// Under a rule, what the tag was read from, its numbers in the accent,
/// and a bar when they are a share of something.
fn evidence(ui: &mut Ui, card: &Card<'_>) {
    ui.add_space(space::SM);
    let rect = row(ui, 9.0);
    ui.painter().hline(
        rect.left() + space::LG..=rect.right() - space::LG,
        rect.center().y,
        (1.0, colour::LINE),
    );
    paragraph(
        ui,
        card.evidence,
        &Face::Body.at(size::MICRO + 1.0),
        colour::TEXT_DIM,
        Some(card.accent),
    );
    if let Some(share) = share(card.evidence) {
        ui.add_space(space::SM);
        let rect = row(ui, 5.0);
        let track = egui::Rect::from_min_max(
            pos2(rect.left() + space::LG, rect.top()),
            pos2(rect.right() - space::LG, rect.bottom()),
        );
        ui.painter()
            .add(paint::slant(track, false, true, colour::BG_INSET));
        let fill = egui::Rect::from_min_size(
            track.min,
            vec2((track.width() * share).max(space::SM), track.height()),
        );
        ui.painter()
            .add(paint::slant(fill, false, true, card.accent));
    }
}

/// A wrapped paragraph, its numbers set brighter when `numbers` says in
/// what, the way a flag's reasons are.
fn paragraph(
    ui: &mut Ui,
    text: &str,
    font: &egui::FontId,
    tint: Color32,
    numbers: Option<Color32>,
) {
    let mut job = egui::text::LayoutJob::default();
    for (piece, is_number) in paint::split_numbers(text) {
        let ink = match numbers {
            Some(bright) if is_number => bright,
            _ => tint,
        };
        job.append(piece, 0.0, egui::TextFormat::simple(font.clone(), ink));
    }
    job.wrap.max_width = WIDE;
    let galley = ui.painter().layout_job(job);
    let rect = row(ui, galley.size().y + 2.0);
    ui.painter().galley(
        pos2(rect.left() + space::LG, rect.top() + 1.0),
        galley,
        tint,
    );
}

/// The share in "31 of 110", if the evidence has one.
fn share(text: &str) -> Option<f32> {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.windows(3).find_map(|w| match w {
        [part, "of", whole] => {
            let part: f32 = part.parse().ok()?;
            let whole: f32 = whole.trim_end_matches(',').parse().ok()?;
            (whole > 0.0 && part <= whole).then_some(part / whole)
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::share;

    /// The bar is drawn from the first "part of whole" in the evidence,
    /// and from nothing that only looks like one.
    #[test]
    fn a_share_is_read_from_its_words() {
        assert_eq!(
            share("Carried the Operator in 31 of 110 rounds over their last 5 games"),
            Some(31.0 / 110.0)
        );
        assert_eq!(
            share("Bought a gun after 3 of 4 lost pistol rounds"),
            Some(0.75)
        );
        assert_eq!(share("Won their last 4 games"), None);
        assert_eq!(share("one of the best"), None);
        assert_eq!(share("5 of 0 rounds"), None);
    }
}
