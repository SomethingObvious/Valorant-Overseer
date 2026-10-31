# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Ranked VALORANT players on Windows who already look players up on tracker
sites between games. The site has to show them that Overseer puts all of that
in front of them in the lobby and in agent select, before the round starts,
with no alt-tabbing to a website.

## Product Purpose

Valorant Overseer is a free Windows app that shows every player in your match
with their rank, peak rank, K/D, headshot and win rates, level, recent form,
who queued together and whether an account looks like a smurf. The GitHub
Pages site at `docs/index.html` gets players to download it from the latest
GitHub release.

## Positioning

It reads the match from the Riot Client's own local API on your PC, so the
whole lobby is filled in before the round starts, and it runs entirely on the
player's machine with no account, no server of its own and no tracking.

## Operating Context

A player uses it in the minute or two between queue pop and the first round:
the lobby, agent select (where the overlay sits over the game and
Ctrl+Alt+O toggles it), the match itself, the match history after a game and
the lineup planner between games. The page follows that order, as one match
from lobby to after the game.

## Capabilities and Constraints

- Board with every player's rank, peak and the act it was reached, K/D,
  headshot rate, win rate, level, RR and last five games, the enemy team on top.
- Player panel with form, mains, a rating line over ten games, frequent
  teammates and reads such as Worth a Look (smurf signs).
- Party tabs for duos and trios, a rank ladder for the lobby, session RR.
- Agent select overlay, always on top, never takes focus, Ctrl+Alt+O.
- Match history with full scoreboards (ADR, headshot rate, KAST, first bloods).
- Lineup planner for every map with clips from YouTube or local video, trimmed
  in the app, shareable as a code.
- Appear Offline in Riot chat, which restarts the Riot Client and VALORANT.
- Never reads game memory, never clicks, types or locks in, never updates
  itself. Windows 10 and 11 only. Version 2.3.1.
- About 175 MB for the window, nothing redrawn while idle, integrated GPU
  preferred so the game keeps the discrete one.

## Brand Commitments

The name is Valorant Overseer, Overseer for short, and VALORANT in capitals
when it means the game. The mark is `docs/assets/mark.svg`. The Riot Games
disclaimer in the footer stays verbatim. Copy is plain and concrete, with
Canadian spelling, title case headings and no marketing words.

## Evidence on Hand

Screenshots from the app's demo mode with made-up players, in
`docs/assets/shots/`: the board, the player panel, parties, the overlay,
match history, lineups and the Appear Offline setting. The licence is
PolyForm Strict 1.0.0. There are no testimonials, user counts, reviews or
press, and the site must not invent any.

## Product Principles

- Show the real app. Every claim on the page points at a screenshot or a fact
  in the README.
- Say what it never does as plainly as what it does.
- Free and local is a fact to state, not a slogan.
