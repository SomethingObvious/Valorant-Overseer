//! What Overseer needs from Windows itself: lineup clips played by Windows'
//! own media engine in a child of the app's window, from a process of their
//! own, the overlay kept from taking the focus, whether the window can be
//! seen, and what the PC has. Decoding with ffmpeg and drawing each frame
//! with egui took most of a core and redrew the whole window for every frame.
//!
//! This is the one crate in the workspace that calls C APIs, which is why it
//! alone allows `unsafe`. Everything here runs on the thread that owns the
//! app's window, apart from [`Video`]'s events, which only set flags.

mod helper;
mod machine;
mod remote;
mod video;
mod window;

pub use helper::serve;
pub use machine::{graphics_card, memory};
pub use remote::{Video, let_go};
pub use window::{hidden, never_activate, set_window};

/// A box in the window's client area, in physical pixels: left, top, right
/// and bottom.
pub type Area = [i32; 4];
