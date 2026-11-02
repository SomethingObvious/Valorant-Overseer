//! What this PC has, for deciding whether to go easy on it.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    BELOW_NORMAL_PRIORITY_CLASS, GetCurrentProcess, GetProcessAffinityMask, NORMAL_PRIORITY_CLASS,
    SetPriorityClass, SetProcessAffinityMask,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::w;

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

/// Whether Windows plays animations. Its "Animation effects" setting, under
/// Accessibility, turns them off, and so does "Adjust for best performance".
/// On when Windows won't say.
#[must_use]
pub fn animations() -> bool {
    let mut on: i32 = 1;
    // SAFETY: this action writes one BOOL, four bytes, to the pointer it gets,
    // and `on` is a local of that size.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_err() || on != 0
}

/// How much bigger Windows' "Text size" setting, under Accessibility, makes
/// text: 1 at the default, up to 2.25. Windows keeps it as a percentage in
/// the registry and has no call that returns it.
#[must_use]
pub fn text_scale() -> f32 {
    let mut percent: u32 = 100;
    let mut size = u32::try_from(size_of::<u32>()).unwrap_or_default();
    // SAFETY: asked for a DWORD only, so at most `size` bytes, four, are
    // written to `percent`, a local of that size.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("SOFTWARE\\Microsoft\\Accessibility"),
            w!("TextScaleFactor"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut percent).cast()),
            Some(&raw mut size),
        )
    };
    if read.is_err() {
        return 1.0;
    }
    f32::from(u16::try_from(percent.clamp(100, 225)).unwrap_or(100)) / 100.0
}

/// Whether the PC has a graphics card of its own, with 2 GB of memory or
/// more. Only reads what each adapter says, so no driver starts.
#[must_use]
pub fn graphics_card() -> bool {
    gpus().iter().any(|&memory| memory >= CARD_LEAST)
}

/// Whether the PC has any GPU at all, as opposed to only Windows' software
/// renderer.
#[must_use]
pub fn any_gpu() -> bool {
    !gpus().is_empty()
}

/// The dedicated memory of each adapter that isn't drawn in software.
fn gpus() -> Vec<usize> {
    let software = u32::try_from(DXGI_ADAPTER_FLAG_SOFTWARE.0).unwrap_or_default();
    // SAFETY: documented DXGI calls, each adapter's description read into a
    // value the call fills.
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return Vec::new();
        };
        (0..)
            .map_while(|at| factory.EnumAdapters1(at).ok())
            .filter_map(|adapter| adapter.GetDesc1().ok())
            .filter(|desc| desc.Flags & software == 0)
            .map(|desc| desc.DedicatedVideoMemory)
            .collect()
    }
}

/// Drops this process to the backend's priority, below normal.
///
/// For a window drawn in software, which keeps the processor busy, so the
/// backend it started below normal still gets a turn.
pub fn step_back() {
    // SAFETY: the pseudo handle for this process, which needs no closing.
    let _set = unsafe { SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS) };
}

/// Keeps this process to the last core it may use, before Windows' software
/// renderer starts.
///
/// The renderer starts a thread for every core it can see, and they spend
/// more time waiting on each other than drawing. Measured here, a frame took
/// 0.5 core-seconds on one core and 8.7 across sixteen. The backend is
/// already running by then and keeps every core, though clip programs
/// started later share the one. Anything else goes through [`launch`].
pub fn one_core() {
    let (mut mine, mut all) = (0_usize, 0_usize);
    // SAFETY: the pseudo handle for this process, which needs no closing,
    // and two plain values for Windows to fill in.
    let read = unsafe { GetProcessAffinityMask(GetCurrentProcess(), &raw mut mine, &raw mut all) };
    if read.is_err() || mine == 0 {
        return;
    }
    let last = 1_usize << (usize::BITS - 1 - mine.leading_zeros());
    // SAFETY: as above, with a mask inside the one Windows gave.
    let _set = unsafe { SetProcessAffinityMask(GetCurrentProcess(), last) };
}

/// Starts a program with every core and normal priority.
///
/// It would otherwise inherit the one core and the low priority of a window
/// drawn in software, and so would whatever it starts in turn, like a browser.
///
/// # Errors
///
/// When the program doesn't start.
pub fn launch(command: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    use std::os::windows::io::AsRawHandle;
    let child = command.spawn()?;
    let handle = HANDLE(child.as_raw_handle());
    let (mut mine, mut all) = (0_usize, 0_usize);
    // SAFETY: the pseudo handle for this process, two plain values for
    // Windows to fill in, and the child's handle, which `child` keeps open.
    unsafe {
        if GetProcessAffinityMask(GetCurrentProcess(), &raw mut mine, &raw mut all).is_ok()
            && all != 0
        {
            let _set = SetProcessAffinityMask(handle, all);
        }
        let _set = SetPriorityClass(handle, NORMAL_PRIORITY_CLASS);
    }
    Ok(child)
}

#[cfg(test)]
mod tests {
    /// Windows' text size reads as a factor inside the limits its own
    /// setting allows, and the animation setting reads at all.
    #[test]
    fn windows_settings_read_within_their_limits() {
        let scale = super::text_scale();
        assert!((1.0..=2.25).contains(&scale), "{scale}");
        let _on = super::animations();
    }
}
