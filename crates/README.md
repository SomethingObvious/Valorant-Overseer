# The window

A second front end for Valorant Overseer: one Rust binary with a real window,
built beside the terminal one rather than instead of it. The installer asks
which you want and installs only that.

This file is the plan of record. It says what was decided, why, and in what
order it gets built, so a session that picks this up later does not re-argue a
settled question or rediscover a constraint the hard way.

## The stack, and why it is this one

**egui and eframe, on wgpu. One language, one binary, no webview.**

The first answer here was Tauri with a React front end, on the grounds that
afterimage already ships that and the effects are easier in CSS. Held against
the actual requirement, which is maintainability, upgradability and speed over
years, it loses on all three:

| | Tauri and React | egui and eframe |
| --- | --- | --- |
| Languages, build systems, lint gates | 3, 2, 2 | 1, 1, 1 |
| A typed boundary that can drift | Rust to JavaScript IPC, by hand | none: the board struct is the input |
| Things to keep upgraded | tauri, three plugins, the JS api, the CLI, react, vite, typescript, vitest, and WebView2 | egui, whose whole family shares one version |
| Breaking migration already announced | Tauri 3, in alpha now | quarterly renames, mechanical |
| Memory floor | a WebView2 process on top of ours | one process |
| Cold start | plus webview init | plus nothing |
| Who controls the frame | Edge's compositor | we do, including which GPU |
| Visual regression testing | screenshots, by eye | `egui_kittest` golden images, in process |
| Polish per hour of work | better | worse |

The last row is the only one Tauri wins, and it is the one that costs time
rather than correctness. Every other row is a thing that would be paid for
every month for as long as this exists. Tauri 1 to 2 was a migration; 3 is
already in alpha, and this app would be signing up for it.

`egui_kittest` is what reversed the argument. The case for a webview was that
the UI could be opened in a browser and looked at. A first party, version
locked snapshot harness that renders the real UI offscreen and writes images is
strictly better than that: the look becomes a test that fails when it changes,
rather than a thing somebody remembers to check.

The cost, stated plainly: visual iteration is slower in a painter than in CSS,
so the look takes longer to arrive, and there is no HTML to fall back on if
something proves awkward. For an app whose job is to be fast and to sit beside
a running game, that is the right way round.

Versions move together, which is the point: `egui`, `eframe`, `egui_extras`,
`egui_plot` and `egui_kittest` are all 0.36.2 today. One number to bump.

## Shape

```text
crates/overseer-core     the data layer: Riot reads, board assembly, stats
crates/overseer-app      eframe and wgpu. Links core directly. One exe.
crates/overseer-setup    the wizard
backend/  tui/           untouched, and the CLI profile keeps working
```

No socket, no token, no IPC, no second process. The app links the data layer
as a library and calls it.

## The data layer, and how Python leaves

`backend/` is 16k lines whose value is in the parts nobody sees: the 429
handling, caches keyed on match ids, the queue preference order in
`_fresh_mids`, the throttles that keep this read-only and unbannable.
Rewriting that from scratch over a weekend is where a ban comes from.

So `overseer-core` defines a trait per group of reads, and ships two
implementations. The first proxies to the running Python bridge, which is how
the window has real data on its first day. The second is native Rust, written
one endpoint at a time, and each port lands with a differential test that runs
both against the same live client and asserts identical output, with Python as
the oracle. When the last endpoint is ported the proxy goes, and the app needs
no Python at all. The terminal front end can then point at the same core and
the CLI profile loses Python too.

Ported in this order, cheapest and safest first: lockfile and entitlements,
presences and game state, the board with ranks and levels, then history and
career, then inventory.

## Performance

The first goal, so it is written as numbers that fail the gate rather than as
an intention.

| Measurement | Budget |
| --- | --- |
| Cold start to first painted frame | 250 ms |
| Idle draw rate | 0 per second |
| Idle CPU | 0.2% |
| Board update, data to painted | 4 ms |
| Frame during a transition | 8.3 ms at 120 Hz, 16.6 ms at 60 Hz |
| Resident memory, app open | 90 MB |
| Resident memory, every view visited | 160 MB |
| Shipped binary | 20 MB |
| Wizard binary | 3 MB |

How they are met:

- **Ask for the integrated GPU.** This laptop has an AMD integrated adapter
  driving the internal panel and an RTX 4070 driving the external one.
  VALORANT wants the 4070. The scoreboard asks wgpu for the low power adapter
  and keeps its VRAM use trivial, so it never takes frames from the game. This
  is the largest single win available and it is specific to this machine.
- **Reactive, not continuous.** egui repaints on input or when something asks
  it to. Nothing here asks unless an animation is running or the board
  changed, so a window sitting still costs nothing at all, and a minimised one
  is not drawn.
- **Two quality tiers, and an automatic choice between them.** Efficient has
  no shader passes, no blur, no shadows, fades of 150 ms or less, a static
  background and a 30 fps ceiling. Rich has the shader background, bloom on a
  rank up, parallax and 120 fps. Auto picks Rich on mains power with a focused
  window, and drops to Efficient on battery, in a remote session, or after
  three consecutive frames over budget. A downgrade is measured, said out loud
  in the UI, and reversible.
- Textures uploaded once and reused, no per-frame allocation in the paint
  path, glyph atlases warmed at startup, and images decoded off the UI thread.
- A harness in `crates/overseer-app/benches` plus a `--perf` mode records cold
  start, the idle draw count over a minute, frame times through a scripted
  transition, resident memory, and which adapter and driver version it ran on.
  Thresholds are part of `scripts/lint.ps1`. Recording the adapter matters
  here: the NVIDIA driver on this machine is behind, which is exactly the
  condition under which the Rich tier has to fall back cleanly.

## Look

The palette is the terminal app's, so the two read as one product: `#FF4655`
red, `#18E5A7` ally green, `#9ADEFF` ice, `#ECE8E1` bone, `#0B1119` ink.

Borrowed deliberately, and named so the borrowing is a choice rather than a
mood: VALORANT's diagonal wipe between screens, the agent select roster strip,
round pips, the ranked badge with its RR bar, team tinted scoreboard rows and
kill feed cards. From elsewhere: stat tiles with sparkline trends, and a
command palette on Ctrl+K.

One celebratory moment, not effects everywhere. A rank up gets bloom and a
rolling number. Everything else snaps: 180 ms for the wipe, 90 ms for content
rising into place, 60 ms for a row tint on selection, 40 ms of stagger when a
roster deals in. Reduced motion and the Efficient tier show the same
information with no movement.

Three layouts and an overlay. Compact under 520 px is one column for the
corner of a second monitor. Normal is the board and the panel. Full, past
1800 px, adds a live round timeline. The overlay is always on top, click
through until hovered, and hides itself under exclusive fullscreen. A flat,
sharp, high contrast design is easier to paint than a soft glassy one, which
is the one way this stack makes the look cheaper rather than dearer.

## What the window can do that a terminal cannot

Agent portraits, rank badges, map thumbnails and weapon skins, all of which
the backend already resolves. Charts through `egui_plot`: RR across the act,
K/D trend, win rate by map, headshot distribution. A tray icon and a global
hotkey to summon the overlay. A notification when a lobby fills or a flagged
account appears. The detail panel detached onto the second monitor, which egui
supports as a second viewport. Private per player notes and tags, searchable.
Two players compared side by side. The board copied to the clipboard as an
image. A history browser with filters and a round by round scrubber. Drag to
reorder and resize columns. Opt in sounds. A theme editor writing the same
settings file the terminal app reads.

Not in scope, at any point: anything that acts on the game. No instalock, no
dodging, no queue automation, no input injection, no reading another process's
memory. The overlay is an ordinary always on top window with no hooks and no
graphics injection. This is the rule the whole project is built around and the
window does not relax it.

## Phases

Each phase ends with something that works, and lands as one commit.

- **P0, the spike.** The workspace, the lint stack, a window, the proxy data
  layer against the running backend, and the board as unstyled rows. Proves
  cold start, an idle cost of zero, the integrated adapter, a golden image
  test, and that the lint stack is green. The point is to find out early if any
  of that is false.
- **P1, the board.** Layouts, theme, images, the detail panel, settings shared
  with the terminal app, both quality tiers, and the performance harness with
  its thresholds wired into the gate. The first version worth using daily.
- **P2, the wizard.** `overseer-setup.exe`: checks the machine, asks for app,
  CLI or both, installs only what that profile needs, writes the profile into
  `.overseer/installed.json`, makes shortcuts, saves the region, and supports
  `--silent --profile`. `install.bat` becomes a front end for it, and there is
  an uninstall path, because an installer that cannot undo itself is half an
  installer.
- **P3, motion.** The wipe, the deal in, the rank up moment, the shader
  background, and Efficient parity for all of it.
- **P4, the window's own features.** Tray, hotkey, overlay, notes, charts,
  history browser, command palette, compare, share card.
- **P5, the optimisation pass.** Profile against the harness, cut what misses,
  shrink the binary, and put the numbers in the README so a regression is
  visible rather than theoretical.
- **P6, the port.** The Riot layer to Rust, endpoint by endpoint, each with its
  differential test against Python, ending with Python retired.

## The lint stack

Copied from afterimage and adapted, because it is already stricter than
anything worth writing from scratch: the toolchain pinned in
`rust-toolchain.toml`, `unsafe_code` forbidden rather than denied,
`missing_docs` and the rustdoc lints denied, clippy's `pedantic`, `nursery` and
`cargo` groups denied, the individually chosen `restriction` lints
(`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `print_stderr` and
the rest), and `allow_attributes` denied so this repository's ban on inline
suppressions holds at the syntax tree rather than as a regular expression over
the file.

`cargo deny` covers advisories, bans, licences and sources. Beside it:
`cargo fmt --check`, `cargo doc` to reach the rustdoc lints, `cargo nextest`,
`cargo miri` on the pure logic, `cargo machete` for dependencies nothing
imports, and `cargo bloat` against the size budget.

Every config sits at the repository root, next to `ruff.toml` and `mypy.ini`,
because a lint config in a subdirectory is a suppression nobody reads and
`lint.ps1` rejects one. All of it hangs off `lint.ps1`, which stays the only
gate, and which fails when a tool is missing rather than quietly skipping the
check.
