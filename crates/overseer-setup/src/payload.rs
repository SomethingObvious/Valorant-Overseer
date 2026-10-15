//! The app carried inside the setup exe. A release build appends the release
//! zip to `overseer-setup.exe`, then the zip's length and [`MAGIC`], so one
//! file is the whole download. Windows' own tar unpacks it, which keeps a zip
//! reader out of this crate.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The last eight bytes of a setup that carries the app. `build-release.ps1`
/// writes the same eight.
const MAGIC: &[u8; 8] = b"OVSETUP1";

/// The zip's length and the magic.
const TAIL: u64 = 16;

/// Windows' `CREATE_NO_WINDOW`.
const NO_WINDOW: u32 = 0x0800_0000;

/// Where the zip sits inside `exe`, as its offset and length, when it carries
/// one.
pub(crate) fn carried(exe: &Path) -> Option<(u64, u64)> {
    let mut file = File::open(exe).ok()?;
    let at = file.metadata().ok()?.len().checked_sub(TAIL)?;
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut tail = [0_u8; 16];
    file.read_exact(&mut tail).ok()?;
    let (len, magic) = tail.split_first_chunk::<8>()?;
    if magic != MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(*len);
    Some((at.checked_sub(len)?, len))
}

/// Where a carried app goes: the same folder every time, so there is nothing
/// to choose and a second run upgrades the first. `OVERSEER_ROOT` moves it,
/// for a test.
pub(crate) fn home() -> Option<PathBuf> {
    if let Some(from_env) = std::env::var_os("OVERSEER_ROOT") {
        return Some(PathBuf::from(from_env));
    }
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("Programs")
            .join("Valorant Overseer"),
    )
}

/// Unpacks the app carried in `exe` into `root`, over whatever an older
/// version left there. Settings, notes and match data aren't in the zip, so
/// they stay.
pub(crate) fn unpack(exe: &Path, root: &Path) -> Result<(), String> {
    let (at, len) = carried(exe).ok_or("This setup doesn't have the app inside it.")?;
    if root.join(".git").exists() {
        return Err(format!(
            "{} is a copy of the source, and unpacking a release would overwrite it.",
            root.display()
        ));
    }
    let zip = std::env::temp_dir().join(format!("overseer-setup-{}.zip", std::process::id()));
    let done = copy_out(exe, at, len, &zip).and_then(|()| {
        close_window(root);
        extract(&zip, root)
    });
    drop(std::fs::remove_file(&zip));
    done
}

/// Copies the carried zip out to a file of its own for tar to read.
fn copy_out(exe: &Path, at: u64, len: u64, zip: &Path) -> Result<(), String> {
    let unreadable = |e: std::io::Error| format!("Couldn't read this setup: {e}");
    let mut file = File::open(exe).map_err(unreadable)?;
    file.seek(SeekFrom::Start(at)).map_err(unreadable)?;
    let mut out =
        File::create(zip).map_err(|e| format!("Couldn't write {}: {e}", zip.display()))?;
    let copied = std::io::copy(&mut file.take(len), &mut out).map_err(unreadable)?;
    if copied == len {
        Ok(())
    } else {
        Err("This setup is cut short. Download it again.".to_owned())
    }
}

/// Closes a window running from `root`, since Windows won't replace an exe
/// that is running. The installer closes the backend itself later on.
fn close_window(root: &Path) {
    let exe = root.join("overseer.exe");
    if !exe.is_file() {
        return;
    }
    let script = "Get-Process overseer -ErrorAction SilentlyContinue | \
        Where-Object { $_.Path -eq $env:OVERSEER_EXE } | \
        ForEach-Object { Stop-Process -InputObject $_ -Force; $null = $_.WaitForExit(10000) }";
    drop(
        Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", script])
            .env("OVERSEER_EXE", exe)
            .creation_flags(NO_WINDOW)
            .status(),
    );
}

/// Unpacks `zip` into `root` with the tar that ships with Windows 10 and 11.
fn extract(zip: &Path, root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| format!("Couldn't make {}: {e}", root.display()))?;
    // By its full path, so a tar.exe earlier on PATH can't stand in for it.
    let tar = std::env::var_os("SystemRoot").map_or_else(
        || PathBuf::from("tar.exe"),
        |windows| PathBuf::from(windows).join("System32").join("tar.exe"),
    );
    let out = Command::new(&tar)
        .arg("-xf")
        .arg(zip)
        .arg("-C")
        .arg(root)
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| format!("Couldn't start {}: {e}", tar.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Couldn't unpack the app into {}: {}",
            root.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use super::{MAGIC, carried, unpack};

    /// A stand-in setup: some bytes for the exe, a real zip made by tar, its
    /// length and the magic.
    fn setup_with(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("backend")).unwrap();
        std::fs::write(src.join("backend").join(name), text).unwrap();
        let zip = dir.join("app.zip");
        let made = Command::new("tar.exe")
            .arg("-a")
            .arg("-cf")
            .arg(&zip)
            .arg("-C")
            .arg(&src)
            .arg("backend")
            .status()
            .unwrap();
        assert!(made.success(), "tar couldn't make the test zip");
        let zip = std::fs::read(&zip).unwrap();
        let mut exe = b"MZ not really an exe".to_vec();
        exe.extend_from_slice(&zip);
        exe.extend_from_slice(&(zip.len() as u64).to_le_bytes());
        exe.extend_from_slice(MAGIC);
        let path = dir.join("setup.exe");
        std::fs::write(&path, exe).unwrap();
        path
    }

    #[test]
    fn a_carried_app_unpacks_into_its_folder() {
        let dir = std::env::temp_dir().join("overseer-setup-payload");
        drop(std::fs::remove_dir_all(&dir));
        let exe = setup_with(&dir, "run.py", "print('hi')\n");
        let root = dir.join("Valorant Overseer");
        std::fs::create_dir_all(root.join("backend")).unwrap();
        std::fs::write(root.join("backend").join(".env"), "RIOT_REGION=eu\n").unwrap();

        assert_eq!(carried(&exe).map(|(at, _)| at), Some(20));
        unpack(&exe, &root).unwrap();
        let unpacked = std::fs::read_to_string(root.join("backend").join("run.py")).unwrap();
        let kept = std::fs::read_to_string(root.join("backend").join(".env")).unwrap();
        drop(std::fs::remove_dir_all(&dir));
        assert_eq!(unpacked, "print('hi')\n");
        assert_eq!(kept, "RIOT_REGION=eu\n", "an upgrade kept the settings");
    }

    #[test]
    fn a_checkout_is_never_unpacked_over() {
        let dir = std::env::temp_dir().join("overseer-setup-checkout");
        drop(std::fs::remove_dir_all(&dir));
        let exe = setup_with(&dir, "run.py", "stripped\n");
        let root = dir.join("checkout");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("backend")).unwrap();
        std::fs::write(root.join("backend").join("run.py"), "# the source\n").unwrap();

        let refused = unpack(&exe, &root);
        let source = std::fs::read_to_string(root.join("backend").join("run.py")).unwrap();
        drop(std::fs::remove_dir_all(&dir));
        assert!(refused.is_err());
        assert_eq!(source, "# the source\n");
    }

    /// The plain setup in the install folder has nothing after it, and
    /// neither does a file too short to hold the tail.
    #[test]
    fn a_plain_exe_carries_nothing() {
        let dir = std::env::temp_dir().join("overseer-setup-plain");
        std::fs::create_dir_all(&dir).unwrap();
        let plain = dir.join("plain.exe");
        std::fs::write(&plain, b"MZ and then just code, nothing appended").unwrap();
        let short = dir.join("short.exe");
        std::fs::write(&short, b"MZ").unwrap();
        let lying = dir.join("lying.exe");
        let mut bytes = b"MZ".to_vec();
        bytes.extend_from_slice(&u64::MAX.to_le_bytes());
        bytes.extend_from_slice(MAGIC);
        std::fs::write(&lying, bytes).unwrap();

        let found = [carried(&plain), carried(&short), carried(&lying)];
        drop(std::fs::remove_dir_all(&dir));
        assert_eq!(found, [None, None, None]);
    }
}
