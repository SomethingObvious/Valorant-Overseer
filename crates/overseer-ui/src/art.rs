//! Riot's portraits, killfeed crops, cards, map strips and rank emblems, bound
//! into the binary and decoded on first use.
//!
//! An agent or a map newer than the build is drawn from Riot's art the
//! backend fetches, cut to the same sizes and frames the bound art was.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use egui::{Context, TextureHandle, TextureOptions};

use crate::art_assets::{CARDS, KILLFEED, MAPS, PORTRAITS, RANKS};

/// The square portrait for an agent, or `None` for one newer than this build,
/// which callers draw as a plate with its initial.
#[must_use]
pub fn agent(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "agent", &PORTRAITS, name).or_else(|| {
        fetched(ctx, Kind::Agent, "agent", name, |dir, key| {
            raw(dir, "agents", key).map(|full| portrait(&full))
        })
    })
}

/// The killfeed crop for an agent, two wide by one tall and tight on the face.
#[must_use]
pub fn killfeed(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "killfeed", &KILLFEED, name).or_else(|| {
        fetched(ctx, Kind::Agent, "killfeed", name, |dir, key| {
            killfeed_from(raw(dir, "killfeed", key), raw(dir, "agents", key))
        })
    })
}

/// An agent's card for the top of the panel, eyes to chin at 2.4 to 1, since
/// the 256 pixel killfeed crop goes soft stretched that wide.
#[must_use]
pub fn card(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "card", &CARDS, name).or_else(|| {
        fetched(ctx, Kind::Agent, "card", name, |dir, key| {
            raw(dir, "agents", key).map(|full| card_from(&full))
        })
    })
}

/// A map's band of art, for behind the header.
#[must_use]
pub fn map(ctx: &Context, name: &str) -> Option<TextureHandle> {
    named(ctx, "map", &MAPS, name).or_else(|| {
        fetched(ctx, Kind::Map, "map", name, |dir, key| {
            raw(dir, "maps", key)
        })
    })
}

/// What the backend is asked for art of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// An agent: their portrait and killfeed art.
    Agent,
    /// A map: its band of art.
    Map,
}

impl Kind {
    /// The name the backend knows it by.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Map => "map",
        }
    }
}

/// Where fetched art is kept, set once by the app.
static FETCHED: OnceLock<PathBuf> = OnceLock::new();

/// An agent or map, by kind and key, whose art was looked for.
type Asked = (Kind, String);

/// Agents and maps art was looked for and not found, not yet handed to
/// the app, and every one looked for already, so each is asked for once.
static WANTED: Mutex<(Vec<Asked>, BTreeSet<Asked>)> = Mutex::new((Vec::new(), BTreeSet::new()));

/// How long before a picture not found yet is looked for again, in
/// seconds, since the backend may be fetching it.
const LOOK_AGAIN: f64 = 3.0;

/// Says where fetched art is kept, once, before anything is drawn.
pub fn set_fetched_dir(dir: PathBuf) {
    let _first = FETCHED.set(dir);
}

/// The agents and maps newer than this build whose art was looked for,
/// each handed over once, for the app to ask the backend for.
#[must_use]
pub fn wanted() -> Vec<Asked> {
    WANTED
        .lock()
        .map(|mut wanted| std::mem::take(&mut wanted.0))
        .unwrap_or_default()
}

/// Art for an agent or a map newer than this build, cut by `make` from
/// what the backend fetched, or else asked for. `use_` tells apart the
/// pictures cut from one fetched file.
fn fetched(
    ctx: &Context,
    kind: Kind,
    use_: &str,
    name: &str,
    make: impl FnOnce(&Path, &str) -> Option<egui::ColorImage>,
) -> Option<TextureHandle> {
    let key = slug(name);
    let dir = FETCHED.get().filter(|_| !key.is_empty())?;
    let looked = egui::Id::new(("art-looked", use_, &key));
    let now = ctx.input(|i| i.time);
    let due = ctx
        .data(|store| store.get_temp::<f64>(looked))
        .is_none_or(|at| now - at >= LOOK_AGAIN);
    let found = cached(ctx, &format!("{use_}-fetched:{key}"), || {
        due.then(|| make(dir, &key)).flatten()
    });
    if found.is_none() && due {
        ctx.data_mut(|store| store.insert_temp(looked, now));
        if let Ok(mut wanted) = WANTED.lock()
            && wanted.1.insert((kind, key.clone()))
        {
            wanted.0.push((kind, key));
        }
    }
    found
}

/// One fetched picture, by what it is and whose.
fn raw(dir: &Path, kind: &str, key: &str) -> Option<egui::ColorImage> {
    std::fs::read(dir.join(kind).join(format!("{key}.png")))
        .ok()
        .and_then(|bytes| decode(&bytes))
}

/// The bound portraits' 64 pixel square from Riot's 1024 pixel portrait:
/// trimmed to what shows, shrunk to fit and centred.
fn portrait(full: &egui::ColorImage) -> egui::ColorImage {
    const SIDE: usize = 64;
    let shown = trimmed(full);
    let [wide, high] = shown.size;
    let size = thumbnail_size([wide, high], SIDE);
    let small = resized(&shown, size);
    let mut tile = egui::ColorImage::filled([SIDE, SIDE], egui::Color32::TRANSPARENT);
    let at = [
        (SIDE - size[0]).div_euclid(2),
        (SIDE - size[1]).div_euclid(2),
    ];
    for y in 0..size[1] {
        for x in 0..size[0] {
            tile[(at[0] + x, at[1] + y)] = small[(x, y)];
        }
    }
    tile
}

/// How big Pillow's thumbnail makes a picture `size` big to fit a `side`
/// square: the long side to `side`, and the short side whichever whole number
/// next to it keeps the shape closest, the smaller one on a tie.
fn thumbnail_size([wide, high]: [usize; 2], side: usize) -> [usize; 2] {
    if wide <= side && high <= side {
        return [wide.max(1), high.max(1)];
    }
    let aspect = wide as f64 / high.max(1) as f64;
    let closest = |exact: f64, off: &dyn Fn(f64) -> f64| {
        let (low, up) = (exact.floor(), exact.ceil());
        let pick = if off(up) < off(low) { up } else { low };
        (pick as usize).max(1)
    };
    let edge = side as f64;
    if aspect <= 1.0 {
        [closest(edge * aspect, &|n| (aspect - n / edge).abs()), side]
    } else {
        [side, closest(edge / aspect, &|n| (aspect - edge / n).abs())]
    }
}

/// The bound cards' frame from Riot's portrait: eyes to chin at 2.4 to 1,
/// from 250 down on the 1024 pixel portrait, 512 wide.
fn card_from(full: &egui::ColorImage) -> egui::ColorImage {
    let square = resized(full, [1024, 1024]);
    resized(&cropped(&square, [0, 250], [1024, 427]), [512, 213])
}

/// Riot's killfeed art, or, where Riot's is too small, the same 2 to 1
/// frame cut from the portrait, the way two of the bound ones were.
fn killfeed_from(
    feed: Option<egui::ColorImage>,
    full: Option<egui::ColorImage>,
) -> Option<egui::ColorImage> {
    match (feed, full) {
        (Some(feed), _) if feed.size[0] >= 256 => Some(feed),
        (feed, Some(full)) => {
            let square = resized(&full, [1024, 1024]);
            Some(resized(
                &cropped(&square, [0, 220], [1024, 512]),
                [256, 128],
            ))
            .or(feed)
        }
        (feed, None) => feed,
    }
}

/// `image` cut to the smallest box holding everything not clear.
fn trimmed(image: &egui::ColorImage) -> egui::ColorImage {
    let [wide, high] = image.size;
    let shows = |x: usize, y: usize| image[(x, y)].a() > 0;
    let rows: Vec<usize> = (0..high)
        .filter(|&y| (0..wide).any(|x| shows(x, y)))
        .collect();
    let columns: Vec<usize> = (0..wide)
        .filter(|&x| (0..high).any(|y| shows(x, y)))
        .collect();
    match (rows.first(), rows.last(), columns.first(), columns.last()) {
        (Some(&top), Some(&bottom), Some(&left), Some(&right)) => {
            cropped(image, [left, top], [right + 1 - left, bottom + 1 - top])
        }
        _ => image.clone(),
    }
}

/// The part of `image` `size` big from `at`, kept inside it.
fn cropped(image: &egui::ColorImage, at: [usize; 2], size: [usize; 2]) -> egui::ColorImage {
    let at = [at[0].min(image.size[0]), at[1].min(image.size[1])];
    let size = [
        size[0].min(image.size[0] - at[0]),
        size[1].min(image.size[1] - at[1]),
    ];
    let mut out = egui::ColorImage::filled(size, egui::Color32::TRANSPARENT);
    for y in 0..size[1] {
        for x in 0..size[0] {
            out[(x, y)] = image[(at[0] + x, at[1] + y)];
        }
    }
    out
}

/// `image` resized with Lanczos, as the bound art was, on its
/// premultiplied pixels so no edge picks up a fringe.
fn resized(picture: &egui::ColorImage, size: [usize; 2]) -> egui::ColorImage {
    let bytes: Vec<u8> = picture
        .pixels
        .iter()
        .flat_map(egui::Color32::to_array)
        .collect();
    let fit = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).max(1);
    let Some(source) =
        image::RgbaImage::from_raw(fit(picture.size[0]), fit(picture.size[1]), bytes)
    else {
        return picture.clone();
    };
    let out = image::imageops::resize(
        &source,
        fit(size[0]),
        fit(size[1]),
        image::imageops::FilterType::Lanczos3,
    );
    egui::ColorImage::from_rgba_premultiplied(size, out.as_raw())
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

/// The Spike as Riot draws it on the Standard mode's icon, for a lineup that
/// is only the Spike, planted.
const SPIKE: &[u8] = include_bytes!("../assets/spike.png");

/// The Spike's icon, white on clear, for the caller to tint. It is drawn far
/// smaller than it is, so it keeps mipmaps to stay sharp.
#[must_use]
pub fn spike(ctx: &Context) -> Option<TextureHandle> {
    let options = TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear));
    cached_with(ctx, "spike", options, || decode(SPIKE))
}

/// Where the middle of the Spike's core is in its icon, as a share of its
/// width and height, measured off the hexagon's edges.
pub const SPIKE_CORE: [f32; 2] = [63.9 / 128.0, 49.85 / 128.0];

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
pub fn save_png(path: &Path, image: &egui::ColorImage) -> std::io::Result<()> {
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
    cached_with(ctx, key, TextureOptions::LINEAR, make)
}

/// [`cached`], with the texture's own `options`.
fn cached_with(
    ctx: &Context,
    key: &str,
    options: TextureOptions,
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
    let handle = ctx.load_texture(key, make()?, options);
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
    use super::{
        CARDS, KILLFEED, Kind, MAPS, PORTRAITS, RANKS, SPIKE, agent, card_from, decode,
        killfeed_from, portrait, save_png, set_fetched_dir, wanted,
    };

    /// A stand-in for Riot's 1024 pixel portrait: clear, with an opaque
    /// figure 400 wide and 800 tall off to one side.
    fn a_portrait() -> egui::ColorImage {
        let mut full = egui::ColorImage::filled([1024, 1024], egui::Color32::TRANSPARENT);
        for y in 100..900 {
            for x in 300..700 {
                full[(x, y)] = egui::Color32::from_rgb(200, 120, 40);
            }
        }
        full
    }

    /// An agent the build doesn't carry is asked for once, and drawn once
    /// what the backend fetched is there.
    #[test]
    fn a_new_agent_is_asked_for_once_and_drawn_once_fetched() {
        let dir = std::env::temp_dir().join(format!("overseer-art-{}", std::process::id()));
        set_fetched_dir(dir.clone());
        let ctx = egui::Context::default();
        assert!(agent(&ctx, "New/Comer").is_none());
        assert_eq!(wanted(), vec![(Kind::Agent, "newcomer".to_owned())]);
        assert!(agent(&ctx, "New/Comer").is_none());
        assert!(wanted().is_empty(), "asked for once");

        std::fs::create_dir_all(dir.join("agents")).unwrap();
        save_png(&dir.join("agents").join("newcomer.png"), &a_portrait()).unwrap();
        let later = egui::RawInput {
            time: Some(10.0),
            ..egui::RawInput::default()
        };
        let mut frame = ctx.run_ui(later, |_| {});
        frame.textures_delta.clear();
        let drawn = agent(&ctx, "New/Comer").map(|t| t.size());
        drop(std::fs::remove_dir_all(&dir));
        assert_eq!(drawn, Some([64, 64]));
    }

    /// The known answer for the cutting: from Riot's own portraits, agents
    /// the build carries come out like their bound art. Run by hand with
    /// `OVERSEER_RAW_ART` naming a folder of valorant-api's displayIcon
    /// files, one per agent, named the way the bound art is. What it cuts is
    /// written beside them.
    #[test]
    #[ignore = "needs OVERSEER_RAW_ART"]
    fn fetched_art_is_cut_like_the_bound_art() {
        let dir = std::path::PathBuf::from(std::env::var("OVERSEER_RAW_ART").unwrap());
        let apart = |a: &egui::ColorImage, b: &egui::ColorImage| {
            assert_eq!(a.size, b.size);
            let sum: u64 = a
                .pixels
                .iter()
                .zip(&b.pixels)
                .flat_map(|(x, y)| x.to_array().into_iter().zip(y.to_array()))
                .map(|(x, y)| u64::from(x.abs_diff(y)))
                .sum();
            sum as f64 / (a.pixels.len() * 4) as f64
        };
        let mut checked = 0;
        for (name, bytes) in PORTRAITS {
            let Ok(raw) = std::fs::read(dir.join(format!("{name}.png"))) else {
                continue;
            };
            let full = decode(&raw).unwrap();
            // Kept beside the raw file, to look at.
            let (cut, framed) = (portrait(&full), card_from(&full));
            save_png(&dir.join(format!("{name}-portrait.png")), &cut).unwrap();
            save_png(&dir.join(format!("{name}-card.png")), &framed).unwrap();
            let square = apart(&cut, &decode(bytes).unwrap());
            let card = CARDS.iter().find(|(n, _)| *n == name).unwrap().1;
            let wide = apart(&framed, &decode(card).unwrap());
            println!("{name}: portrait {square:.2}, card {wide:.2} apart on average");
            assert!(square < 1.0 && wide < 3.0, "{name}");
            // Riot's killfeed art for these two is too small, so the bound
            // crops were cut from the portrait, as a new agent's would be.
            if ["breach", "jett"].contains(&name) {
                let bound = KILLFEED.iter().find(|(n, _)| *n == name).unwrap().1;
                let cut = killfeed_from(None, Some(full.clone())).unwrap();
                let feed = apart(&cut, &decode(bound).unwrap());
                println!("{name}: killfeed {feed:.2} apart on average");
                assert!(feed < 3.0, "{name} killfeed");
            }
            checked += 1;
        }
        assert!(checked > 0, "no raw portraits in the folder");
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
        assert!(decode(SPIKE).is_some(), "the Spike did not decode");
    }
}
