//! The data layer, a client of the Python bridge that owns every request to
//! Riot. The window goes through this crate so the native reads planned in
//! `crates/README.md` can replace the bridge one endpoint at a time.

pub mod board;
pub mod bridge;
pub mod games;
pub mod lineups;
pub mod profile;

pub use board::{
    AutoTag, Board, Encounter, LockProgress, MapWinRate, Notice, Party, Player, Score, Session,
    SessionPoint, Skin, StackGuess, Streak, TeamStats, TopAgent, WeaponSkin,
};
pub use bridge::{Bridge, Event, Status};
pub use games::{Game, Line, Recent};
pub use lineups::{Ability, Atlas, Callout, Clip, Drawing, Kit, Lineup, Plan, Tools};
pub use profile::{Averages, BonusBuy, CareerMatch, CoPlayer, ForceHabit, Gun, Profile};
