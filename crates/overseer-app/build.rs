//! Links `assets/overseer.ico` into the executable.
//!
//! Explorer, the Start menu and a taskbar pin all show it. The MSVC linker
//! takes a compiled `.res` file like any object, so this writes one by hand
//! and needs no resource compiler or Windows SDK. `overseer-setup` builds
//! with this script too.

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;

/// Every resource file opens with this empty entry, which marks it 32 bit.
const EMPTY: [u8; 32] = [
    0, 0, 0, 0, 32, 0, 0, 0, 0xFF, 0xFF, 0, 0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0,
];

fn main() -> io::Result<()> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(io::Error::other)?);
    let ico = root.join("../../assets/overseer.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return Ok(());
    }
    let res = resources(&fs::read(&ico)?).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "assets/overseer.ico is not an icon",
        )
    })?;
    let out = PathBuf::from(env::var("OUT_DIR").map_err(io::Error::other)?).join("icon.res");
    fs::write(&out, res)?;
    println!("cargo:rustc-link-arg-bins={}", out.display());
    Ok(())
}

/// An `.ico` as resources: each picture copied as is, and a group whose
/// entries point at them by id instead of by file offset.
fn resources(ico: &[u8]) -> Option<Vec<u8>> {
    let count = u16_at(ico, 4)?;
    let mut res = EMPTY.to_vec();
    let mut group = vec![0, 0, 1, 0];
    group.extend_from_slice(&count.to_le_bytes());
    for id in 1..=count {
        let at = 6 + 16 * usize::from(id - 1);
        let dir = ico.get(at..at + 16)?;
        let (size, offset) = (u32_at(dir, 8)?, u32_at(dir, 12)?);
        res.extend(entry(RT_ICON, id, ico.get(offset..offset + size)?));
        group.extend_from_slice(dir.get(..4)?);
        // One plane: Pillow writes 0, and Windows multiplies by it.
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(dir.get(6..12)?);
        group.extend_from_slice(&id.to_le_bytes());
    }
    res.extend(entry(RT_GROUP_ICON, 1, &group));
    Some(res)
}

fn entry(kind: u16, id: u16, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + data.len() + 3);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for ordinal in [kind, id] {
        out.extend_from_slice(&[0xFF, 0xFF]);
        out.extend_from_slice(&ordinal.to_le_bytes());
    }
    out.extend_from_slice(&[0; 4]); // data version
    out.extend_from_slice(&0x1010u16.to_le_bytes()); // moveable, discardable
    out.extend_from_slice(&0x0409u16.to_le_bytes()); // English (US), as rc writes
    out.extend_from_slice(&[0; 8]); // version, characteristics
    out.extend_from_slice(data);
    out.resize(out.len().next_multiple_of(4), 0);
    out
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)).ok()
}
