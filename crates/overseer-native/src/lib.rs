//! What Overseer needs from Windows itself: lineup clips played by Windows'
//! own media engine in a child of the app's window, the overlay kept from
//! taking the focus, whether the window can be seen, and what the PC has.
//! Decoding with ffmpeg and drawing each frame with egui took most of a
//! core and redrew the whole window for every frame.
//!
//! This is the one crate in the workspace that calls C APIs, which is why it
//! alone allows `unsafe`. Everything here runs on the thread that owns the
//! app's window, apart from [`Video`]'s events, which only set flags.

mod machine;
mod video;
mod window;

pub use machine::{graphics_card, memory};
pub use video::{Area, Video, set_window};
pub use window::{hidden, never_activate};
