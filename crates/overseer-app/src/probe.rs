//! Which GPU the window asks for, and a report of what it found, so the gate
//! checks the claims in `crates/README.md` instead of trusting them.

use std::path::Path;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, wgpu};

/// How many textures DX12 makes room for at once, which it reserves at the
/// start. Its default is a million. Each picture on screen takes one, and the
/// window never holds more than a few hundred.
const BINDINGS: u32 = 16_384;

/// How the window draws, in the order they are tried until one opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Draw {
    /// DX12 on a real GPU. Asking for every backend starts the Vulkan and
    /// OpenGL drivers too, at 60 MB and six threads, even when DX12 draws.
    Dx12,
    /// Any of wgpu's backends on a real GPU.
    Gpu,
    /// OpenGL through glow, for a GPU too old for wgpu.
    OpenGl,
    /// Windows' software renderer, when nothing else opens. Every frame costs
    /// it seconds of processor time, spread over as many cores as it can get.
    Software,
}

/// Every way to draw, best first.
pub(crate) const DRAWS: [Draw; 4] = [Draw::Dx12, Draw::Gpu, Draw::OpenGl, Draw::Software];

/// The renderer configuration, asking for the integrated GPU. VALORANT wants
/// the discrete one, and flat shapes and text don't need it. `LowPower` is a
/// request, so a machine with one GPU just gets that one.
///
/// It also asks for small memory blocks. By default the GPU allocator takes
/// 128 MB for textures and 64 MB for uploads before anything is drawn, and on
/// an integrated GPU that memory is the machine's own RAM. This window only
/// needs a few MB of either.
#[must_use]
pub(crate) fn low_power_wgpu(draw: Draw) -> WgpuConfiguration {
    let mut config = WgpuConfiguration::default();
    if let WgpuSetup::CreateNew(create) = &mut config.wgpu_setup {
        create.power_preference = wgpu::PowerPreference::LowPower;
        // Windows' software renderer is a DX12 adapter too.
        if matches!(draw, Draw::Dx12 | Draw::Software) && wgpu::Backends::from_env().is_none() {
            create.instance_descriptor.backends = wgpu::Backends::DX12;
        }
        if draw != Draw::Software {
            create.native_adapter_selector = Some(std::sync::Arc::new(gpu));
        }
        let default = std::sync::Arc::clone(&create.device_descriptor);
        create.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut descriptor = default(adapter);
            descriptor.memory_hints = wgpu::MemoryHints::MemoryUsage;
            descriptor.required_limits.max_non_sampler_bindings = BINDINGS;
            descriptor
        });
    }
    config
}

/// The integrated GPU, or else any other that can draw to the window, but
/// never Windows' software renderer, which glow on an old GPU would beat.
fn gpu(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    let rank = |kind: wgpu::DeviceType| match kind {
        wgpu::DeviceType::IntegratedGpu => Some(0),
        wgpu::DeviceType::DiscreteGpu => Some(1),
        wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => Some(2),
        wgpu::DeviceType::Cpu => None,
    };
    adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .filter_map(|a| rank(a.get_info().device_type).map(|r| (r, a)))
        .min_by_key(|(r, _)| *r)
        .map(|(_, a)| a.clone())
        .ok_or_else(|| "no GPU draws through wgpu".to_owned())
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
