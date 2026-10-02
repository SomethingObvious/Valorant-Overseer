# The Window

`overseer.exe` is Valorant Overseer's window, a Rust program. It draws the board the Python
backend sends over the local bridge, with Riot's own art built into the
binary: agent portraits, killfeed crops, rank emblems, player cards and map
strips. Rest the pointer on a row, or select it, and the detail panel shows
that player's recent matches and your notes and tags on them. The overlay puts the board over the game without
taking a click, and the tray icon brings the window back from it. Closing the window
quits.

The setup wizard, `overseer-setup.exe`, is built here too. It asks which region
you play in, then runs `scripts/install.ps1` with the log on screen, so there
is only one installer to keep right.

A release also ships it as `Valorant-Overseer-Setup.exe`, with the release zip
appended to the exe. That copy finds the zip on itself (`payload.rs`), unpacks
it into `%LOCALAPPDATA%\Programs\Valorant Overseer` with Windows' own
`tar.exe` and then runs the same `install.ps1`, so a player downloads one file
and never picks a folder.

## How It Fits Together

There are four crates. `overseer-core` is the data layer: a client for
`backend/ws_server.py` on its own thread, and the board and profile types,
named the way `backend/` names them. `overseer-ui` is the design system the
window and the wizard share, with the fonts and Riot's art. `overseer-app` is
the window and `overseer-setup` is the wizard.

The window is a client of the Python backend. It reads the port and the per-launch token from
`.overseer/bridge.json` and connects to the bridge on 127.0.0.1. It sends no
`Origin` header, since the bridge closes any connection that has one. When the
backend goes away it reconnects, waiting half a second at first and doubling
up to ten. The window starts that backend itself, hidden, as
`backend\run.py --prod --parent <pid>` under the install's
`.venv`, and the backend exits when the window does. With no `.venv` it just
waits for a backend to appear.

It finds the install by walking up from its own exe to the first directory
with a `.overseer` in it, which is the install directory once shipped and the
repository root in a checkout. `OVERSEER_ROOT` overrides that, so a checkout
can run while an installed copy is on the machine too. Only one window runs at
a time. A second launch knocks on 127.0.0.1:47873, the first window comes to
the front, and the second one exits. Settings go in `.overseer/app.json` and
notes in `.overseer/notes.json`, both plain JSON you can fix or delete by hand.

## Building and Running

You need rustup and the MSVC build tools. `rust-toolchain.toml` pins Rust
1.98.0 for `x86_64-pc-windows-msvc`, and rustup fetches it on the first build.
Run `install.bat` once before any of this, so there is a `.venv` for the
window's backend. Cargo runs from `crates`, where the workspace is.

```powershell
cd crates
cargo build --release                         # target\release\overseer.exe and overseer-setup.exe
..\install.bat                                # setup copies both to the install root and makes the shortcuts
cargo run -p overseer-app                     # the window from the checkout, starting its own backend
python ..\backend\run.py --window             # the window, with a backend that run.py starts and watches
target\release\overseer.exe --probe | Out-Host
target\release\overseer-setup.exe --silent --profile both --region na | Out-Host
```

`--probe` prints the install root, the path to `bridge.json` and whether that
file is there, and exits without opening a window. The pipe matters, because
both programs are Windows GUI programs and PowerShell only waits for one and
shows its output when the output goes somewhere. The window prints the GPU
adapter it got the same way when it starts. `--silent` runs setup with no
window. `--profile` takes `app`, `cli` or `both` and defaults to `app`, and
`--region` takes `na`, `eu`, `ap`, `kr`, `latam` or `br` and defaults to `na`.

Setup only copies a build that is newer than the one already in the install
root, and it closes a running window first, since an open window holds its
file.

## What It Won't Do

Nothing that acts on the game, at any point. No instalock, no dodging, no
queue automation, no input injection, no reading another process's memory.
The overlay is an ordinary always-on-top window with no hooks and no graphics
injection. It shows over VALORANT's default borderless mode, and nothing can
draw over exclusive fullscreen. The Ctrl+Alt+O hotkey that switches it is
plain `RegisterHotKey`, the API every screenshot tool uses. The window doesn't
go online either. Its art is in the binary, and the only connections it makes
are to the bridge and, at start-up, to an earlier copy of itself, both on
127.0.0.1. This is the rule the whole project is built around, and the window
doesn't relax it.

## Staying Out of the Game's Way

The window asks wgpu for the low power adapter, which on a laptop with two
GPUs is the integrated one, so VALORANT keeps the discrete one. Flat shapes,
text and one shader that touches each pixel once need nothing more. It is only
a request, so a machine with one GPU gets that one.

Nothing polls. egui repaints when the bridge, the hotkey, the tray or a
running animation asks it to, so a window nobody touches draws nothing.
Quality is auto, efficient or rich, on the settings screen. Efficient is the
same board with no movement and no shader. Auto starts rich and drops to
efficient, and says so, once three frames in a row take longer than 1/45 of a
second to build. It ignores the first second and a half, when the font atlas,
the shader and the swapchain miss every budget there is, and it tries rich
again after 600 good frames.

## Tests

`cargo test --workspace` renders every layout of the window and every step of
the wizard offscreen through wgpu and compares each one with a PNG in that
crate's `tests/snapshots`. The window's pinned pictures all come from one
test, so their wgpu devices never run in parallel, because two at once crash
this machine's driver with an access violation. After a deliberate change to
the look, run the tests with `UPDATE_SNAPSHOTS=1` set and look at every new
PNG before committing.

`src/perf.rs` in `overseer-app` is the performance budget, as tests that count
work rather than time. A timing assertion measures whatever else the machine
is busy with, but a frame that asks for another frame, or a board that
suddenly draws twice the shapes, is the same on every run. A window left alone
has to stop asking for frames, a full ten player board has to draw at most 330
shapes and at least half that, and the overlay has to be exactly as tall as it
was placed for.

To look at lobbies built to break the layout, set `STRESS_OUT` to a folder and
run `cargo test -p overseer-app stress -- --ignored`. Those images are for
looking at 1:1 after a change, and nothing compares them against anything.
With `OVERSEER_TRACE_FRAMES` set, the window prints what each frame cost
and the size and scale it drew at.

## The Lint Stack

`scripts\lint.ps1` checks the Rust half in the same run as everything else:
`cargo fmt --check`, clippy over every target, the tests and snapshots,
`cargo doc` to reach the rustdoc lints, which only fire under rustdoc, and
`cargo deny` for advisories, bans, licences and sources. A missing `cargo` or
`cargo-deny` fails the gate rather than skipping the check. Install cargo-deny
with `cargo install cargo-deny`.

The lint setup is afterimage's, adapted, because it is already stricter than
anything worth writing from scratch. `rust-toolchain.toml` pins the toolchain,
since "whatever rustc is on this machine" is how a lint appears or disappears
without anybody changing a line. `unsafe_code` is forbidden rather than
denied, so no `#[allow]` can switch it back on. `missing_docs` and the rustdoc
lints are denied, and so are clippy's `pedantic`, `nursery` and `cargo` groups
and a hand-picked set of `restriction` lints, like `unwrap_used`,
`expect_used`, `panic`, `indexing_slicing` and `print_stderr`, which are the
ones that stop a scoreboard from taking the app down mid-match.
`allow_attributes` is denied as well, so the repository's ban on inline
suppressions holds at the syntax tree rather than as a regular expression over
the file. The few relaxations are at the bottom of `crates/Cargo.toml`, each
with its reason.

The Rust configs (`clippy.toml`, `rustfmt.toml`, `deny.toml` and
`rust-toolchain.toml`) sit in `crates` beside the workspace `Cargo.toml` that
holds the lint levels. `lint.ps1` fails on any of them anywhere else, since a
lint config lower down is a suppression nobody reads.

## Why egui and Not a Web View

The window is egui and eframe on wgpu: one language, one binary, no web view.
Tauri with a React front end is the obvious other choice, since afterimage
already ships one and effects are easier in CSS. Held against what this app
needs for years, which is being easy to keep, easy to upgrade and fast, it
loses. Tauri means three languages, two build systems and two lint gates
where this has one of each, and it adds a hand-written boundary between Rust
and JavaScript on top of the one with the backend. It brings tauri, its
plugins, its JavaScript API and CLI, React, Vite, TypeScript, Vitest and
WebView2 to keep upgraded, where egui's family moves together on one version
number: `egui`, `eframe` and `egui_kittest` are all 0.36.2. A web view adds a
WebView2 process to the memory floor and its start-up to the cold start, and
Edge's compositor decides the frame, where eframe lets the app decide even
which GPU draws it.

`egui_kittest` settles it. The case for a web view is that the UI can be
opened in a browser and looked at. A harness that renders the real UI
offscreen and compares it with a picture is better than that, because the look
becomes a test that fails when it changes rather than something somebody
remembers to check.

The cost is that visual work is slower in a painter than in CSS, so the look
takes longer to arrive, and there is no HTML to fall back on when something
proves awkward. For an app whose job is to be fast and to sit beside a running
game, that is the right way round.

## The Python Backend Stays for Now

`backend/` is about 10,000 lines, and its value is in the parts nobody sees:
the 429 handling, the caches keyed on match ids, the queue preference order in
`_fresh_mids`, the throttles that keep this read-only and unbannable.
Rewriting that in one go is where a ban comes from. The window only reaches
the backend through `overseer-core`, so native reads can replace the bridge
one endpoint at a time. Each port is meant to land with a differential test
that runs Rust and Python against the same live client and asserts identical
output, with Python as the oracle. The order is cheapest and safest first:
lockfile and entitlements, presences and game state, the board with ranks and
levels, then history and career. When the last one lands, the bridge goes,
and the window needs no Python after that.

Setup lists the app in Settings > Apps, and the Uninstall button there runs
`scripts/uninstall.ps1`. It closes the app and removes the shortcuts, the
entry, what the app keeps in AppData and the install. It deletes only what a
release puts in the folder, never follows a junction out of it, and leaves
a folder other programs share alone. With the folder already deleted or
moved, the same button clears what it left in Windows. Notes, lineups, settings and match data stay unless
the player says otherwise, so installing again picks them back up. On a
copy of the source it removes only the shortcuts and the entry.
