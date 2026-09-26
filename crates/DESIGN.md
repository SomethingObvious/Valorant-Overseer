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

**Type.** Three faces, by role:

| Role | Face | Why |
| --- | --- | --- |
| Display, labels | Bebas Neue, shipped in the binary | Tall, condensed, caps only. VALORANT's own display face is Tungsten Bold, always uppercase, and Windows has nothing like it: Bahnschrift is a variable font and this rasteriser can only take its default instance, which is the flattest, widest cut in the family. Sixty kilobytes buys a face that looks the same on every machine and looks like something. |
| Body, names | Segoe UI | The system's reading face; it disappears, which is what a name wants. |
| Numbers | Cascadia Mono, Consolas behind it | Tabular by construction, so a column of K/D cannot drift. Cascadia first because its figures are rounder and its zero is slashed. |

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
- **Density by width, not by preference.** Under 520 points the board keeps
  name, rank and K/D and drops the rest by column priority. The same priority
  list the terminal front end uses.

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
