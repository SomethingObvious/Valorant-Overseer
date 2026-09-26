//! Riot's own agent portraits and rank emblems, bound into the binary.
//!
//! Generated. The script that writes it lives in the session that added it
//! and is not part of the build: these files change when Riot ships an agent,
//! which is a handful of times a year and is a decision somebody takes rather
//! than something a build does behind their back. Nothing in this app goes
//! online, and that is the whole reason they are here rather than fetched.
//!
//! Keyed by the name the backend sends, lowercased with everything that is
//! not a letter or a digit removed, so `KAY/O` and `kayo` are the same agent.

/// Every playable agent's portrait.
pub(crate) const AGENTS: [(&str, &[u8]); 29] = [
    ("astra", include_bytes!("../assets/agents/astra.png")),
    ("breach", include_bytes!("../assets/agents/breach.png")),
    (
        "brimstone",
        include_bytes!("../assets/agents/brimstone.png"),
    ),
    ("chamber", include_bytes!("../assets/agents/chamber.png")),
    ("clove", include_bytes!("../assets/agents/clove.png")),
    ("cypher", include_bytes!("../assets/agents/cypher.png")),
    ("deadlock", include_bytes!("../assets/agents/deadlock.png")),
    ("fade", include_bytes!("../assets/agents/fade.png")),
    ("gekko", include_bytes!("../assets/agents/gekko.png")),
    ("harbor", include_bytes!("../assets/agents/harbor.png")),
    ("iso", include_bytes!("../assets/agents/iso.png")),
    ("jett", include_bytes!("../assets/agents/jett.png")),
    ("kayo", include_bytes!("../assets/agents/kayo.png")),
    ("killjoy", include_bytes!("../assets/agents/killjoy.png")),
    ("miks", include_bytes!("../assets/agents/miks.png")),
    ("neon", include_bytes!("../assets/agents/neon.png")),
    ("omen", include_bytes!("../assets/agents/omen.png")),
    ("phoenix", include_bytes!("../assets/agents/phoenix.png")),
    ("raze", include_bytes!("../assets/agents/raze.png")),
    ("reyna", include_bytes!("../assets/agents/reyna.png")),
    ("sage", include_bytes!("../assets/agents/sage.png")),
    ("skye", include_bytes!("../assets/agents/skye.png")),
    ("sova", include_bytes!("../assets/agents/sova.png")),
    ("tejo", include_bytes!("../assets/agents/tejo.png")),
    ("veto", include_bytes!("../assets/agents/veto.png")),
    ("viper", include_bytes!("../assets/agents/viper.png")),
    ("vyse", include_bytes!("../assets/agents/vyse.png")),
    ("waylay", include_bytes!("../assets/agents/waylay.png")),
    ("yoru", include_bytes!("../assets/agents/yoru.png")),
];

/// Every ranked tier's emblem, by Riot's tier number.
pub(crate) const RANKS: [(u32, &[u8]); 25] = [
    (3, include_bytes!("../assets/ranks/3.png")),
    (4, include_bytes!("../assets/ranks/4.png")),
    (5, include_bytes!("../assets/ranks/5.png")),
    (6, include_bytes!("../assets/ranks/6.png")),
    (7, include_bytes!("../assets/ranks/7.png")),
    (8, include_bytes!("../assets/ranks/8.png")),
    (9, include_bytes!("../assets/ranks/9.png")),
    (10, include_bytes!("../assets/ranks/10.png")),
    (11, include_bytes!("../assets/ranks/11.png")),
    (12, include_bytes!("../assets/ranks/12.png")),
    (13, include_bytes!("../assets/ranks/13.png")),
    (14, include_bytes!("../assets/ranks/14.png")),
    (15, include_bytes!("../assets/ranks/15.png")),
    (16, include_bytes!("../assets/ranks/16.png")),
    (17, include_bytes!("../assets/ranks/17.png")),
    (18, include_bytes!("../assets/ranks/18.png")),
    (19, include_bytes!("../assets/ranks/19.png")),
    (20, include_bytes!("../assets/ranks/20.png")),
    (21, include_bytes!("../assets/ranks/21.png")),
    (22, include_bytes!("../assets/ranks/22.png")),
    (23, include_bytes!("../assets/ranks/23.png")),
    (24, include_bytes!("../assets/ranks/24.png")),
    (25, include_bytes!("../assets/ranks/25.png")),
    (26, include_bytes!("../assets/ranks/26.png")),
    (27, include_bytes!("../assets/ranks/27.png")),
];
