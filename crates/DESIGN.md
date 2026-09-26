# The design system

This is the design system for the window in `crates/overseer-app`. It is the
plan of record for how the app looks, the way `crates/README.md` is the plan
of record for what it is. It was written from the built app, after the build,
so it describes what is on the screen rather than what somebody hoped would be.

An app looks generated when every screen was decided separately. The cure is
fewer decisions, each with a reason, used everywhere. Use the token, not the
literal.

## What this app is

A scoreboard beside or over a running game that answers one question in under
a second: **who am I about to play against, and which of them should I worry
about?** The enemy team is the reason the app is open. Every decision serves
reading those five players fast, under pressure, with as few inputs as
possible.

## The world: a broadcast graphic

The board is drawn the way a VCT broadcast draws a team: solid plates, heavy
condensed numerals, each player's own face at the head of their row. The
owner chose it over three other directions, and it replaced a system whose
first principle was "sharp, flat, quiet". That principle is why three rounds
of polish on the old look never landed: every revamp polished a spreadsheet of
twelve equal columns at thirteen points instead of replacing it.

What it refuses:

- **The tracker spreadsheet.** Twelve columns at one size is a table with no
  answer in it. The K/D is the largest numeral on the board because it is the
  number anybody scans first.
- **Near black, one neon accent, glowing edges.** That is the look generated
  dark interfaces land on. The ground here is neutral black with the blue
  taken out, the red owns one whole plate instead of trimming everything, and
  nothing glows unless something is lit.
- **Letters standing in for pictures.** Every tracker anybody uses shows the
  agent and the rank emblem. So does this one, at a size where they are the
  thing rather than an icon of it.

## Principles

1. **The enemy is the graphic.** The enemy block owns the one solid colour on
   the board: a red team plate with black type. Enemy rows are 56 points with
   the largest type; your own rows are the same row at two thirds the size and
   half the volume, under a rule rather than a plate. You already know who is
   on your team.
2. **Colour says whose number it is.** On the enemy's side a good number is
   bad news, so it runs from neutral through amber to red
   (`colour::threat`). On yours it runs to green (`colour::strength`). The old
   board painted an enemy's 1.92 K/D in ally green because a good number was
   green whoever it belonged to.
3. **Riot's art, at the size where it is the thing.** Killfeed crops at the
   head of every row, rank emblems in their own light, square portraits on the
   ladder, the map behind the masthead, and the agent's face across the top of
   the panel.
4. **Slabs stand on the ground.** A row is an object: a fill, a hairline of
   light along its top edge, and a shadow with an offset below it. A shadow
   with no offset is a glow, and a glow round everything is decoration.
5. **Light only where something is lit.** Three things carry light and
   nothing else does: a rank emblem (the tier's colour falling off round it),
   the connection light, and a score digit for as long as it takes the
   animator to catch up with a round just won.
6. **The slant is punctuation.** About twelve degrees (`paint::LEAN`), on the
   team plate's tail, the right edge of a killfeed crop, chips, pips and the
   score's rule. Rows themselves are rectangles; a slant on every edge is
   noise.
7. **One entrance, and nothing at idle.** When a lobby lands a red slab wipes
   off the enemy block in 280ms, exponential ease-out, and the numerals count
   up once over 400ms. That is the only entrance on the board. Hover lifts a
   slab over 150ms. Nothing moves while nothing is happening.
8. **Measure before drawing.** Every row of chips, every team plate and every
   header item is measured and dropped if it would reach its neighbour.
   Printing a word over another reads as a rendering fault, not a narrow
   window.

## Tokens

All of these live in `crates/overseer-ui/src/lib.rs`. A literal in a widget is
a decision somebody made alone.

**Colour.** Contrast figures are on the slab (`BG_RAISED`).

| Token | Value | Role |
| --- | --- | --- |
| `VOID` | `#050608` | Below the ground: shadows |
| `BG` | `#0A0B0E` | The ground |
| `BG_RAISED` | `#15171C` | A slab: a row, a card, a plate |
| `BG_INSET` | `#1D2027` | A slab on a slab: chips, inputs |
| `BG_HOVER` | `#1C1F26` | A slab under the pointer |
| `BG_SELECTED` | `#252933` | The slab you chose |
| `LINE` | `#2A2E36` | A rule, where one is really needed |
| `TEXT_STRONG` | `#F2EFE8` | Names and numerals, 15.6:1 |
| `TEXT` | `#C9CED6` | Values, 11.3:1 |
| `TEXT_DIM` | `#9AA1AB` | Secondary, 6.9:1 |
| `TEXT_FAINT` | `#8B929C` | Labels, 5.7:1 |
| `ENEMY` | `#FF4655` | The enemy's plate and nothing else loud |
| `ALLY` | `#18E5A7` | Your side |
| `WARN` | `#FFC845` | Worth a look: the one colour meaning "this one" |
| `PARTY` | `#7AA2FF #C58BFF #5CE1E6 #F28DD5` | Party brackets, never red, green or amber |

Rank colours are Riot's, from their own competitive tier data, and appear only
in the emblem's light and the tier's name.

**Type.** Two families, four cuts, all shipped inside the binary.

| Face | Cut | Used for |
| --- | --- | --- |
| `Face::Heavy` | Barlow Condensed ExtraBold Italic | Numerals, team titles, the map, the score, section headings |
| `Face::Display` | Barlow Condensed Bold | Names, labels, chips: caps that are scanned |
| `Face::Number` | Barlow Condensed SemiBold | Secondary numbers and quieter labels |
| `Face::Body` | Inter, weight 440, optical size 16 | Sentences: reasons, notes, explanations |

Barlow Condensed is cut from the lettering on signs and number plates and has
an italic heavy enough to carry a numeral across a room. Its digits are
proportional, so the big numerals are set in fixed cells (`paint::numeral`):
each digit gets the width of the widest and sits in the middle of it, which
is what a tabular figure is, and a column of 1.11 over 0.88 lines up at the
point. Inter is the only face for reading word by word.

**Space.** Everything on a four point grid: `SM 4`, `MD 8`, `LG 12`, `XL 16`,
`XXL 24`, rows `ROW 28` and `ROW_TIGHT 24`. A test fails on anything off it.

**Rows.** Three measurements, in `board/rows.rs`:

| | Height | Name | Emblem | K/D | Other stats |
| --- | --- | --- | --- | --- | --- |
| Enemy, window | 56 | 21 | 36 | 32 | 17 |
| Ally, window | 36 | 16 | 24 | 21 | 14 |
| Enemy, overlay or narrow | 44 | 18 | 28 | 26 | 15 |

The killfeed crop is always exactly as tall as the row and twice as wide:
Riot cut it two by one, and a face stretched to fit a box is not a face.

**Motion.** `INSTANT 150ms` for a hover, `QUICK 200ms` for a selection or a
cross-fade, `MEASURE 700ms` for a bar whose length is a number, and the
stinger's own 280ms and 400ms. Everything decelerates. The efficient tier
shortens every one of them to nothing and draws no shadow and no light.

## Components

- **Slab** (`paint::slab`): fill, top light, offset shadow, lift under the
  pointer.
- **Team plate** (`heads::team`): solid red for the enemy with the worth-a-look
  chip and the lobby averages in black; a green rule and title for yours.
- **Row** (`rows::row`): crop, name and agent, rank, the stats the grid kept,
  last five as slanted pips. A flagged account gets an amber rail, a faint
  amber wash, and its reasons as a second line in Inter with the numbers in
  amber. An account the backend could not see is one short quiet line.
- **Rank cell**: the emblem in its light, the tier's name, and the peak under
  it when the peak is two ranks or more above today. When room is tight the
  rank keeps its emblem and gives up its word, because the emblem is the rank
  to anybody who plays and the columns it would cost are not said anywhere
  else.
- **Masthead** (`header.rs`): the map's own art dimmed behind a ramp, the map
  as the title, state, queue and side as plates, the score centred.
- **Player card** (`panel.rs`): the agent's killfeed crop across the panel,
  the name set in the dark it fades to, then the verdict band, the rank in its
  light at 64 points, and the numbers.
- **Ladder** (`board/ladder.rs`): every ranked account on Riot's tier colours,
  enemies above, allies below, faces nudged apart without leaving the strip.
- **Shared controls** in the design crate: `choice`, `say`, `keycap`.

## Layout

- **One board, one door.** The window, the overlay and the snapshot tests all
  go through `board::draw` with one plain `Scene`. The tests used to assemble
  their own copy of the board, and fourteen things about it had drifted from
  what the window drew.
- **One grid a frame.** `board::grid::Grid` places every column once, and the
  heads and every row read from it. Columns are added by priority rather than
  shed, and hiding one can never take a different one away.
- **A row reserves its whole height before it asks whether it is on screen**,
  reasons line included, so nothing jumps as the board scrolls.
- **The overlay** is enemies only, at the narrow measurements. It is placed
  once, as if it were always its designed height (the plate and five rows),
  and only ever resized after that: a window moved after it is shown stops
  being composited on this machine. It is exactly as tall as its board
  because this adapter presents opaque, and a fixed tall overlay would lay a
  black slab across the game under its last row.

## Where the pictures come from

Everything under `crates/overseer-ui/assets/` except the fonts is Riot's own
art, fetched once by hand from valorant-api.com, which mirrors Riot's game
assets, and committed. Nothing in the app goes online. Riot permit this kind
of personal, non-commercial fan use of their art; this app is private and is
not distributed.

| Folder | Source field | Processing |
| --- | --- | --- |
| `agents/` | agent `displayIcon` | Trimmed to its content, fitted to 128 px square |
| `killfeed/` | agent `killfeedPortrait` | Unchanged, 256 by 128 |
| `ranks/` | competitive tier `largeIcon`, latest episode | Trimmed, fitted to 96 px square |
| `maps/` | map `listViewIcon` | Resized to 304 by 67, blurred by 0.8 px, 64-colour palette |

The map strips are palettes because a photograph behind the dimming costs
three hundred kilobytes as RGBA and about nine as sixty four colours, and
nobody can see the difference through the dark. `art_assets.rs`, which binds
them into the binary, is generated from those folders. When Riot ship an
agent or a map, fetch it the same way; an agent this build has never heard of
falls back to a plate in its colour with its initial.

Barlow Condensed and Inter are under the SIL Open Font License, whose text is
beside them.

## What it costs

Measured on the release build against a live backend, on the integrated
adapter the window deliberately asks for, at 2264 by 1464 device pixels:

| | |
| --- | --- |
| Idle | nothing measurable |
| Pointer swept down the roster as fast as it moves | 0.34% of a core |
| Committed memory | 446 MB |
| Resident memory | 390 to 460 MB, or 16 MB once Windows has trimmed it |

The two memory figures only mean anything together. The resident number is
not reproducible alone: six samples in one run came out 391, 13, 16, 16, 16
and 17 megabytes, because Windows trims the working set of a window nobody is
touching and pages it back the moment anything is. The committed figure has
been 445 to 446 in every sample of every run. Almost none of it is this app's
data; it is wgpu and the adapter's driver.

Idle is nothing because the window is reactive. The bridge wakes it when a
board arrives, egui's animator asks for frames while a tint is in flight, and
the stinger asks for exactly as long as it runs. `perf.rs` holds a shape
budget for a full board and asserts that a still window asks for nothing.

## How to know it worked

`crates/overseer-app/src/shot.rs` renders the real board offscreen through
wgpu and compares it to a PNG in `tests/snapshots`. After any visual change:

```console
UPDATE_SNAPSHOTS=1 cargo test --workspace
```

then **open the PNG and look at it** at full size, region by region, and list
what is wrong before deciding it is finished. The test protects against an
accidental change; only a person can say whether the deliberate one was any
good. For the overlay, capture it off the screen rather than from the window:
a window capture has no alpha and shows empty space as black whether or not it
is.

## What "generated" looks like here, so it can be avoided

- Near black, one neon accent, glowing edges. If somebody could guess the look
  from "dark gaming app", it is the default.
- A glow with no offset round something that is not lit.
- Every column the same size, so nothing on the row is the answer.
- A letter or a unicode glyph where a picture or a drawn mark belongs.
- A small caps label above a heading that already says what it is.
- Motion on load, hover, scroll and selection at once. There is one entrance.
- Spacing of 5, 7, 13. If a gap is not on the grid, it is a guess.
- Labels that name the system: "smurfReasons", "puuid". Write what the player
  would say: "Worth a look", "Met 4 times".
