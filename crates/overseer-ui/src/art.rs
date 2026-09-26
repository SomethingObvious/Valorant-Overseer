//! The pictures: Riot's agent portraits, killfeed crops, map strips and rank
//! emblems, decoded once and kept as textures for as long as the window is
//! open.
//!
//! Every scoreboard anybody actually uses shows the agent's face and the
//! rank's emblem, and a broadcast shows them large. A letter is a thing to
//! read; a face is a thing you already know, and after two matches you read a
//! lobby's composition off the left edge of the board without reading a word.
//!
//! Bound into the binary rather than fetched, because this app does not go
//! online and is not going to start. Decoded on first use rather than at
//! startup, because a lobby is ten agents out of twenty nine and one map out
//! of twenty six, and the rest are work nobody asked for.

use egui::{Context, TextureHandle, TextureOptions};

use crate::art_assets::{CARDS, KILLFEED, MAPS, PORTRAITS, RANKS};

/// The square portrait for an agent, for the panel and the ladder.
///
/// Returns nothing for an agent added to the game after this build shipped,
/// and every call site falls back to a coloured plate with the initial on
/// it. A missing picture is a plate with a letter on it, not a hole.
#[must_use]
pub fn agent(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "agent", &PORTRAITS, name)
}

/// The killfeed crop for an agent: two wide by one tall, cut tight on the
/// face, which is what a broadcast puts at the head of each player's row.
#[must_use]
pub fn killfeed(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "killfeed", &KILLFEED, name)
}

/// An agent's player card: eyes to chin at 2.4 wide to 1 tall, cut from
/// Riot's full portrait for the top of the panel.
///
/// The panel used to stretch the killfeed crop across itself, which is 256
/// pixels wide and came out visibly soft at twice that.
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
    let bytes = RANKS
        .iter()
        .find(|(known, _)| *known == tier)
        .map(|(_, bytes)| *bytes)?;
    texture(ctx, &format!("rank:{tier}"), bytes)
}

/// The light behind a tier's emblem: the emblem's own silhouette, blurred
/// and white, for the caller to tint with the tier's colour.
///
/// A disc of colour behind the emblem read as a smudge: it had an edge you
/// could see and it lit the empty corners of a square. A bloom of the shape
/// itself is how the game lights a rank, and it falls off the way light
/// does. Made once per tier from the emblem already in the binary, at the
/// emblem's pixel scale with [`GLOW_PAD`] of room on every side.
#[must_use]
pub fn rank_glow(ctx: &Context, tier: u32) -> Option<TextureHandle> {
    let bytes = RANKS
        .iter()
        .find(|(known, _)| *known == tier)
        .map(|(_, bytes)| *bytes)?;
    cached(ctx, &format!("rank-glow:{tier}"), || {
        decode(bytes).map(|e| bloom(&e))
    })
}

/// How far the glow reaches past the emblem, in the emblem's pixels. The
/// glow's picture is this much bigger than the emblem's on every side.
pub const GLOW_PAD: usize = 48;

/// The emblem's alpha, spread by three box blurs (close to a gaussian) and
/// set in white.
///
/// Wide enough to read as light rather than as an outline, and narrow enough
/// to be gone before the edge of the row it is clipped to, where a glow
/// still lit would end in a hard line.
fn bloom(emblem: &egui::ColorImage) -> egui::ColorImage {
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

/// One of the tables keyed by name, looked up by what the backend sent.
fn named(ctx: &Context, kind: &str, table: &[(&str, &[u8])], name: &str) -> Option<TextureHandle> {
    let key = slug(name);
    let bytes = table
        .iter()
        .find(|(known, _)| *known == key)
        .map(|(_, bytes)| *bytes)?;
    texture(ctx, &format!("{kind}:{key}"), bytes)
}

/// The name the backend sent, reduced to what can be looked up.
///
/// `KAY/O` is `kayo`, and so is `kay/o`. Riot's own naming has a slash in
/// one agent and nothing else unusual, but a lookup that breaks on the next
/// punctuation mark they invent is a lookup that will break.
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
    // Kept in the context's own store rather than in a static of this
    // module. A texture id only means anything to the context that handed it
    // out, and a cache shared between contexts hands the second one an id
    // that belongs to the first: the snapshot harness builds its own context
    // and got a board of agents wearing each other's faces.
    let id = egui::Id::new(("art", key));
    if let Some(handle) = ctx.data(|store| store.get_temp::<TextureHandle>(id)) {
        return Some(handle);
    }
    // Linear filtering both ways: these are drawn at about half the size
    // they are stored at and the window can be on a display at any scale, so
    // there is no size at which nearest would be the honest one.
    let handle = ctx.load_texture(key, make()?, TextureOptions::LINEAR);
    ctx.data_mut(|store| store.insert_temp(id, handle.clone()));
    Some(handle)
}

/// One PNG, as RGBA.
///
/// The portraits and emblems are straight RGBA; the map strips are palette
/// images, because a photograph behind the header costs three hundred
/// kilobytes as RGBA and nine as sixty four colours, and nobody can see the
/// difference through the dimming. Expansion turns either into eight bit
/// channels, and three channels get an opaque alpha.
///
/// Anything else returns nothing rather than panicking. These are files this
/// repository wrote and checked, so a failure here means somebody replaced
/// one by hand, and the right answer to that is the fallback plate rather
/// than a window that will not open.
fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
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
    use super::{CARDS, GLOW_PAD, KILLFEED, MAPS, PORTRAITS, RANKS, bloom, decode, slug};

    /// The slug has to survive the one agent with punctuation in its name.
    #[test]
    fn a_name_becomes_the_key_it_is_filed_under() {
        assert_eq!(slug("KAY/O"), "kayo");
        assert_eq!(slug("Clove"), "clove");
        assert_eq!(slug("The Range"), "therange");
    }

    /// Every file bound into the binary has to decode, because the decoder
    /// refuses anything it does not recognise and the failure would be a
    /// silently missing picture.
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

    /// The glow is brightest where the emblem is, fades away from it, and
    /// has nothing left at the edge of its own picture, where a hard cut
    /// would show.
    #[test]
    fn the_glow_falls_off_like_light() {
        let (_tier, bytes) = RANKS
            .iter()
            .find(|(tier, _)| *tier == 12)
            .expect("gold has an emblem");
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

    /// Every agent with a portrait has a killfeed crop and the other way
    /// round, or the board and the panel would disagree about who has a face.
    #[test]
    fn every_agent_has_both_pictures() {
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
