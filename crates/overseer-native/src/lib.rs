//! What Overseer needs from Windows itself: lineup clips played by Windows'
//! own media engine in a child of the app's window, from a process of their
//! own, the overlay kept from taking the focus, the keys and mouse buttons
//! pressed while the game is in front, whether the window can be seen, and
//! what the PC has. Clips play natively because decoding with ffmpeg and
//! drawing each frame with egui takes most of a core and redraws the whole
//! window for every frame.
//!
//! This is the one crate in the workspace that calls C APIs, which is why it
//! alone allows `unsafe`. Everything here runs on the thread that owns the
//! app's window, apart from [`Video`]'s events, which only set flags, and
//! [`listen`], which reads on a thread of its own.

mod buttons;
mod helper;
mod machine;
mod pick;
mod remote;
mod video;
mod window;

pub use buttons::{Buttons, Held, Press, listen};
pub use helper::serve;
pub use machine::{
    animations, any_gpu, graphics_card, launch, memory, one_core, step_back, text_scale,
};
pub use pick::{pick_pictures, take_pictures};
pub use remote::{Video, let_go};
pub use window::{hidden, never_activate, set_window};

/// A box in the window's client area, in physical pixels: left, top, right
/// and bottom.
pub type Area = [i32; 4];
