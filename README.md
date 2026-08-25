# Valorant Overseer

Live in-match VALORANT intelligence, read from the game client on this PC and
rendered in the terminal. Ranks, peaks, parties, K/D, win rate, smurf risk,
skins, RR history, an encounter log across sessions, appear-offline and
Discord Rich Presence. 

Private build. It does not update itself, it does not report anything about you
anywhere, and it answers to nobody but the person at this keyboard.

## What this build will not do

This is the point of it, so it is the first section.

- **It never updates itself.** There is no updater, no version check, no
  release download, no archive that gets unpacked over the install. Nothing
  short of you editing the files changes the code on this machine. A repository
  can be compromised and its releases replaced; a build that cannot update
  itself does not care.
- **It sends no telemetry.** There is no install ID, no heartbeat, no
  "anonymous usage" ping. The upstream build posted your install ID, Riot ID,
  region, current rank and OS to a third-party server every sixty seconds, on
  by default. That code is deleted, not disabled.
- **It accepts no remote control.** The phone/remote-pairing channel is gone.
  Nothing outside this machine can drive the local bridge.
- **It serves no web dashboard by default.** A hosted dashboard is somebody
  else's JavaScript talking to a bridge on this machine. Set `FRONTEND_URL`
  only if you run the page yourself.
- **It never acts on the game.** No instalock, no auto-dodge, no queue
  control. Every route that did anything to a live match is deleted, and the
  command router will not accept the words either. It reads your client and
  renders what it sees; that is the whole of it.

`scripts\lint.ps1` enforces all of that. Two of its checks exist for nothing
else: one fails on any host that is not on a written allowlist, the other fails
on anything that looks like a self-update.

### Who it does talk to

The complete list, all of it read-only or verified:

| Host | Why | Guard |
| --- | --- | --- |
| `*.a.pvp.net`, `*.riotgames.com` | Your own game client and Riot's APIs. The product. |, |
| `valorant-api.com` | Agent portraits, rank icons, skin and map metadata. Nothing about you is sent; nothing it returns is executed. |, |
| `www.python.org` | The CPython installer, once, during setup. | SHA-256 + Authenticode, checked before it runs |
| `mln.cx` | The offline-mode chat certificate. | SHA-256 pinned in `offline_launch.py` |

Anything else fails the lint suite.

## Quick start

Windows 10/11 x64, plus Node.js 20+ on PATH for the scoreboard. The installer
sets up its own pinned CPython 3.12.10; other Pythons on the PC are untouched.

```powershell
install.bat     # once. Installs the runtime and the pinned dependencies.
start.bat       # the scoreboard.
```

Open VALORANT and join a lobby, agent select or a match. **With the game closed
you get a holding screen, not a lobby.** Nothing invented is ever drawn as if it
were real: sample players exist only behind an explicit opt-in.

```powershell
$env:DATA_SOURCE = "demo"   # sample players, clearly marked
```

### Keys

The scoreboard is interactive. Nothing here touches the game; every key moves
the view.

| Key | Does |
| --- | --- |
| `Up` `Down`, `j` `k` | move the selection, the detail panel follows it |
| `Enter` | open the selected player's career |
| `1`–`5`, `Tab` | switch view |
| `d` | detail panel on/off |
| `s` | session panel on/off |
| `,` | settings |
| `Esc` | back to the board, or quit from it |
| `q` | quit |

### Views

| View | Shows |
| --- | --- |
| `1` Board | the live scoreboard |
| `2` Career | the selected player: win rate by agent and by map, their last ten matches with K/D/A, ACS and RR, and how often you have met before |
| `3` Session | ranked record, net RR, average win and loss, RR per match, splits by map and agent, and the insights the backend derives |
| `4` Last match | the full end-of-game scoreboard: K/D/A, ACS and headshot rate for all ten, MVP, and your RR change |
| `5` Seen before | every account you have run into again, and your record with them |

Only the board is live. The other four fetch when you open them and not before,
so a view you never open costs no Riot requests, which is the whole reason the
request volume stays where [What this build will not do](#what-this-build-will-not-do)
says it does.

Settings live in `.overseer/tui.json` and cover density, which panels show,
whether the enemy team appears, and the refresh interval. None of them changes
what is fetched.

Columns drop as the terminal narrows, lowest priority first: `LVL` goes before
`AGENT`, and name, rank and K/D never go. Below about 108 columns the detail
panel steps aside for the table.

`node tui\dist\overseer.js --self-check` renders every board shape a missing
Riot field can produce and checks the column fitter, with no game and no
network; `lint.ps1` runs it, and found a real crash the first time it did.

### Working on the scoreboard

The scoreboard is TypeScript on [Ink](https://github.com/vadimdemedes/ink),
bundled to one file so a user needs node and nothing else, no `npm install` on
their machine, no `node_modules` in the release.

```powershell
cd tui
npm install
npm run build              # -> tui\dist\overseer.js
node preview.mjs --list    # every screen the app can show, by name
node preview.mjs ingame    # draw one
node preview.mjs           # draw all of them
```

The scoreboard's output is a picture, so the screens are named stories in
`tui/src/stories.ts` -- in game, agent select, lobby, the holding screen, hidden
and unranked accounts, three terminal widths, settings. `preview.mjs` renders
them into a fake terminal with no TTY, which is how a layout change gets looked
at before it ships. `lint.ps1` renders all of them and fails if any comes back
empty.

`lint.ps1` rebuilds the bundle and fails if a single byte differs from the
committed one, so the shipped bundle is always exactly what `tui/src` and
`package-lock.json` produce.

## Configuration

`backend\.env`, all optional:

| Key | Default | Notes |
| --- | --- | --- |
| `RIOT_REGION` | asked at install | `na`, `eu`, `ap`, `kr`, `latam`, `br` |
| `DATA_SOURCE` | `auto` | `demo` shows sample players instead of waiting |
| `FRONTEND_URL` | unset | A dashboard you host yourself. Unset means terminal only. |
| `BACKEND_PORT` / `WS_PORT` | `5000` / `7878` | |

The local API needs a per-launch token, published in `.overseer/bridge.json`.
Only `/api/health` is open, so that the launcher can wait on it.

## Working on it

```powershell
scripts\setup.ps1     # once: hooks, merge policy, tool check
scripts\lint.ps1      # 25 checks, all fatal. Must pass before you push.
scripts\lint.ps1 -Fix # format and autofix first
```

`AGENTS.md` has the rules and the reasons behind them.

## Notice

Not affiliated with or endorsed by Riot Games. Reads only your own local client;
nothing is injected into the game.
