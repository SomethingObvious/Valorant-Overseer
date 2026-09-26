//! The pictures: Riot's agent portraits and rank emblems, decoded once and
//! kept as textures for as long as the window is open.
//!
//! Every scoreboard anybody actually uses shows the agent's face and the
//! rank's emblem, and this one drew a letter in a box and three chevrons. A
//! letter is a thing to read; a face is a thing you already know, and after
//! two matches you read a lobby's composition off the left edge of the board
//! without reading a word of it. That is the whole reason to spend three
//! hundred kilobytes on it.
//!
//! Bound into the binary rather than fetched, because this app does not go
//! online and is not going to start. Decoded on first use rather than at
//! startup, because a lobby is ten agents out of twenty nine and the other
//! nineteen are work nobody asked for.

use egui::{Context, TextureHandle, TextureOptions};

use crate::art_assets::{AGENTS, RANKS};

/// The portrait for an agent the backend named, if it is one this build
/// knows.
///
/// Returns nothing for an agent added to the game after this build shipped,
/// and every call site falls back to the coloured tile it used to draw. A
/// missing picture is a plate with a letter on it, not a hole.
#[must_use]
pub fn agent(ctx: &Context, name: &str) -> Option<TextureHandle> {
    let key = slug(name);
    let bytes = AGENTS
        .iter()
        .find(|(known, _)| *known == key)
        .map(|(_, bytes)| *bytes)?;
    texture(ctx, &format!("agent:{key}"), bytes)
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
    let handle = ctx.load_texture(key, decode(bytes)?, TextureOptions::LINEAR);
    ctx.data_mut(|store| store.insert_temp(id, handle.clone()));
    Some(handle)
}

/// One RGBA8 PNG, which is what every file in `assets` is.
///
/// Anything else returns nothing rather than panicking. These are files this
/// repository wrote and checked, so a failure here means somebody replaced
/// one by hand, and the right answer to that is the coloured tile rather
/// than a window that will not open.
fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let size = [info.width as usize, info.height as usize];
    buffer.truncate(info.buffer_size());
    Some(egui::ColorImage::from_rgba_unmultiplied(size, &buffer))
}

#[cfg(test)]
mod tests {
    use super::{AGENTS, RANKS, decode, slug};

    /// The slug has to survive the one agent with punctuation in its name.
    #[test]
    fn a_name_becomes_the_key_it_is_filed_under() {
        assert_eq!(slug("KAY/O"), "kayo");
        assert_eq!(slug("Clove"), "clove");
        assert_eq!(slug("Omen"), "omen");
    }

    /// Every file bound into the binary has to be the shape the decoder
    /// expects, because the decoder refuses anything else and the failure
    /// would be a silently missing picture.
    #[test]
    fn every_bound_picture_decodes() {
        for (name, bytes) in AGENTS {
            assert!(decode(bytes).is_some(), "{name} did not decode");
        }
        for (tier, bytes) in RANKS {
            assert!(decode(bytes).is_some(), "tier {tier} did not decode");
        }
    }

    /// The table is keyed by the slug, so a key that is not already one
    /// could never be found.
    #[test]
    fn every_key_is_already_a_slug() {
        for (name, _) in AGENTS {
            assert_eq!(slug(name), name);
        }
    }
}
