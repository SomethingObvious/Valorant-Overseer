//! What this PC has, for deciding whether to go easy on it.

use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// The least memory of its own a graphics card has here, in bytes. Below it
/// is a chip in the processor, which VALORANT shares with this app, or a card
/// too weak to tell apart from one.
const CARD_LEAST: usize = 2 << 30;

/// The memory in the PC, in bytes, or nothing when Windows won't say.
#[must_use]
pub fn memory() -> Option<u64> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or_default(),
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: `status` is a local of the size its first field says.
    let read = unsafe { GlobalMemoryStatusEx(&raw mut status) };
    read.is_ok().then_some(status.ullTotalPhys)
}

/// Whether the PC has a graphics card of its own, with 2 GB of memory or
/// more. Only reads what each adapter says, so no driver starts.
#[must_use]
pub fn graphics_card() -> bool {
    // SAFETY: documented DXGI calls, each adapter's description read into a
    // value the call fills.
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return false;
        };
        (0..)
            .map_while(|at| factory.EnumAdapters1(at).ok())
            .filter_map(|adapter| adapter.GetDesc1().ok())
            .any(|desc| {
                desc.Flags & u32::try_from(DXGI_ADAPTER_FLAG_SOFTWARE.0).unwrap_or_default() == 0
                    && desc.DedicatedVideoMemory >= CARD_LEAST
            })
    }
}
