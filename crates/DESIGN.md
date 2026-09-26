# The design system

This is the design system for the window in `crates/overseer-app`, and for
the terminal front end where the two overlap. It is the plan of record for
how the app looks, the way `crates/README.md` is the plan of record for what
it is.

An app looks generated when every screen was decided separately. The cure is
not more decoration, it is fewer decisions: a small set of values, used
everywhere, with a reason attached to each. Everything below is one of those
decisions. Use the token, not the literal.

## What this app is

A scoreboard that sits beside a running game and answers one question in under
a second: **who am I about to play with, and which of them should I worry
about?** Every decision serves reading speed under pressure. Nothing here is
a dashboard to browse or a place to spend time.

Riot's own direction for the VALORANT client is the right reference, in their
words: push contrast, simplify the shape language, remove decoration that does
not serve a purpose, and let motion express flow. This app follows that,
because it lives next to that.

## Principles

1. **The data is the design.** A number, aligned and coloured by meaning, is
   the interface. Frames, gradients and glows are what you add when the data
   is not saying enough, which means the fix is upstream.
2. **One accent.** Red means the enemy or a danger. Green means your side or a
   good outcome. Everything else is a shade of grey doing hierarchy. An
   interface where four things are bright has nothing bright.
3. **Alignment is credibility.** Numbers right aligned in a mono face, labels
   left aligned in caps, one baseline per row. Nothing looks hand assembled
   faster than a column that drifts.
4. **Sharp, flat, quiet.** No corner radius, no shadow, no blur in the
   Efficient tier. This is the game's language and it is also the cheapest
   thing to draw.
5. **Motion explains a change, or it does not happen.** A row tinting under
   the cursor, a roster dealing in, a rank going up. Never an entrance
   animation on something that was always there.

## Tokens

Do not write a literal into a widget. Everything comes from `design.rs`.

**Type.** Three faces, all shipped inside the binary, all variable, all
pinned to one cut:

| Role | Face | Why |
| --- | --- | --- |
| Display, labels | Oswald, weight 600 | Riot's display face is Tungsten Bold, always uppercase, and it is commercial. Oswald is the substitute Riot themselves use: their own site sets it wherever Tungsten cannot be. Tall, condensed, and legible at ten points, which a poster face is not. |
| Body, names | Inter, weight 420, optical size 16 | Riot's interface face is DIN Next W1G, also commercial, and the fallback at the end of their own DIN stack is Inter. Drawn for interfaces at small sizes. |
| Numbers | JetBrains Mono, weight 500 | Tabular by construction, so a column of K/D cannot drift. Consolas is a terminal font from 2004 and looked it. |

Shipped rather than read from Windows, which is what this did at first and
which was wrong twice over: Windows has no condensed display grotesque worth
using, and a design system whose type depends on which machine it is running
on is not a design system. About a megabyte and a quarter, and it buys the
app a voice.

All three files are variable and the rasteriser takes the default instance
unless told otherwise, which for Oswald is Regular and far too light to be a
heading. The weight axis is pinned per face in `Face::tweak`, along with the
nudge that puts three faces of one point size onto one baseline.

Sizes, in points, rounded to whole pixels because glyphs are rasterised:
`HERO 30`, `DISPLAY 20`, `TITLE 15`, `BODY 13`, `LABEL 11`, `MICRO 10`.
`HERO` is the score and nothing else.

**Tracking is laid out, not typed.** Caps get `extra_letter_spacing`, scaled
by size: about a sixteenth of an em at ten points down to a fiftieth at
thirty. It used to be thin spaces pushed between the letters, which measures
as text, so it broke kerning, broke truncation, and threw every centred label
off by half a space. Riot's own display type is tracked by about a hundredth
of an em; small caps need much more, large caps need almost none, and one
constant cannot be both.

**Space.** A four point grid: `SM 4`, `MD 8`, `LG 12`, `XL 16`, `XXL 24`. A
row is `ROW 24`, body type doubled and rounded onto the grid. It was 26 for
an afternoon, which is body doubled exactly, and the grid test caught it: a
rule with an exception in its first week is not a rule. The board keeps an
18 point gutter down both sides, which is wider than the grid on purpose:
the party bracket lives in it.

**Colour by role, never by name.** Four surfaces rather than two: `void`,
`bg`, `bg_raised`, `bg_inset`. A flat interface is not calm, it is
undesigned; without a tone difference nothing can sit on anything and every
edge has to be a line.

The three that matter come from Riot's own shipped CSS, where they appear in
the ratio 73 : 41 : 41 — one dark ground `#0F1923`, cream type `#ECE8E1`,
and red `#FF4655` as punctuation and nothing else. Rank colours are the
`color` field of every tier in `valorant-api.com/v1/competitivetiers`, byte
for byte, because that encoding is the one a player already knows by heart.

`you` is bone, not red: in game your own row is the light one and the enemy
is red, and an app beside the game that swaps those is actively misleading.

Most numbers are `neutral`. Colour spent on an ordinary value is colour taken
from the one that matters, so a win rate is only tinted past 57 or under 43,
and a level only when it is low enough to be a tell.

**Surfaces are lit, not filled.** Nothing large is one flat colour. The
title bar runs from `bg_inset` at the top to `bg_raised` at the bottom and
drops a soft edge onto the board; the board's ground falls from `bg` to
`void`, which is what stops the half of the window the roster does not reach
from being the largest area of undesigned colour in the app; the panel runs
the other way. A team band is a wash of the team's colour that runs out
before the averages, so the block has a direction. A row's hover and
selection tints run left to right, away from the rail, so they look like
they came from it.

A wash on its own was not enough, and for a long time this app looked
printed. What makes a surface look like a surface is the edge where the
light lands on it, so `shape::lit` is the wash plus one bright hairline
along the top and one dark one along the bottom: two strokes, and the whole
difference between a coloured rectangle and a thing sitting on top of
something else. Every raised thing gets it, which is why it is one call
rather than a habit each screen has to remember.

**Blocks sit on something, rows are cut into it.** Each team, and the
ladder and session together, is one surface with a shadow under it, punched
into the paint list before the block is drawn and filled in once its height
is known. Ten rows on the window's own background is a list; the same ten on
a raised surface is a board. The separator between rows is then a groove, one
stroke of shadow and one of light, because the old single hairline was darker
than the page but lighter than the surface the rows had just been given, and
it vanished the moment they got one.

**A glow is for a thing that happens, not for decoration.** `shape::halo`
goes on the marks meant to catch the eye from across the window and nowhere
else: the flag on an account worth looking at, the rail of the row under the
pointer, and a score digit for as long as the animator takes to catch up
with a round that was just won. A glow on everything is a glow on nothing.

All of it costs the same triangles the flat version costs: the colour is per
vertex and the tessellator interpolates it for nothing. `shape::cut_wash`
does the chamfered ones, `shape::lit` the raised ones,
`Shape::gradient_rect` the square ones, and `shape::drop_shadow` is three
stacked blurred rects because epaint's blur is an oversized feather with a
linear falloff and one of them alone has a hard edge.

**Shape.** One motif: a corner cut at forty five degrees, on two diagonally
opposite corners, seven points deep. It is the game's own language, it costs
four points of a rectangle, and only things that can be acted on or that
group other things get one. Plus the tick: two points wide and a cap tall,
before a section heading, so the eye has somewhere to start.

**Motion.** `INSTANT 150ms` for a hover tint, `QUICK 200ms` for a selection
or a colour, `ARRIVE 300ms` with `STAGGER 28ms` for a roster landing, and
`MEASURE 700ms` for a bar whose length is a number. These are the durations
Riot's own interfaces ship: their working range is 150 to 250 milliseconds,
two hundred decelerating is the default, and nothing in their bundles
bounces, overshoots or springs. Everything here decelerates; nothing else
gets a duration.

What moves: opacity, position, a bar's length, a tint. What does not: type
size, tracking, corner geometry, or any number that updates on its own. A
value that changes every second must never animate.

## Rules that keep it coherent

- **A column is a spec, not a guess.** Width, alignment and heading live in
  one table; the header row and the data row read from the same entry. This is
  what stops a header drifting off its own numbers.
- **Caps for labels, sentence case for prose.** Column headings, section
  labels and buttons are caps with letterspacing. Anything that is a sentence
  is written like one, including empty states and errors.
- **An empty screen says what is happening and what to do.** "Looking for the
  backend" over a spinner, every time.
- **Two tiers, one layout.** Efficient removes shaders, blur and shadows and
  shortens every duration to 150ms or less; it never removes information or
  moves anything. A person switching tiers should see the same app, calmer.
- **Density by width, not by preference.** A narrow board keeps name, rank
  and K/D and drops the rest by column priority, the same priority list the
  terminal front end uses. The fill is monotone: switching a column off can
  only ever add columns, never swap one for another, which it used to do.
- **A mark beats a word.** Four things on a row are shapes rather than text,
  because four different channels can be read at once and four columns of
  words cannot: the agent as a tile in the agent's own colour, the rank as
  chevrons on a plate in the tier's colour, a party as a bracket down the
  gutter, the last five results as pips. Everything else is tabular support.
- **Nothing sizes itself by arithmetic.** The overlay asks for exactly the
  height the board drew last frame. Adding the pieces up by hand means the
  sum has to be revisited whenever a band grows a point, and nobody revisits
  a sum: that is twice it clipped its own last player.
- **A mark that raises a question answers it on the next line.** The flag at
  the end of a row says "look at this one" and nothing else. The reasons sit
  under the row that raised the question, in the colour of the mark, cut at a
  word when the width runs out. Putting them in the detail panel instead cost
  five hovers to read five of them, and the overlay has no panel at all.
- **A scale beats ten readings of it.** The ladder under the roster is every
  account in the lobby on Riot's own tier colours, enemies above it and
  allies below, each carrying the same agent tile they have out on their row.
  It is the only block in the window that says nothing the rows do not
  already say, which is why it is also the first one dropped when the window
  is short.
- **One control, drawn once.** The window and the installer ask the same
  question in the same shape, so there is one `choice` in the design crate
  and no copy in either. Two copies had already drifted: one lit the chosen
  row and one did not.
- **Stillness is a feature on the screen nobody is looking at.** The empty
  state names the three things that have to happen before there is anything
  to show and which of them have, because from in here all three failures
  look identical. None of it pulses. A pulse would wake the compositor every
  frame for as long as somebody sits in the game's menus, which is the one
  cost this app has promised not to have.
- **A guess presented as a fact is a lie.** A party Riot confirmed gets a
  bracket; a stack the app inferred gets a question mark and its evidence in
  the panel. An account the backend cannot see says "not visible" rather than
  drawing ten columns of dashes.

## What it costs

Measured on the release build against a live backend, on the integrated
adapter the window deliberately asks for, with the window at 2264 by 1464
device pixels:

| | |
| --- | --- |
| Idle | 0.2% of a core |
| Pointer swept down the roster as fast as it moves | under 0.2% of a core |
| Committed | 445 MB |
| Resident | 390 to 460 MB, or 16 MB once Windows has trimmed it |

Take the two memory figures together or neither. The resident number is not
reproducible on its own: six samples across one run came out 391, 13, 16, 16,
16 and 17 megabytes, because Windows trims the working set of a window
nobody is touching and pages it straight back the moment anything is. The
committed figure was 444.6 in every one of those samples and in every run.
Almost none of it is this app's own data, which is one board of ten players
and a career of a few dozen matches; the rest is wgpu and the adapter's
driver, and it moves when they do rather than when this window does.

Idle is near zero because the window is reactive: nothing asks for a frame
unless something happened. The bridge thread wakes it when a board arrives, egui's
own animator asks while a tint is in flight and stops when it settles, and
the two things painted from a timestamp rather than from the animator — the
roster landing and the hover dwell — ask for exactly as long as they run.

The loudest complaint about the overlay this app is an alternative to is
that it drops frames in game. That is the number that matters most, and it
is the reason `perf.rs` holds a shape budget and an assertion that a still
window asks for nothing.

## How to know it worked

`crates/overseer-app/src/shot.rs` renders the real UI offscreen through wgpu
and compares it to a PNG in `tests/snapshots`. After any visual change:

```console
UPDATE_SNAPSHOTS=1 cargo test --workspace
```

then **open the PNG and look at it** before committing. The test protects
against accidental change; only a person can say whether the deliberate change
was any good. Look at it at full size, region by region, and list what is
wrong before deciding it is finished.

## What "generated" looks like here, so it can be avoided

- A new colour mixed on the spot because none of the roles felt right. The
  answer is to pick the closest role, or to argue for a new one in
  `design.rs` where everyone can see it.
- Spacing of 5, 7, 13. If a gap is not on the four point grid, it is a guess.
- Every panel getting the same border, the same heading and the same padding
  whether or not it is the same kind of thing.
- Motion on load, on hover, on scroll and on selection, all at once, all
  200ms, all ease-in-out.
- Labels that name the system: "smurfReasons", "puuid", "K/D over N". Write
  what the player would say: "Worth a look", "Met 4 times before".
