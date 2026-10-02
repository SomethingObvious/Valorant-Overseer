//! Riot's portraits, killfeed crops, cards, map strips and rank emblems, bound
//! into the binary since the app never goes online, and decoded on first use.

use egui::{Context, TextureHandle, TextureOptions};

use crate::art_assets::{CARDS, KILLFEED, MAPS, PORTRAITS, RANKS};

/// The square portrait for an agent, or `None` for one newer than this build,
/// which callers draw as a plate with its initial.
#[must_use]
pub fn agent(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "agent", &PORTRAITS, name)
}

/// The killfeed crop for an agent, two wide by one tall and tight on the face.
#[must_use]
pub fn killfeed(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "killfeed", &KILLFEED, name)
}

/// An agent's card for the top of the panel, eyes to chin at 2.4 to 1, since
/// the 256 pixel killfeed crop goes soft stretched that wide.
#[must_use]
pub fn card(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "card", &CARDS, name)
}

/// A map's band of art, for behind the header.
#[must_use]
pub fn map(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "map", &MAPS, name)
}

/// The emblem for a tier, if it is one Riot has an emblem for.
#[must_use]
pub fn rank(ctx: &Context, tier: u32) -> Option<TextureHandle> {
    texture(ctx, &format!("rank:{tier}"), emblem(tier)?)
}

/// A tier's emblem blurred and white with [`GLOW_PAD`] of room each side, for
/// the caller to tint as the light behind it.
#[must_use]
pub fn rank_glow(ctx: &Context, tier: u32) -> Option<TextureHandle> {
    let bytes = emblem(tier)?;
    cached(ctx, &format!("rank-glow:{tier}"), || {
        decode(bytes).map(|e| bloom(&e))
    })
}

/// The PNG for a tier's emblem.
fn emblem(tier: u32) -> Option<&'static [u8]> {
    RANKS
        .iter()
        .find(|(known, _)| *known == tier)
        .map(|(_, bytes)| *bytes)
}

/// Makes every emblem and its glow on a thread at startup, because blurring
/// ten of them on first use drops frames while the lobby animates in.
pub fn warm(ctx: &Context) {
    let ctx = ctx.clone();
    let _warming = std::thread::Builder::new()
        .name("overseer-art".to_owned())
        .spawn(move || {
            for (tier, _bytes) in RANKS {
                let _emblem = rank(&ctx, tier);
                let _glow = rank_glow(&ctx, tier);
            }
        });
}

/// How far the glow reaches past the emblem on every side, in the emblem's
/// pixels.
pub const GLOW_PAD: usize = 48;

/// The emblem's alpha in white, spread by three box blurs, which comes close
/// to a gaussian.
fn bloom(emblem: &egui::ColorImage) -> egui::ColorImage {
    // Wide enough to read as light, and narrow enough to fade out before the
    // edge of the row that clips it.
    const RADIUS: usize = 14;
    let [w, h] = emblem.size;
    let (width, height) = (w + 2 * GLOW_PAD, h + 2 * GLOW_PAD);
    let mut alpha = vec![0.0_f32; width * height];
    for (row, source) in alpha
        .chunks_mut(width)
        .skip(GLOW_PAD)
        .zip(emblem.pixels.chunks(w))
    {
        for (to, pixel) in row.iter_mut().skip(GLOW_PAD).zip(source) {
            *to = f32::from(pixel.a()) / 255.0;
        }
    }
    for _pass in 0..3 {
        blur_rows(&mut alpha, width, RADIUS);
        let mut down = transpose(&alpha, width);
        blur_rows(&mut down, height, RADIUS);
        alpha = transpose(&down, height);
    }
    let pixels = alpha
        .iter()
        .map(|a| egui::Color32::from_white_alpha((a.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    egui::ColorImage::new([width, height], pixels)
}

/// One box blur of `radius` along every row, with nothing past either end.
fn blur_rows(values: &mut [f32], width: usize, radius: usize) {
    let span = (2 * radius + 1) as f32;
    for row in values.chunks_mut(width) {
        let sums: Vec<f32> = std::iter::once(0.0)
            .chain(row.iter().scan(0.0, |total, v| {
                *total += v;
                Some(*total)
            }))
            .collect();
        let sum = |at: usize| sums.get(at).copied().unwrap_or(0.0);
        for (j, out) in row.iter_mut().enumerate() {
            let (from, to) = (j.saturating_sub(radius), (j + radius + 1).min(width));
            *out = (sum(to) - sum(from)) / span;
        }
    }
}

/// Rows become columns, for a picture `width` wide.
fn transpose(values: &[f32], width: usize) -> Vec<f32> {
    (0..width)
        .flat_map(|x| values.iter().skip(x).step_by(width).copied())
        .collect()
}

/// The app's mark, the lock-on, at icon size.
const LOGO: &[u8] = include_bytes!("../assets/logo.png");

/// The mark as the window icon for the taskbar and Alt Tab, in place of
/// egui's own logo.
#[must_use]
pub fn logo_icon() -> Option<egui::IconData> {
    let image = decode(LOGO)?;
    let [width, height] = image.size;
    Some(egui::IconData {
        rgba: image
            .pixels
            .iter()
            .flat_map(egui::Color32::to_srgba_unmultiplied)
            .collect(),
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
    })
}

/// How much taller than the capitals beside it the mark is drawn in a
/// title bar, centred on them.
pub const MARK_OVER_CAPS: f32 = 1.4;

/// The mark cut to its own edges, for drawing inside the window.
#[must_use]
pub fn mark(ctx: &Context) -> Option<TextureHandle> {
    texture(ctx, "mark", include_bytes!("../assets/mark.png"))
}

/// One of the tables keyed by name, looked up by what the backend sent.
fn named(ctx: &Context, kind: &str, table: &[(&str, &[u8])], name: &str) -> Option<TextureHandle> {
    let key = slug(name);
    let bytes = table
        .iter()
        .find(|(known, _)| *known == key)
        .map(|(_, bytes)| *bytes)?;
    texture(ctx, &format!("{kind}:{key}"), bytes)
}

/// A picture from a local file, for the player cards the backend fetches. A
/// file that fails is remembered, or it would be read again every frame.
#[must_use]
pub fn file(ctx: &Context, path: &str) -> Option<TextureHandle> {
    let failed = egui::Id::new(("art-failed", path));
    if ctx.data(|store| store.get_temp::<bool>(failed)).is_some() {
        return None;
    }
    let found = cached(ctx, &format!("file:{path}"), || {
        std::fs::read(path).ok().and_then(|bytes| decode(&bytes))
    });
    if found.is_none() {
        ctx.data_mut(|store| store.insert_temp(failed, true));
    }
    found
}

/// The pixels of a local picture, for code that reads them rather than draws
/// them. Decoded once and kept, a file that fails included.
#[must_use]
pub fn file_pixels(ctx: &Context, path: &str) -> Option<std::sync::Arc<egui::ColorImage>> {
    let id = egui::Id::new(("art-pixels", path));
    if let Some(kept) =
        ctx.data(|store| store.get_temp::<Option<std::sync::Arc<egui::ColorImage>>>(id))
    {
        return kept;
    }
    let image = std::fs::read(path)
        .ok()
        .and_then(|bytes| decode(&bytes))
        .map(std::sync::Arc::new);
    ctx.data_mut(|store| store.insert_temp(id, image.clone()));
    image
}

/// Only the pixels of a local picture that `keep` picks, in white, and the
/// rest clear, for lighting part of a picture in a colour of its own. `name`
/// tells apart two picks from the same file.
#[must_use]
pub fn file_where(
    ctx: &Context,
    path: &str,
    name: &str,
    keep: fn(egui::Color32) -> bool,
) -> Option<TextureHandle> {
    let failed = egui::Id::new(("art-failed", name, path));
    if ctx.data(|store| store.get_temp::<bool>(failed)).is_some() {
        return None;
    }
    let found = cached(ctx, &format!("file-where:{name}:{path}"), || {
        let image = std::fs::read(path).ok().and_then(|bytes| decode(&bytes))?;
        let pixels = image
            .pixels
            .iter()
            .map(|p| {
                if keep(*p) {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::TRANSPARENT
                }
            })
            .collect();
        Some(egui::ColorImage::new(image.size, pixels))
    });
    if found.is_none() {
        ctx.data_mut(|store| store.insert_temp(failed, true));
    }
    found
}

/// Writes a picture to a PNG file, for a map somebody exports.
///
/// # Errors
///
/// When the file can't be created or written.
pub fn save_png(path: &std::path::Path, image: &egui::ColorImage) -> std::io::Result<()> {
    let [width, height] = image.size;
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(
        file,
        u32::try_from(width).map_err(std::io::Error::other)?,
        u32::try_from(height).map_err(std::io::Error::other)?,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_srgba_unmultiplied)
        .collect();
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&bytes))
        .map_err(std::io::Error::other)
}

/// Riot's default player card, for a row whose player has no card of their own.
#[must_use]
pub fn default_card(ctx: &Context) -> Option<TextureHandle> {
    texture(
        ctx,
        "default-card",
        include_bytes!("../assets/default-card.png"),
    )
}

/// Some agent's portrait picked by `seed`, for the blacked out figure of a
/// player who has not picked yet, so five of them are not one shape.
#[must_use]
pub fn stand_in_portrait(ctx: &Context, seed: u64) -> Option<TextureHandle> {
    pick(ctx, "agent", &PORTRAITS, seed)
}

/// One entry of a table, picked by `seed`.
fn pick(ctx: &Context, kind: &str, table: &[(&str, &[u8])], seed: u64) -> Option<TextureHandle> {
    let count = u64::try_from(table.len()).ok()?.max(1);
    let (name, bytes) = table.get(usize::try_from(seed % count).ok()?)?;
    texture(ctx, &format!("{kind}:{name}"), bytes)
}

/// The name the backend sent as a lookup key, so `KAY/O` is `kayo`.
fn slug(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// Decodes a PNG the first time it is asked for and hands back the texture
/// every time after.
fn texture(ctx: &Context, key: &str, bytes: &[u8]) -> Option<TextureHandle> {
    cached(ctx, key, || decode(bytes))
}

/// A texture made the first time it is asked for and handed back every time
/// after.
fn cached(
    ctx: &Context,
    key: &str,
    make: impl FnOnce() -> Option<egui::ColorImage>,
) -> Option<TextureHandle> {
    // Kept in the context's store, not a static, because a texture id only
    // means something to the context that made it, and the snapshot harness
    // builds contexts of its own.
    let id = egui::Id::new(("art", key));
    if let Some(handle) = ctx.data(|store| store.get_temp::<TextureHandle>(id)) {
        return Some(handle);
    }
    // Linear, because these are drawn at about half their stored size on a
    // display of any scale.
    let handle = ctx.load_texture(key, make()?, TextureOptions::LINEAR);
    ctx.data_mut(|store| store.insert_temp(id, handle.clone()));
    Some(handle)
}

/// One PNG as RGBA, or `None` for anything but 8 bit RGB or RGBA, which
/// callers draw as a fallback plate.
fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // The map strips are 64 colour palette images, 9 KB each instead of 300
    // as RGBA.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buffer.truncate(info.buffer_size());
    let size = [info.width as usize, info.height as usize];
    match info.color_type {
        png::ColorType::Rgba => Some(egui::ColorImage::from_rgba_unmultiplied(size, &buffer)),
        png::ColorType::Rgb => Some(egui::ColorImage::from_rgb(size, &buffer)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{CARDS, GLOW_PAD, KILLFEED, MAPS, PORTRAITS, RANKS, bloom, decode, emblem, slug};

    /// The slug has to survive the one agent with punctuation in its name.
    #[test]
    fn a_name_becomes_the_key_it_is_filed_under() {
        assert_eq!(slug("KAY/O"), "kayo");
        assert_eq!(slug("Clove"), "clove");
        assert_eq!(slug("The Range"), "therange");
    }

    /// The mark decodes, square, as the window's icon.
    #[test]
    fn the_logo_is_an_icon() {
        let icon = super::logo_icon().expect("the logo decodes");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }

    /// A picture that fails to decode just goes missing, so this is where it
    /// gets caught.
    #[test]
    fn every_bound_picture_decodes() {
        for table in [&PORTRAITS[..], &KILLFEED[..], &CARDS[..], &MAPS[..]] {
            for (name, bytes) in table {
                assert!(decode(bytes).is_some(), "{name} did not decode");
            }
        }
        for (tier, bytes) in RANKS {
            assert!(decode(bytes).is_some(), "tier {tier} did not decode");
        }
    }

    /// Brightest on the emblem and gone by the edge of its own picture, where
    /// a hard cut would show.
    #[test]
    fn the_glow_falls_off_like_light() {
        let bytes = emblem(12).expect("gold has an emblem");
        let glow = bloom(&decode(bytes).expect("an emblem decodes"));
        let [w, h] = glow.size;
        let at = |x: usize, y: usize| glow.pixels.get(y * w + x).map_or(0, egui::Color32::a);
        let (mx, my) = (w.div_ceil(2), h.div_ceil(2));
        assert!(at(mx, my) > 200, "the middle is {}", at(mx, my));
        assert!(at(mx, my) > at(mx, GLOW_PAD.div_ceil(2) + 8));
        for x in 0..w {
            assert_eq!(at(x, 0), 0, "the top edge is lit at {x}");
            assert_eq!(at(x, h - 1), 0, "the bottom edge is lit at {x}");
        }
    }

    /// Otherwise the board and the panel disagree about who has a face.
    #[test]
    fn every_agent_has_all_three_pictures() {
        let portraits: Vec<&str> = PORTRAITS.iter().map(|(name, _)| *name).collect();
        let crops: Vec<&str> = KILLFEED.iter().map(|(name, _)| *name).collect();
        let cards: Vec<&str> = CARDS.iter().map(|(name, _)| *name).collect();
        assert_eq!(portraits, crops);
        assert_eq!(portraits, cards);
    }

    /// The tables are keyed by the slug, so a key that is not already one
    /// could never be found.
    #[test]
    fn every_key_is_already_a_slug() {
        for table in [&PORTRAITS[..], &KILLFEED[..], &CARDS[..], &MAPS[..]] {
            for (name, _) in table {
                assert_eq!(slug(name), *name);
            }
        }
    }
}
