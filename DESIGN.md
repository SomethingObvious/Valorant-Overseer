---
name: Valorant Overseer
description: A printed survey sheet of your next VALORANT match, with contours, a grid and a key that is the route down the page.
colors:
  water: "#1F6AA5"
  water-deep: "#174F7C"
  contour: "#B4532A"
  grid: "#C2185B"
  gridline: "rgba(194, 24, 91, 0.26)"
  paper: "#F6F9EF"
  ground: "#D7E6C3"
  ink: "#161616"
  ink-soft: "#3D4A33"
  white: "#FFFFFF"
  frame-black: "#0A0B0E"
  board-enemy: "#E8434F"
  board-team: "#2BB3A3"
  board-flag: "#FFC845"
  board-party: "#8E5BD8"
typography:
  display:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "clamp(4.2rem, 8.6vw, 6rem)"
    fontWeight: 900
    lineHeight: 0.84
    letterSpacing: "-0.035em"
  headline:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "clamp(2.1rem, 4.2vw, 3.7rem)"
    fontWeight: 900
    lineHeight: 0.98
    letterSpacing: "-0.015em"
  deck:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "clamp(1.25rem, 1.9vw, 1.65rem)"
    fontWeight: 800
    lineHeight: 1.12
  subhead:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "1.25rem"
    fontWeight: 900
    lineHeight: 1.15
    letterSpacing: "0.01em"
  box-head:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "16px"
    fontWeight: 900
    lineHeight: 1
    letterSpacing: "0.16em"
  action:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "19px"
    fontWeight: 900
    lineHeight: 1.05
    letterSpacing: "0.01em"
  body:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.6
  caption:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  item:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "15px"
    fontWeight: 800
    lineHeight: 1.2
  note:
    fontFamily: "Overpass, Segoe UI, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.3
  figure:
    fontFamily: "Overpass Mono, Cascadia Mono, monospace"
    fontSize: "14px"
    fontWeight: 700
    lineHeight: 1.4
    fontFeature: "'tnum'"
  label:
    fontFamily: "Overpass Mono, Cascadia Mono, monospace"
    fontSize: "11px"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "0.08em"
rounded:
  none: "0"
  round: "50%"
spacing:
  band: "30px"
  inset: "clamp(10px, 1.1vw, 18px)"
  inset-pad: "clamp(18px, 1.8vw, 26px)"
  row-top: "clamp(40px, 5vw, 72px)"
  row-bottom: "clamp(48px, 6vw, 88px)"
  head-after: "clamp(24px, 3vw, 40px)"
  stack: "clamp(20px, 2.4vw, 32px)"
components:
  download:
    backgroundColor: "{colors.water}"
    textColor: "{colors.white}"
    typography: "{typography.action}"
    rounded: "{rounded.none}"
    padding: "16px 18px"
  download-hover:
    backgroundColor: "{colors.water-deep}"
    textColor: "{colors.white}"
  download-big:
    backgroundColor: "{colors.water}"
    textColor: "{colors.white}"
    padding: "22px"
  download-strip:
    backgroundColor: "{colors.water}"
    textColor: "{colors.white}"
    padding: "9px 14px"
  inset:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    padding: "{spacing.inset-pad}"
  heading-label:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    typography: "{typography.headline}"
    rounded: "{rounded.none}"
    padding: "0.14em 0.32em 0.04em"
  frame:
    backgroundColor: "{colors.frame-black}"
    rounded: "{rounded.none}"
  figure-caption:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    typography: "{typography.caption}"
    padding: "9px 14px 8px"
  route-pin:
    backgroundColor: "{colors.paper}"
    rounded: "{rounded.round}"
    size: "36px"
    padding: "6px"
  row-mark:
    backgroundColor: "{colors.paper}"
    rounded: "{rounded.round}"
    size: "64px"
    padding: "12px"
  strip:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    height: "56px"
    padding: "0 24px"
  key-cap:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    padding: "3px 6px 2px"
---

# Design System: Valorant Overseer

## Overview

Creative North Star: **"The Survey Sheet"**

The page is printed the way a survey sheet is, the map you unfold to see the whole ground before you set out. Pale green ground carries real contour lines in burnt sienna, a faint magenta grid runs across the sheet in eight columns lettered A to H, and every section is a numbered row of that grid. Water blue is the one colour you act on. Everything else is ink on paper.

The sheet is flat and ruled. A hairline outer border and a heavy 3px neat line hold the map, with a margin band between them for the grid letters and row numbers. Text never sits on the relief. It sits on cleared paper insets with a 1px ink border, the way a map knocks out the ground under a label. Screenshots of the app sit in black map frames under the grid, numbered as figures. One face family does all the type: Overpass in black uppercase for the title and heads, Overpass Mono for numbers, references and labels.

It refuses the dark gamer landing with a neon accent, feature cards and a glowing hero screenshot.

**Key Characteristics:**

- Green ground, sienna contours, magenta grid, blue water, on paper, in black ink.
- An eight column grid, lettered in the top and bottom margins and numbered by row in both side margins.
- Text on cleared paper insets, square and outlined in ink.
- Numbered figures in black frames with paper captions.
- A Key that is the route down the page, with one map symbol per stop.

## Colors

The palette is a survey printer's four inks on two papers, plus the app's own board colours where the page shows the board.

### Primary

- **Water Blue** (water): the download, every link, the FAQ plus, text selection and the text caret. It is the only colour that means act here.
- **Deep Water** (water-deep): the hover state of links and the download, and nothing else.

### Secondary

- **Burnt Sienna Contour** (contour): the contour lines in the terrain, the dashed footpath that joins the Key's stops, the dashed boundary round Ground Rules, the form line in the board key, several route symbols and the scrollbar thumb. Relief and routes, never text.

### Tertiary

- **Grid Magenta** (grid): the 3px focus ring. As **Gridline** (gridline, the same magenta at 26%) it draws the eight column grid across the neat line and the rule that opens each row. The frames over screenshots draw it at 30%.

### Neutral

- **Survey Paper** (paper): the page, every inset, the title block, the strip and the caption under each figure.
- **Survey Green** (ground): the ground inside the neat line, under the terrain and grid.
- **Ink** (ink): type, the neat line and sheet border, inset borders, rules and solid map symbols.
- **Olive Ink** (ink-soft): secondary text on paper, such as small descriptions, table heads, field labels and the imprint.
- **White** (white): text on water blue and the face of a key cap.
- **Frame Black** (frame-black): the ground in every screenshot frame, so the dark app captures sit on a matching black.

### Board Colours

The app's own board colours, used only in the team sheet and its key so the table reads the same as Fig. 1: **Enemy Red** (board-enemy) and **Team Teal** (board-team) for the team tags, **Flag Yellow** (board-flag) for the Worth a Look triangle with an ink stroke, and **Party Violet** (board-party) for the party tab down a row.

### Named Rules

**The One Water Rule.** Water blue is the only action colour. If something is not a link or the download, it is not blue.

**The Map Inks Rule.** Sienna draws relief and routes, magenta draws the grid and the focus ring. Neither one fills a button or sets running text.

**The Board Colours Rule.** Red, teal, yellow and violet belong to the board. They appear where the page reproduces the app and nowhere else.

## Typography

**Display Font:** Overpass variable (weights 100 to 900), with Segoe UI as fallback
**Body Font:** Overpass
**Label/Mono Font:** Overpass Mono (weights 300 to 700), with Cascadia Mono as fallback

**Character:** A highway signage grotesque at full black for the shout, plain at regular weight for reading, and its monospaced sibling for anything you would read off a ruler.

### Hierarchy

- **Display** (900, clamp(4.2rem, 8.6vw, 6rem), 0.84, uppercase): OVERSEER in the title block only. Under 720px it scales to clamp(3rem, 15vw, 6rem). The first word of the name sits above it in Overpass Mono at 13px, tracked 0.14em, in water blue.
- **Headline** (900, clamp(2.1rem, 4.2vw, 3.7rem), 0.98, uppercase): row heads, set as labels on cleared paper.
- **Deck** (800, clamp(1.25rem, 1.9vw, 1.65rem), 1.12): the one line in the title block, balanced.
- **Subhead** (900, 1.25rem, 1.15, uppercase): heads inside an inset.
- **Box Head** (900, 16px, tracked 0.16em, uppercase): the head of the Key and the Board Key, ruled under with 2px ink. Team captions use the same voice at 15px and 0.12em.
- **Action** (900, 19px, uppercase): the download, 15px in the strip and the deck's size on the big one.
- **Body** (400, 17px, 1.6, 16px under 720px): running text held to 66ch. Strong is weight 750.
- **Caption** (400, 14px, 1.45): figure captions and the board key.
- **Item** (800, 15px, 1.2): stop names in the Key, ranks in the team sheet and the setup note. Team captions and the strip's download take the same size at 900.
- **Note** (400, 13px, 1.3): the line under each stop, agents and peaks under a player, the board key's note and the imprint.
- **Figure** (Overpass Mono 700, 14px, tabular): table numbers, row numbers, step counters and figure numbers. The sheet number in the title block is the large case at 800 and clamp(2rem, 3vw, 2.7rem).
- **Label** (Overpass Mono 700, 11px, uppercase, tracked 0.06em to 0.14em): grid letters, field names, table heads, the Worth a Look tag, the download stub and the strip's location.

### Named Rules

**The Two Voices Rule.** Overpass speaks in words, Overpass Mono speaks in numbers, references and labels. A heading is never mono and a number in a table is never proportional.

**The Black Caps Rule.** Every head is Overpass at 900 in uppercase. Weight carries the hierarchy, so there is no second display face.

## Layout

The sheet sits 12px inside the window with a 1px ink border. A 30px margin band runs inside it on every side, carrying the grid letters A to H across the top and bottom and each row's number in both side margins. Inside the band the neat line, a 3px ink border, holds the map.

Everything inside the neat line is an eight column grid. The title block is a ruled table across it, name in columns A to D, the deck in E and F, the sheet number with north point and scale bar in G and H. The hero puts the board figure in A to F and the side column (lede, download, Key) in G and H, shifting to A to E and F to H under 1280px. Each row after that opens with a gridline rule, puts its map symbol in column A, sticky 76px from the top, and its head and content in B to H, split per row such as B to D beside E to H. Columns are inset from their grid lines by clamp(10px, 1.1vw, 18px). Rows pad clamp(40px, 5vw, 72px) above and clamp(48px, 6vw, 88px) below.

Under 1000px the margin band narrows to 16px, the outer border drops, and every grid goes to one column. The row symbol moves inline above the head at 48px and its phase label hides. Under 720px the strip drops the brand and the download stub, the table hides HS, Win and Lvl, and wide screenshots keep a fixed width (760px or 1000px) and pan sideways instead of shrinking.

**The Eight Columns Rule.** Every block starts and ends on one of the sheet's eight grid lines. Nothing floats between them.

## Elevation & Depth

There are no shadows. Depth is the map's own: ground under paper, with the contours and grid below every inset and label. The heavy neat line, 3px rules under the title block and strip, and 2px rules under box heads and table heads do the separating. Screenshot frames carry a 2px ink border.

Motion is one curve, cubic-bezier(0.16, 1, 0.3, 1). On load the Key's footpath draws down over 1.6s and its pins pop in one after another, 0.15s apart. The strip slides in over 0.4s once the title block has scrolled away, the download darkens over 0.2s and the FAQ plus turns 45 degrees over 0.3s. All of it stops under reduced motion.

### Named Rules

**The Cleared Ground Rule.** Text that sits on the map gets cleared paper under it, an inset or a label with a 1px ink border. Type never prints straight onto the contours.

**The Flat Sheet Rule.** Nothing lifts. Separation is a rule, a border or a change from ground to paper.

## Shapes

Rectangles are square and outlined: 1px ink for insets, captions and heading labels, 2px for frames and the download, 3px for the neat line and the strip's bottom edge. Circles belong to map symbols only, which means the route pins, the row marks and the setup step counters, each a paper disc with an ink ring. Dashes mean a path or a boundary, 2px sienna for the footpath and Ground Rules' outline, set 9px inside its inset. The key cap's 3px bottom border is the one deliberate thickening, drawn like a keyboard key.

## Components

### Download

The one action, a water blue bar with an ink border.

- **Shape:** square, 2px ink border.
- **Content:** a drawn download arrow and the action text on the left, a Label stub on the right with Free and the version.
- **Sizes:** regular in the hero, big in Setup, compact in the strip with the stub hidden under 720px.
- **Hover / Focus:** darkens to deep water. It presses 1px on active. Focus is the 3px magenta ring offset 3px, as on every control.

### Inset

- **Corner Style:** square.
- **Background:** paper on the green ground.
- **Shadow Strategy:** no shadow, by the Cleared Ground Rule.
- **Border:** 1px ink.
- **Internal Padding:** clamp(18px, 1.8vw, 26px).

### Key and Route

The page's navigation and its signature. An inset with a Box Head, then an ordered list of eight stops, each a 36px route pin with its map symbol, the stop name, a one line description in Olive Ink and its row number in mono. A dashed sienna footpath joins the pins. Hover turns the name and pin ring water blue. The same eight symbols (viewpoint, settlement, trig point, flag, footpath, boundary, water, information) mark each row in column A and the strip.

### Row Mark

A 64px paper disc in column A with the row's symbol, sticky as the row scrolls, with the phase name knocked out on paper beneath it in uppercase. Rows past the match (Ground Rules, Setup, Questions) carry the symbol alone.

### Figure

- **Frame:** frame black with a 2px ink border and the magenta grid over the image, eight columns and a 120px row.
- **Caption:** a paper strip with a 1px ink border joined under the frame, the figure number (Fig. 1 to Fig. 7) in mono beside a caption sentence.

### Team Sheet

The board reproduced as a gazetteer table on an inset, enemy team on top. A board colour tag leads each caption. Mono heads over a 2px ink rule, rows split by ink at 22%, player name at 800 with tag and agent below, K/D and stats right aligned in tabular mono. Parties get a violet tab down the left of their rows and Worth a Look gets the yellow triangle with a mono label. A Board Key inset beside it explains each mark.

### Strip

The navigation once the title block has gone. A paper bar fixed to the top, 56px with a 3px ink bottom edge, holding the mark and name, then the current row's symbol and "Row n, Phase" in mono, then the compact download at the right.

### Questions

Native details elements between ink hairlines, with a drawn water blue plus that turns into a cross when open. The question turns water blue on hover.

## Do's and Don'ts

### Do

- **Do** keep every surface inside the sheet, within the hairline border, the 30px margin band with grid letters and the 3px neat line.
- **Do** put text on a paper inset or a paper label with a 1px ink border.
- **Do** open each row with a gridline rule, its number in both margins and its map symbol in column A.
- **Do** frame every screenshot on frame black under the magenta grid, numbered as a figure with a caption.
- **Do** set numbers, references and labels in Overpass Mono with tabular figures.
- **Do** keep water blue for the download, links and the controls that open something.

### Don't

- **Don't** round a rectangle. Circles are for map symbols, route pins and step counters only.
- **Don't** add soft shadows or a colour gradient you can see. Gradients only draw hairlines.
- **Don't** put a kicker or eyebrow line above a row head. The small name line over OVERSEER belongs to the title block alone.
- **Don't** use sienna or magenta for a button, or board colours outside the team sheet.
- **Don't** use a dark page with a neon accent, feature cards or a glowing hero screenshot.
