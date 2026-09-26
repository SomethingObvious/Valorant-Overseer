//! What this machine actually gave us, asked for rather than assumed.
//!
//! Two jobs. It asks wgpu for the adapter the app should be on, and it prints
//! what it found, so the claims in `crates/README.md` are checked by the gate
//! instead of trusted.

use std::path::Path;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, wgpu};

/// The renderer configuration: the integrated GPU, please.
///
/// This is the largest single performance decision in the app, and it is about
/// the game rather than about us. This laptop has an integrated adapter and an
/// RTX 4070; VALORANT wants the 4070, and a scoreboard drawing flat shapes and
/// text has no business competing for it. `PowerPreference::LowPower` is a
/// request rather than a guarantee: a machine with one GPU gets that one, and
/// nothing here breaks if the request is not honoured.
#[must_use]
pub(crate) fn low_power_wgpu() -> WgpuConfiguration {
    let mut config = WgpuConfiguration::default();
    if let WgpuSetup::CreateNew(create) = &mut config.wgpu_setup {
        create.power_preference = wgpu::PowerPreference::LowPower;
    }
    config
}

/// One line describing the adapter the app ended up on.
///
/// Recorded because it decides which quality tier is honest, and because the
/// NVIDIA driver on this machine is behind, which is exactly the condition
/// under which the richer tier has to fall back cleanly.
#[must_use]
pub(crate) fn adapter_line(info: &wgpu::AdapterInfo) -> String {
    format!(
        "adapter {} [{:?} via {:?}] driver {} {}",
        info.name, info.device_type, info.backend, info.driver, info.driver_info
    )
}

/// Prints what the window would run on, and returns.
///
/// Deliberately does not open a window: this runs in the gate, where there may
/// be no session to open one in. The adapter the app settles on is reported by
/// the app itself on its first frame, which is the only place it is known.
pub(crate) fn report(root: &Path) {
    let bridge = overseer_core::bridge::credentials_path(root);
    println!("root      {}", root.display());
    println!("bridge    {}", bridge.display());
    println!(
        "backend   {}",
        if bridge.is_file() {
            "listening (bridge.json present)"
        } else {
            "not running"
        }
    );
}
