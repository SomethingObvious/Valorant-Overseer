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

**Type.** Three faces, by role, all present on Windows 11, with egui's bundled
font as the fallback:

| Role | Face | Why |
| --- | --- | --- |
| Display, labels | Bahnschrift | A DIN, which is the family VALORANT's own interface uses. Caps and letterspaced for labels. |
| Body, names | Segoe UI | The system's reading face; it disappears, which is what a name wants. |
| Numbers | Consolas | Tabular by construction, so a column of K/D cannot drift. |

Sizes, in points, rounded to whole pixels because glyphs are rasterised:
`DISPLAY 20`, `TITLE 15`, `BODY 13`, `LABEL 11`, `MICRO 10`.

**Space.** A four point grid: `XS 2`, `SM 4`, `MD 8`, `LG 12`, `XL 16`,
`XXL 24`. A row is `ROW 24`, body type doubled and rounded onto the grid, so
rows stack on the same rhythm as everything else. It was 26 for an afternoon,
which is body doubled exactly, and the grid test caught it: a rule with an
exception in its first week is not a rule.

**Colour by role, never by name.** `bg`, `bg_raised`, `line`, `line_soft`,
`text`, `text_strong`, `text_dim`, `text_faint`, `ally`, `enemy`, `you`,
`good`, `info`, `warn`, `bad`. Rank colours come from the tier, one per group,
the same values the terminal app uses.

`you` is bone, not red: in game your own row is the light one and the enemy is
red, and an app beside the game that swaps those is actively misleading.

**Motion.** `INSTANT 60ms` for a hover tint, `QUICK 120ms` for a selection,
`MOVE 180ms` for something travelling across the screen, `STAGGER 40ms`
between siblings, capped at ten. Ease out for arrivals, linear for progress.
Nothing else gets a duration.

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
