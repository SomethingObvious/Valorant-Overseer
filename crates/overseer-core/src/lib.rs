//! The data layer behind both front ends.
//!
//! Today this is a client of the Python bridge in `backend/ws_server.py`:
//! [`bridge`] holds the socket, and [`board`] is the shape of what comes back.
//! The plan in `crates/README.md` replaces that with native Riot reads one
//! endpoint at a time, which is why the window talks to this crate rather than
//! to a socket. When an endpoint moves, nothing above this line changes.
//!
//! Nothing here contacts Riot. Python owns every request, including the rate
//! limiting, and this crate only reads what the bridge is willing to say.

pub mod board;
pub mod bridge;

pub use board::{Board, Player};
pub use bridge::{Bridge, Event, Status};
