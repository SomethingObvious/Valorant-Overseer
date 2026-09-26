//! The data layer behind both front ends.
//!
//! Today this is a client of the Python bridge in `backend/ws_server.py`:
//! [`bridge`] holds the socket, [`board`] is the shape of what it broadcasts,
//! and [`profile`] is the shape of the one thing worth asking it for.
//! The plan in `crates/README.md` replaces that with native Riot reads one
//! endpoint at a time, which is why the window talks to this crate rather than
//! to a socket. When an endpoint moves, nothing above this line changes.
//!
//! Nothing here contacts Riot. Python owns every request, including the rate
//! limiting, and this crate only reads what the bridge is willing to say.

pub mod board;
pub mod bridge;
pub mod profile;

pub use board::{
    Board, Encounter, LockProgress, MapWinRate, Notice, Party, Player, Score, Session,
    SessionPoint, Skin, StackGuess, Streak, TeamStats, TopAgent, WeaponSkin,
};
pub use bridge::{Bridge, Event, Status};
pub use profile::{Averages, BonusBuy, CareerMatch, CoPlayer, ForceHabit, Gun, Profile};
