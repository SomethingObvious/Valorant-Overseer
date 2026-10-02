//! Which GPU the window asks for, and a report of what it found, so the gate
//! checks the claims in `crates/README.md` instead of trusting them.

use std::path::Path;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, wgpu};

/// The renderer configuration, asking for the integrated GPU. VALORANT wants
/// the discrete one, and flat shapes and text don't need it. `LowPower` is a
/// request, so a machine with one GPU just gets that one.
#[must_use]
pub(crate) fn low_power_wgpu() -> WgpuConfiguration {
    let mut config = WgpuConfiguration::default();
    if let WgpuSetup::CreateNew(create) = &mut config.wgpu_setup {
        create.power_preference = wgpu::PowerPreference::LowPower;
    }
    config
}

/// One line describing the adapter the app ended up on, since that decides
/// which quality tier it can hold.
#[must_use]
pub(crate) fn adapter_line(info: &wgpu::AdapterInfo) -> String {
    format!(
        "adapter {} [{:?} via {:?}] driver {} {}",
        info.name, info.device_type, info.backend, info.driver, info.driver_info
    )
}

/// Prints where the window would find its backend, without opening a window,
/// because the gate may have no desktop session to open one in. The adapter is
/// only known once the app draws its first frame, so the app reports that.
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
