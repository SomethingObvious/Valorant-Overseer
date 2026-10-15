# Valorant Overseer

![Valorant Overseer showing an enemy team's ranks, peak ranks, K/D and parties in a VALORANT match](docs/banner.svg)

Valorant Overseer is a match tracker for VALORANT on Windows that runs entirely
on your PC. It shows everyone in your lobby and your match with their rank,
peak rank, K/D and win rate, who queued together, and whether an account looks
like a smurf. It also keeps your match history, has a lineup planner with clips
for every map, puts an overlay over the game, and can hide you from your
friends in Riot chat.

**[Download Valorant Overseer for Windows](https://github.com/SomethingObvious/Valorant-Overseer/releases/latest)**

There's more on the [Valorant Overseer site](https://somethingobvious.github.io/Valorant-Overseer/).

## Screenshots

These come from the app's demo mode, so the players are made up.

![The Overseer board in a VALORANT match, with each player's rank, peak rank and K/D](docs/assets/shots/board-1600.webp)

![Overseer in agent select, with your team's ranks, a duo and the rank ladder](docs/assets/shots/pregame-1600.webp)

![The Overseer overlay over VALORANT in agent select](docs/assets/shots/overlay-full.webp)

![Overseer's match history with a game opened to its scoreboard](docs/assets/shots/history-1600.webp)

![Overseer's lineup planner on Haven](docs/assets/shots/lineups-1600.webp)

## How It Works

Overseer reads your match from the Riot Client's own local API, the same one
the client uses to show your party, and asks Riot's servers for the recent
games of the players in it. It never touches the game itself: it doesn't read
the game's memory, and it never clicks, types or locks in an agent.

There's no account, no server of its own and no tracking. Agent and map art
comes from valorant-api.com, and nothing about you is sent there. Once a day it
asks GitHub which version is newest, in one small request with nothing about
you in it, and Look for Updates in settings turns that off. It never downloads
or installs anything by itself. Your notes, lineups and history stay in the
install folder, and Your Data in settings opens it.

## Installing

Overseer runs on Windows 10 and 11. Download `Valorant-Overseer-Setup.exe`,
open it and pick your region. It installs into
`AppData\Local\Programs\Valorant Overseer` with its own copy of Python and
doesn't need admin rights. If Windows says it protected your PC,
click More info, then Run anyway.

Then open VALORANT and join a lobby. It uninstalls from Settings > Apps.

When a new version is out, the footer says so, and its setup updates in place.
It keeps your region, lineups, notes and settings, and running it again
repairs an install.

Lineup clips need yt-dlp and FFmpeg. Setup installs them with winget, and
Update Clip Tools in settings updates them when a link stops downloading.

## Notice

Valorant Overseer isn't made or endorsed by Riot Games.

Working on the code? `crates/README.md` covers that.

## Licence

Overseer is under the [PolyForm Strict License 1.0.0](LICENSE.md). You can use
it for anything noncommercial, but you can't sell it, use it in a business,
share copies of it, or publish a changed version of it.
