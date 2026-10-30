//! What the demo backend really sends, read through the types the window
//! uses. The other tests here read JSON written by hand, which can't notice
//! the backend starting to send a null or a float where Rust wants a number.

use std::path::Path;
use std::process::Command;

use overseer_core::{Board, Profile, Recent};

/// Asks the backend for three boards, a history and a profile, the way the
/// window would, with `allow_nan` off, since serde refuses a NaN and would
/// drop the whole frame.
const DUMP: &str = r#"
import json, sys
sys.path.insert(0, "backend")
import app
boards = [app.build_live(7, state) for state in ("", "menus", "pregame")]
puuid = boards[0]["players"][0]["puuid"]
print(json.dumps({
    "boards": boards,
    "history": app.handle_data_request("history", {"count": 5}),
    "profile": app.handle_data_request("profile", {"puuid": puuid}),
}, allow_nan=False, default=str))
"#;

#[test]
fn the_backends_own_answers_read() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let python = root.join(".venv").join("Scripts").join("python.exe");
    let out = Command::new(&python)
        .args(["-c", DUMP])
        .current_dir(&root)
        .env("DATA_SOURCE", "demo")
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "couldn't run {}: {e}. Run install.bat first",
                python.display()
            )
        });
    assert!(
        out.status.success(),
        "the backend failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sent: Sent = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("the backend's answers didn't read: {e}"));
    assert_eq!(sent.boards.len(), 3);
    assert!(
        sent.boards.iter().all(|b| !b.players.is_empty()),
        "a demo board has players"
    );
    assert!(!sent.history.games.is_empty(), "the demo history has games");
    assert!(
        !sent.profile.matches.is_empty(),
        "the demo profile has matches"
    );
}

/// What `DUMP` prints.
#[derive(serde::Deserialize)]
struct Sent {
    boards: Vec<Board>,
    history: Recent,
    profile: Profile,
}
