//! The pictures a lineup carries: a few thumbnails in the panel, and one at a
//! time big in the middle of the window over a dark backdrop.

use egui::{
    Align2, Color32, CursorIcon, Id, Key, Modifiers, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2,
};
use overseer_ui::{Face, art, colour, size, space};

use super::player::cross;

/// How many thumbnails share a row.
const ACROSS: usize = 3;

/// The kinds of file a picture can be added from, by extension.
pub(super) const KINDS: [&str; 6] = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// The most pictures one lineup carries, the same as the backend keeps.
pub(super) const MOST: usize = 8;

/// Whether `path` names a picture that can be added, by its extension.
pub(super) fn is_picture(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| KINDS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Thumbnails of `pictures`, three to a row, each filling its tile with the
/// middle of its picture. With `removable` each has an X in its corner.
/// Returns the one clicked, and the one whose X was clicked.
pub(super) fn thumbnails(
    ui: &mut Ui,
    pictures: &[String],
    removable: bool,
) -> (Option<usize>, Option<usize>) {
    let gap = space::SM;
    let tile = gap.mul_add(-(ACROSS as f32 - 1.0), ui.available_width()) / ACROSS as f32;
    let (mut opened, mut removed) = (None, None);
    for (row, chunk) in pictures.chunks(ACROSS).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (column, path) in chunk.iter().enumerate() {
                let index = row * ACROSS + column;
                let (rect, response) =
                    ui.allocate_exact_size(vec2(tile, tile * 0.62), Sense::click());
                let response = response.on_hover_cursor(CursorIcon::PointingHand);
                if ui.is_rect_visible(rect) {
                    tile_picture(ui, rect, path, response.hovered());
                }
                if removable {
                    let spot = Rect::from_center_size(
                        rect.right_top() + vec2(-12.0, 12.0),
                        vec2(20.0, 20.0),
                    );
                    let x = ui
                        .interact(
                            spot,
                            ui.id().with(("picture-remove", index)),
                            Sense::click(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    cross(ui.painter(), spot, x.hovered());
                    if x.clicked() {
                        removed = Some(index);
                    }
                }
                if response.clicked() && removed.is_none() {
                    opened = Some(index);
                }
            }
        });
    }
    (opened, removed)
}

/// One thumbnail in `rect`, or a word saying the file is gone.
fn tile_picture(ui: &Ui, rect: Rect, path: &str, hot: bool) {
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, colour::BG_INSET);
    match art::file(ui.ctx(), path) {
        Some(texture) => {
            painter.image(
                texture.id(),
                rect,
                cover(texture.size_vec2(), rect.size()),
                Color32::WHITE,
            );
        }
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Missing",
                Face::Body.at(size::MICRO),
                colour::TEXT_FAINT,
            );
        }
    }
    if hot {
        painter.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.5, colour::TEXT_STRONG),
            egui::StrokeKind::Inside,
        );
    }
}

/// The middle of a picture `size` that fills a tile `tile` without
/// stretching, as the part of the picture to draw.
fn cover(size: Vec2, tile: Vec2) -> Rect {
    let picture = size.x / size.y.max(1.0);
    let wanted = tile.x / tile.y.max(1.0);
    if picture > wanted {
        let share = wanted / picture;
        Rect::from_min_max(
            pos2((1.0 - share) / 2.0, 0.0),
            pos2(f32::midpoint(1.0, share), 1.0),
        )
    } else {
        let share = picture / wanted;
        Rect::from_min_max(
            pos2(0.0, (1.0 - share) / 2.0),
            pos2(1.0, f32::midpoint(1.0, share)),
        )
    }
}

/// The picture at `at` of `pictures` big in the middle of the window, with
/// an X in its corner and, when there are more, arrows either side and the
/// arrow keys to go between them. The X, a click on the backdrop or Escape
/// close it. Returns which one to show next frame, or nothing once closed.
pub(super) fn viewer(ctx: &egui::Context, pictures: &[String], at: usize) -> Option<usize> {
    let count = pictures.len();
    if count == 0 {
        return None;
    }
    let mut at = at.min(count - 1);
    // Taken here, so the clip behind doesn't skip on the same keys.
    let (back, on) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowLeft),
            i.consume_key(Modifiers::NONE, Key::ArrowRight),
        )
    });
    if back {
        at = (at + count - 1) % count;
    }
    if on {
        at = (at + 1) % count;
    }
    let room = ctx.content_rect().shrink(48.0);
    let mut close = false;
    let shown = egui::Modal::new(Id::new("lineup-picture"))
        .frame(egui::Frame::NONE)
        .backdrop_color(Color32::from_black_alpha(220))
        .show(ctx, |ui| {
            let texture = pictures.get(at).and_then(|p| art::file(ui.ctx(), p));
            let size = texture
                .as_ref()
                .map_or(vec2(16.0, 9.0), egui::TextureHandle::size_vec2);
            let scale = (room.width() / size.x).min(room.height() / size.y);
            let (rect, _) = ui.allocate_exact_size(size * scale, Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, 0, Color32::BLACK);
            if let Some(texture) = &texture {
                let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.image(texture.id(), rect, whole, Color32::WHITE);
            }
            let spot =
                Rect::from_center_size(rect.right_top() + vec2(-26.0, 26.0), vec2(34.0, 34.0));
            let x = ui
                .interact(spot, Id::new("lineup-picture-close"), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            cross(ui.painter(), spot, x.hovered());
            close = x.clicked();
            if count > 1 {
                at = arrows(ui, rect, (at, count));
            }
        });
    (!(close || shown.should_close())).then_some(at)
}

/// The arrows either side of a picture in `rect`, and which of them it is,
/// like 2 / 5, under it. Returns which picture to show after a click.
fn arrows(ui: &Ui, rect: Rect, (at, count): (usize, usize)) -> usize {
    let mut next = at;
    for (left, step) in [(true, count - 1), (false, 1)] {
        let x = if left {
            rect.left() + 32.0
        } else {
            rect.right() - 32.0
        };
        let spot = Rect::from_center_size(pos2(x, rect.center().y), vec2(44.0, 44.0));
        let arrow = ui
            .interact(spot, Id::new(("lineup-picture-step", left)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        chevron(ui.painter(), spot, left, arrow.hovered());
        if arrow.clicked() {
            next = (at + step) % count;
        }
    }
    let label = pos2(rect.center().x, rect.bottom() - 18.0);
    ui.painter().text(
        label,
        Align2::CENTER_CENTER,
        format!("{} / {count}", at + 1),
        Face::Body.at(size::BODY),
        colour::TEXT_STRONG,
    );
    next
}

/// A dark disc with an arrowhead pointing left or right, brighter under the
/// pointer.
fn chevron(painter: &egui::Painter, rect: Rect, left: bool, hot: bool) {
    painter.circle_filled(
        rect.center(),
        rect.width() / 2.0,
        Color32::from_black_alpha(if hot { 230 } else { 160 }),
    );
    let ink = if hot {
        colour::TEXT_STRONG
    } else {
        colour::TEXT
    };
    let c = rect.center();
    let point = if left { -1.0 } else { 1.0 };
    let stroke = Stroke::new(2.5, ink);
    painter.line_segment(
        [c + vec2(-4.0 * point, -8.0), c + vec2(4.0 * point, 0.0)],
        stroke,
    );
    painter.line_segment(
        [c + vec2(4.0 * point, 0.0), c + vec2(-4.0 * point, 8.0)],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::{cover, is_picture};
    use egui::vec2;

    /// A wide picture shows its middle in a narrower tile, a tall one its
    /// middle in a wider tile, and one already the tile's shape all of it.
    #[test]
    fn a_thumbnail_shows_the_middle_of_its_picture() {
        let wide = cover(vec2(200.0, 100.0), vec2(100.0, 100.0));
        assert!((wide.min.x - 0.25).abs() < 1e-5 && (wide.max.x - 0.75).abs() < 1e-5);
        let tall = cover(vec2(100.0, 200.0), vec2(100.0, 100.0));
        assert!((tall.min.y - 0.25).abs() < 1e-5 && (tall.max.y - 0.75).abs() < 1e-5);
        let same = cover(vec2(160.0, 100.0), vec2(80.0, 50.0));
        assert!(same.min.x.abs() < 1e-5 && (same.max.x - 1.0).abs() < 1e-5);
        assert!(is_picture(std::path::Path::new("C:/shots/Smoke.JPG")));
        assert!(!is_picture(std::path::Path::new("C:/shots/clip.mp4")));
    }
}
