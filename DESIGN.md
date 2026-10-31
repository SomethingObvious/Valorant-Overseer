---
name: Valorant Overseer
description: The match programme for your next VALORANT game, printed in two inks on cover stock and white paper.
colors:
  ultramarine: "#1A2BA8"
  ultramarine-deep: "#111C78"
  chrome-yellow: "#FFD100"
  chrome-yellow-lit: "#FFE04D"
  paper: "#FFFFFF"
  ink: "#0D1033"
  cover-blue-soft: "#C9CFF4"
  slate-ink: "#3C4270"
  plate-black: "#0A0B0E"
typography:
  masthead:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "clamp(5.5rem, 26.5vw, 30rem)"
    fontWeight: 900
    lineHeight: 0.86
    letterSpacing: "-0.01em"
    fontVariation: "'wdth' 62"
  clock:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "clamp(2.6rem, 5vw, 4.4rem)"
    fontWeight: 900
    lineHeight: 0.9
    fontVariation: "'wdth' 62"
  headline:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "clamp(1.9rem, 4.4vw, 3.6rem)"
    fontWeight: 800
    lineHeight: 1.02
    letterSpacing: "0"
    fontVariation: "'wdth' 125"
  subhead:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "clamp(1.4rem, 2.4vw, 2rem)"
    fontWeight: 800
    lineHeight: 1.02
    letterSpacing: "0"
    fontVariation: "'wdth' 125"
  deck:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "clamp(1.7rem, 3vw, 2.6rem)"
    fontWeight: 800
    lineHeight: 1.02
    fontVariation: "'wdth' 75"
  figure:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "22px"
    fontWeight: 900
    lineHeight: 1
    letterSpacing: "0.01em"
    fontVariation: "'wdth' 75"
  body:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.6
    fontFeature: "'tnum'"
  caption:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  label:
    fontFamily: "Archivo, Arial Narrow, sans-serif"
    fontSize: "11px"
    fontWeight: 700
    letterSpacing: "0.06em"
    fontVariation: "'wdth' 125"
rounded:
  none: "0"
spacing:
  gutter: "clamp(16px, 4vw, 64px)"
  spread: "clamp(48px, 7vw, 104px)"
  margin-gap: "clamp(16px, 3vw, 48px)"
  head-after: "clamp(28px, 4vw, 56px)"
  row: "18px"
components:
  ticket:
    backgroundColor: "{colors.chrome-yellow}"
    textColor: "{colors.ink}"
    typography: "{typography.figure}"
    rounded: "{rounded.none}"
    padding: "0 24px"
    height: "54px"
  ticket-hover:
    backgroundColor: "{colors.chrome-yellow-lit}"
  ticket-small:
    backgroundColor: "{colors.chrome-yellow}"
    textColor: "{colors.ink}"
    padding: "0 14px"
    height: "34px"
  ticket-big:
    backgroundColor: "{colors.chrome-yellow}"
    textColor: "{colors.ink}"
    padding: "0 26px"
    height: "72px"
  running-head:
    backgroundColor: "{colors.ultramarine-deep}"
    textColor: "{colors.paper}"
    padding: "0 {spacing.gutter}"
    height: "56px"
  clock:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.chrome-yellow}"
    padding: "0 12px"
    height: "34px"
  flag:
    backgroundColor: "{colors.chrome-yellow}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "3px 7px 2px"
  party:
    textColor: "{colors.cover-blue-soft}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "2px 6px 1px"
  plate-frame:
    backgroundColor: "{colors.plate-black}"
    rounded: "{rounded.none}"
---

# Design System: Valorant Overseer

## Overview

Creative North Star: **"The Match Programme"**

The site is printed like the programme you would buy at the gate. Saturated ultramarine cover stock owns about half the page, printed in white and one chrome yellow second ink. Inner spreads are white paper in navy ink and ultramarine. Spreads run in the order a match happens, and each one prints its match clock time in the left margin.

Everything is flat print. Stocks change at full bleed, never as floating cards. Depth comes from the stock change, thick and thin printer's rules and the black ground behind each screenshot plate. One face, Archivo, does all the work at its full width range, from an ultra condensed masthead to expanded section heads.

It refuses the dark gamer landing with a neon accent, feature cards and a glowing hero screenshot.

**Key Characteristics:**

- Two inks (ultramarine and chrome yellow) on two stocks (ultramarine and white), with yellow used once as a third stock.
- One variable face at three widths, tabular figures everywhere.
- Square corners, no shadows, no gradients that read as gradients.
- Screenshots are numbered plates with captions.
- A running match clock in the sticky head.

## Colors

The palette is a printer's, with two inks, two stocks and one notice in yellow.

### Primary

- **Ultramarine Cover Stock** (#1A2BA8): the cover and every second spread. Type on it is white paper. Also the ink for margin clock times and focus rings on white spreads.
- **Ultramarine Deep** (#111C78): the sticky running head and the scrollbar track. It sits one step darker so the head reads as a separate band over the cover.

### Secondary

- **Chrome Yellow Second Ink** (#FFD100): the download ticket, clock times on blue, row numbers, step numbers, the Worth a Look flag, table captions and text selection. Also the focus ring on blue.
- **Chrome Yellow Lit** (#FFE04D): the ticket's hover state only.

### Neutral

- **Paper** (#FFFFFF): pure white inner spreads, and the white type on blue.
- **Programme Ink** (#0D1033): body type on paper and yellow, the clock box, the footer stock.
- **Pale Cover Blue** (#C9CFF4): secondary text on blue, such as ledes, captions, table heads and nav links.
- **Slate Ink** (#3C4270): secondary text on paper, mostly plate captions.
- **Plate Black** (#0A0B0E): the ground inside every screenshot frame, so dark app captures sit on a matching black.

### Named Rules

**The Two Stock Rule.** Spreads alternate ultramarine and white at full bleed. A spread is one continuous stock from rule to rule.

**The Notice Rule.** Yellow is printed as a stock for exactly one spread, Ground Rules, which reads as a posted notice. No other spread gets a yellow ground.

**The Second Ink Rule.** Yellow carries figures and the one action. It never sets running text on white, where it can't be read.

## Typography

**Display Font:** Archivo variable (wdth 62 to 125, wght 100 to 900), with Arial Narrow as fallback
**Body Font:** Archivo at normal width

**Character:** One grotesque stretched to its limits. Condensed black for the shout and the figures, expanded caps for heads and labels, plain regular for reading.

### Hierarchy

- **Masthead** (900, wdth 62, clamp(5.5rem, 26.5vw, 30rem), 0.86): OVERSEER on the cover only, set across the full width and cropped at the right edge.
- **Clock** (900, wdth 62, clamp(2.6rem, 5vw, 4.4rem), 0.9): each spread's match time in the margin, yellow on blue and ultramarine on white.
- **Headline** (800, wdth 125, clamp(1.9rem, 4.4vw, 3.6rem), 1.02, uppercase): spread heads (h2).
- **Subhead** (800, wdth 125, clamp(1.4rem, 2.4vw, 2rem), 1.02, uppercase): sub heads inside a spread (h3).
- **Deck** (800, wdth 75, clamp(1.7rem, 3vw, 2.6rem), uppercase): the one line under the masthead.
- **Figure** (900, wdth 70 to 75, 22 to 26px, uppercase): ticket text, player names, K/D and the running clock. List counters and row numbers drop to wdth 62.
- **Body** (400, 17px, 1.6, 16px under 720px): running text held to 62ch.
- **Caption** (400, 14px, 1.45): plate captions up to 70ch.
- **Label** (700, wdth 125, 11 to 12px, 0.06em, uppercase): plate numbers, phases, nav, ticket stubs, table heads and tags.

### Named Rules

**The One Shout Rule.** Wdth 62 at weight 900 belongs to the masthead, clock times, numerals and player names. Section heads are expanded, so the masthead stays the only condensed shout on the page.

**The Tabular Rule.** The body sets tabular figures, so every number in a column lines up figure for figure.

## Layout

The page gutter is clamp(16px, 4vw, 64px) and spreads pad clamp(48px, 7vw, 104px) top and bottom. Each spread opens with a printer's rule, then a head grid of 2fr margin and 10fr heading. Body content sits in the same 2fr and 10fr columns, with the margin column left empty. Inside the main column, text and plate split 5 to 7, 4 to 6 or 4 to 8. Text measures cap at 62ch.

The left margin carries only the match clock time and its phase, or nothing. Spreads past full time (Ground Rules, Admission Free, Questions) leave it empty.

The running head is sticky at 56px. It drops the second issue label under 1600px and the nav under 1360px. Under 1000px every split goes to one column and the margin time sits inline beside its phase. Under 720px the brand name and issue hide, and wide plates pan sideways at a fixed width instead of shrinking. Under 400px the clock phase and the small ticket's stub hide.

## Elevation & Depth

There are no shadows. Depth comes from stock changes at full bleed, the thick and thin rule (5px bar, 3px gap, 1px hairline) and the plate black behind screenshots. The only motion is the ticket lifting 2px on hover, the FAQ plus turning 45 degrees and the clock counting between times, all on cubic-bezier(0.16, 1, 0.3, 1) and all off under reduced motion.

### Named Rules

**The Flat Print Rule.** Nothing floats. If a surface needs separating, change the stock or draw a rule.

## Shapes

Every corner is square. Borders are 1px ink or white hairlines under rows and around plates. The two shaped forms are the ticket, with half circle perforation notches cut top and bottom where the stub tears off and a dashed 2px line down the tear, and the kbd key, with a 3px bottom border like a keycap edge.

## Components

### Ticket

The one action, a yellow admission ticket.

- Shape: square, with notches masked at the tear line and a dashed stub border.
- Sizes: small (34px, in the running head), regular (54px, on the cover), big (72px, in Admission Free). The text is figure type with a download glyph drawn as inline SVG. The stub carries label type such as Free and the version.
- Hover / Focus: lightens to Chrome Yellow Lit and lifts 2px. Focus is a 3px yellow outline on blue, ultramarine on white.

### Plate

- Frame: 1px border in the current ink around a Plate Black ground.
- Caption: label type plate number (Plate 1 to Plate 7) beside a caption sentence, in Pale Cover Blue on blue and Slate Ink on white.

### Team Sheet

Two stacked tables, enemy team on top as in the app, with a yellow caption over each.

- Columns: fixed table layout with set widths so both teams line up. Row number in condensed yellow, name in figure type with tag and agent below, rank, K/D right aligned in figure type, then HS, Win and Lvl.
- Tags: Worth a Look is a yellow flag with ink text. Duo is a hairline outlined tag in Pale Cover Blue.
- Mobile: under 720px HS, Win and Lvl hide and long names truncate with an ellipsis at 9.5em.

### Running Head and Clock

An Ultramarine Deep sticky band with the mark, issue labels, nav links in label type and the clock box: ink ground, yellow time in figure type and the phase in label type. The clock counts to each spread's time as it scrolls into view.

### Numbered Lists

Ground Rules and the setup steps use large condensed counters (30 to 40px, wdth 62) in a narrow first column, with hairline rules between items.

### Questions

Native details elements between ink hairlines, with a drawn ultramarine plus that turns into a cross when open.

## Do's and Don'ts

### Do

- Do alternate ultramarine and white spreads at full bleed, each opened by the thick and thin rule.
- Do put a spread's match clock time and phase in the left margin, or leave the margin empty.
- Do set every screenshot as a numbered plate on Plate Black with a caption.
- Do keep heads expanded at wdth 125 and weight 800, and keep wdth 62 for the masthead and figures.
- Do make the download a yellow ticket with a stub.

### Don't

- Don't put a kicker or eyebrow line above a heading.
- Don't add rounded corners, drop shadows, cards or visible gradients.
- Don't print a second spread on yellow stock.
- Don't set running text in yellow on white.
- Don't use a dark page with a neon accent or a glowing hero screenshot.
