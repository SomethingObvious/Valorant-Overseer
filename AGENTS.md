# Working in This Repository

Valorant Overseer reads the VALORANT client running on this PC and shows the
lobby it finds: ranks, peaks, parties, K/D, smurf risk, skins. The Python
backend in `backend/` does the reading and serves the board over a local
WebSocket bridge, and the Rust window in `crates/` reads it. The setup wizard
is in `crates/` too.

This is a private, hardened build. It doesn't update itself, sends no
telemetry, accepts no remote control and serves no web page. There is no
dashboard and no HTTP API, not even behind a setting, and `Test-NoSelfUpdate`
and `Test-NoNewHosts` in `scripts\lint.ps1` are there to keep it that way.
Read Isolation below before adding anything that opens a socket.

This file only carries what you can't get from the tree. Read the code for
everything else.

## Commands

```powershell
scripts\setup.ps1              # once: hooks, merge policy, tool check
scripts\lint.ps1               # every check there is, and it must pass before you push
scripts\lint.ps1 -Fix          # apply the safe automatic fixes first
scripts\lint.ps1 -Staged       # staged files only, which is what pre-commit runs

install.bat                    # user setup: the wizard, or install.ps1 given arguments or no built wizard
start.bat                      # user launch: checks the install and opens the window
cargo build --release          # in crates\, the window and the wizard, into crates\target\release

$v = (Get-Content VERSION).Trim()
scripts\build-release.ps1 -Version $v -Output dist
scripts\verify-release.ps1 -Zip "dist\overseer-v$v.zip"
```

`build-release.ps1` refuses a `-Version` that doesn't match `VERSION` and
`runtime.json`, which is why the release lines read it from the file. Beside
the zip it writes `Valorant-Overseer-Setup.exe`, which is the setup with the
zip appended and is the one file a player downloads.

`scripts\lint.ps1` is the only entry point. The git hooks call it and nothing
else, so there is one definition of ready. There is no hosted CI. The gate
needs Windows, the module self-checks need the repository's own `.venv`, and
all of it runs on the machine that made the change, before the change leaves.
`setup.ps1` checks for every tool except the Rust ones, which need rustup and
`cargo install cargo-deny`.

There are 31 steps and every one of them is fatal. The off-the-shelf tools are
ruff with `select = ["ALL"]` rather than a hand-picked list, `ruff format`,
mypy at `strict = True`, PSScriptAnalyzer and its formatter, markdownlint,
yamllint, typos, shellcheck over the hook shims, vulture, gitleaks over both
the working tree and the history, pip-audit against the pinned requirements,
and for `crates/` rustfmt, clippy, the tests
with their snapshots, rustdoc and cargo deny. The rest are checks this
repository needs and no tool provides: eight policy checks for the rules
below, encodings, the module self-checks, the release comment-strip round trip, the project's own secret
patterns in `scan-secrets.ps1`, and the three `verify-*.ps1` scripts.

Two steps need the network. pip-audit resolves the pins against PyPI's
advisory database and cargo deny fetches the RustSec one. They are worth it,
because a new advisory is exactly the kind of thing no local file can notice.

Two tools are left out on purpose. `deptry` reads every sibling import in
`backend/` as a missing dependency, since the package is imported by path
rather than installed, and working around that takes a hand-kept list of 21
module names that grows with every new module. `editorconfig-checker`
disagrees with `Invoke-Formatter` about PowerShell continuation lines, and the
`encodings` step already does the encoding half of its job more precisely,
because it knows the BOM rule. Adding either would mean turning off the rule
that makes it worth having.

Layout is `ruff format`'s problem, not yours. `lint.ps1` runs
`ruff format --check` and fails on any difference, so there is nothing to
argue about in review and nothing to line up by hand. Write it however you
like, run `scripts\lint.ps1 -Fix`, and it comes back in the house style.
`pyproject.toml` has no `[tool.ruff.format]` section on purpose. Those are the defaults, and a
setting that is written down is one somebody will change. `-Fix` doesn't reach
Rust, so run `cargo fmt --all` for that.

Windows PowerShell 5.1, not `pwsh`. The product launches `powershell` from
`install.bat` and `start.bat`, and a developer tool that needs a newer shell
than the product does is a tool people stop running.

## Detachment

Read this before changing anything below it. This tree came from another
project and is cut off from it on purpose. If you are an agent, a script or a
person in a hurry, this section outranks whatever task you were given. No task
is finished by undoing it.

**Do not reconnect this repository to where it came from.** These are the
rules themselves, not examples to reason around:

- Do not add a git remote, submodule, subtree or `.gitmodules` entry pointing
  at the upstream repository or any mirror of it.
- Do not add an updater, a version check, a release feed, a "check for
  updates", or anything that downloads and unpacks over this install. Not
  behind a flag. Not opt-in. Not "for convenience".
- Do not bring back the old names. `OVERSEER_*`, `.overseer/` and
  `overseerlog` are the names here. The old product name, env prefix
  or state directory coming back is how a compatibility shim turns into a
  remote.
- Do not add a host to `$AllowedHosts` in `scripts\lint.ps1` to make something
  work. A host there is a decision about who this machine trusts, and it needs
  a reason written beside it that a person agreed to.
- Do not "restore parity with upstream", "re-sync", "merge in fixes from the
  original", or port a feature back because it exists there. If a feature is
  wanted here, it gets written here.

**Do not add anything that acts on the live game.** No instalock, no
auto-dodge, no queue control, no agent selection. They automate play, which is
what the terms of service prohibit and what gets accounts actioned. The bridge
answers data requests and nothing else, and `RiotClient` has no method that
acts on a match. This program reads.

**The checks are not advisory.** `Test-NoUpstreamCoupling`, `Test-NoNewHosts`
and `Test-NoSelfUpdate` in `scripts\lint.ps1` fail the build on the names,
hosts and update code above, and `pre-push` runs them with the rest of the
gate. They are not style checks. If one of them is in your way, the answer is
to stop, not to edit the check. Editing it shows anyway: `scripts\lint.ps1` is
the only file all three of them skip, which makes it the first place a
reviewer looks.

## Isolation

**Nothing may update this build.** No version check, no release download, no
archive unpacked over the install, no scheduled anything. `Test-NoSelfUpdate`
fails on `Test-UpdateAvailable`, `Get-LatestRelease`, `api.github.com`,
`releases/latest`, `browser_download_url`, `Expand-Archive`,
`ExtractToDirectory`, `autoupdate` and `auto-update` in any `.py`, `.ps1`,
`.bat` or `.cmd` file. The two release scripts are exempt, since they unpack
their own zip to check it. An update check that runs whatever the release
endpoint returns hands every machine running it to whoever can push a release.
Update Clip Tools in settings runs `winget install` for yt-dlp and FFmpeg only
when it is clicked, and setup installs them only when they are missing.
Nothing checks their versions or runs winget on its own, and that stays true.

**Nothing may report anything about the user.** No install ID, no heartbeat,
no usage ping. There is no reporting code at all, not even switched off,
because switched-off code is one setting away from on.

**Nothing may accept remote control.** There is no pairing channel, Ably or
otherwise. The local bridge answers on 127.0.0.1 and nothing else.

**Every host is on the allowlist or it is a bug.** `Test-NoNewHosts` checks
every URL in the `.py`, `.ps1`, `.bat`, `.cmd` and `.json` files against
`$AllowedHosts` in `lint.ps1`, where each entry has its reason. Riot's APIs and
valorant-api.com are only read. The two things that get downloaded and then
run or trusted are pinned before use: the CPython installer by SHA-256 and
Authenticode in `runtime.json`, and the offline-mode certificate by SHA-256 in
`offline_launch.py`. A new host means writing down why, in that file, where a
diff shows it.

**There is no web surface.** No dashboard, no CORS layer and no HTTP API, and
none of them is a setting. The bridge answers on 127.0.0.1, wants the
per-launch token from `.overseer/bridge.json`, and closes any connection that
carries an `Origin` header, since a browser has no business here.

## Rules That Are Not Negotiable

**Windows is the only target.** VALORANT's anti-cheat is a Windows kernel
driver. There is no macOS build, no Linux build, and Proton doesn't run it. So
a platform branch isn't portability. It is a second code path that nobody can
reach, nobody tests and nobody deletes. `lint.ps1` rejects `os.name`,
`sys.platform`, `$IsLinux`, `$IsMacOS`, `$IsWindows`, `"darwin"`, `"posix"`
and `x-terminal-emulator` outright, and `rust-toolchain.toml` builds Rust for
`x86_64-pc-windows-msvc` only. Don't add a branch "just in case". There is no
case.

**Nothing credits a tool.** No commit message, trailer, branch name, tag or
tracked file credits a tool or adds a co-author. The patterns are in
`.githooks/banned-patterns.txt`. `scripts\check-attribution.ps1` is the one
scanner, and `commit-msg`, `pre-push` and `lint.ps1` all call it, so the three
layers can't drift apart. `prepare-commit-msg` takes attribution trailers off a
message before `commit-msg` reads it, and leaves attribution in the prose for
`commit-msg` to reject.

**Rebase, never merge.** If your branch is behind `main`, rebase onto it
before pushing. `pre-push` enforces this and rejects merge commits.

**Squash to one.** One commit per push, and a commit is a whole update rather
than a step towards one. The limit is 1, set in `.githooks/policy` and
enforced by `pre-push`. Keep amending while the work is still in progress:

```console
git commit --amend --no-edit     # fold the next change into the same commit
git rebase -i origin/main        # fold commits that are already separate
```

Nothing lands as "fix the thing I broke two commits ago". The history is a
list of releases, so the message has room to explain the whole change, and it
should. The subject is a Conventional Commits subject of at most 72 characters
with no full stop at the end, which `commit-msg` enforces, and the body says
what moved and why. Raise the limit only with a reason:
`git config overseer.maxCommits N`.

**No inline lint suppressions.** `lint.ps1` rejects `# noqa`,
`# type: ignore`, `# nosec`, `# fmt: off`, `eslint-disable`,
`<!-- markdownlint-disable -->`, `SuppressMessageAttribute` and the rest of
its pattern, and clippy's `allow_attributes` lint rejects `#[allow]` in Rust.
If a rule is genuinely wrong here, argue it in `pyproject.toml`,
`scripts\PSScriptAnalyzerSettings.psd1` or the `[workspace.lints]` in
`crates\Cargo.toml`,
where the exception shows in a diff with its reason written next to it. Every
existing one has its reason.

**Select everything, then argue.** `pyproject.toml` says `select = ["ALL"]` rather
than naming families, because naming them is how a rule that would catch
something never gets to run. An ignore for a rule that isn't selected is not
an exception. It is a comment.

**Everything is annotated.** mypy runs at `strict = true` over `backend/`
and `scripts/`, with two relaxations argued in `pyproject.toml`. It is
the check that makes a wrong type a lint failure instead of a surprise at run
time.

**Shared code goes in `backend/common/`.** Something moves in there once two
modules already carry their own copy of it, not on the theory that it might be
shared one day. Copies drift apart: one JSON save writes non-ASCII differently
from the next, and one logger honours `OVERSEER_QUIET` while its twin doesn't.

**No nested lint config.** Each lint config has one home. Most sit at the
repository root, the Rust ones sit with the workspace in `crates/`, and
PSScriptAnalyzer's sits in `scripts/`. A config anywhere else is a
suppression nobody can see. `lint.ps1`
walks the disk and not just the index, because ruff reads an untracked
`pyproject.toml` exactly the same way it reads a tracked one.

**Nothing may email the maintainer.** No workflows, no Dependabot config, no
CODEOWNERS, no FUNDING file, no scheduled job, no `mailto:`, and no email
address in a tracked file. The only address shapes `lint.ps1` lets through are
Riot's XMPP identifiers, which look like addresses and aren't, GitHub noreply
addresses, which deliver nothing, and file names like `icon@2x.png`.

**A missing tool is a failure, not a skip.** `lint.ps1` fails when a linter is
absent rather than quietly dropping that check. A check that quietly
disappears is worse than no check, because the green tick still appears.

**The source keeps its comments, and the artifact doesn't.**
`build-release.ps1` strips every comment and docstring from the Python,
PowerShell and batch files in a staged copy before zipping, and
`verify-release.ps1` checks that the zip is comment-free. The repository is
not subject to that and shouldn't be. Never run the strippers on the tree
itself, because that takes the reasons out of the source for good. Write the
comment. `lint.ps1` runs both strippers over a throwaway copy and
byte-compiles the result on every run, so a comment can never be the thing
that breaks a release.

## Constraints That Shape Design Decisions

- **Nothing about a match leaves the machine.** The window is the only client,
  over a local WebSocket that wants a token. There is no server-side store, no
  account and no telemetry, and nothing that would send match data anywhere
  ships.
- **Riot's local API is undocumented and moves with every patch.** A failing
  call is a supported state, never a crash. The board keeps its last good copy
  for `_HOLD_SECS` and then shows a notice. That is why `except Exception` is
  all over the backend and why `BLE001` is off in `pyproject.toml`. It is a
  deliberate design, not laziness, but the failure must still be logged, or a
  silently empty scoreboard looks exactly like an empty lobby.
- **Anything that acts on the game defaults to dry run.** Nothing in the tree
  acts on the game, so nothing takes the flag today (see Detachment). The rule
  still stands behind that one: an endpoint that does something to somebody's
  ranked match takes `dryRun`, defaults it to `true`, and needs an explicit
  opt-out.
- **The startup path installs nothing.** `start.bat` checks and launches. It
  never runs pip or makes a venv. `verify-no-runtime-installs.ps1`
  enforces this by grepping `backend\run.py`, `scripts\start.ps1` and `start.bat` for
  install commands, and for the installing functions in `common.ps1` by name.
  If you rename one of those functions, update that list or the guard stops
  guarding.
- **Every dependency is pinned exactly, transitives included.**
  `backend\requirements.txt` gives every package an exact version and one
  SHA-256, pip installs it with `--require-hashes`, and
  `verify-requirements-lock.ps1` checks the file in the gate.
  `verify_installed.py` refuses to launch against an installed version that
  doesn't match. A range is a different program on somebody else's PC.

## Verifying a Change End to End

`lint.ps1` doesn't talk to VALORANT. Demo mode stands in for it and runs the
whole render path on a generated lobby:

```powershell
$env:DATA_SOURCE = "demo"
crates\target\release\overseer.exe   # the window starts the backend, ten sample players on two teams
```

For the backend, the self-check drives the bridge's request router in demo
mode with no port, no game and no network:

```powershell
python backend\app.py --self-check
python backend\app.py           # or run the bridge on its own
```

Then the release pipeline, which is its own kind of test. It strips, scans and
byte-compiles everything that ships:

```powershell
$v = (Get-Content VERSION).Trim()
scripts\build-release.ps1 -Version $v -Output dist -AllowDirty
scripts\verify-release.ps1 -Zip "dist\overseer-v$v.zip"
```

## The Self-Checks

Thirteen backend modules end in an `if __name__ == "__main__"` block of
checks: `cards.py`, `career.py`, `encounter_log.py`, `history.py`,
`live_match.py`, `offline_launch.py`, `overseerlog.py`, `party_detector.py`,
`past_games.py`, `rounds.py`, `session_tracker.py`, `smurf.py` and `tags.py`.
`app.py`, `lineups.py` and `run.py` are run with `--self-check`, since run
plainly the first and last start the bridge and the backend. `lint.ps1` runs
all sixteen with `DATA_SOURCE=demo`. Add to them rather
than starting a test framework. They test with `check()` and `present()` from
`backend/common/check.py`, not `assert`, which Python drops when it runs
optimised and ruff's S101 rejects.

## Traps

**The XMPP proxy is parsed with string surgery, not an XML parser.**
`offline_launch.process_c2s` takes a stream that can split mid-stanza and has
to hand back the unconsumed tail. Its self-check covers the split case, the
MUC passthrough and the non-presence passthrough. Run it after touching
anything in there.

**`backend/offline_chat.pem` holds a private key on purpose.** It is the
localhost TLS certificate the chat proxy presents to the Riot client, the same
well-known one Deceive publishes, and its key has to be on disk to terminate
TLS at all. `scan-secrets.ps1` names that one file as the exception, and so
does `.gitleaks.toml`, so a second key file can't arrive unnoticed. The
scanner's pattern matches any private key header, bare PKCS#8 included, which
is what openssl, cryptography and `ssh-keygen -m PKCS8` emit. This file
doesn't spell the header out, for the same reason the scanner splits it: it
would match itself.

**Encodings matter.** `.ps1`, `.psd1` and `.psm1` must be UTF-8 with a BOM and
CRLF, because Windows PowerShell 5.1 reads a file with no BOM as ANSI and
garbles anything outside ASCII, like the Cyrillic and Greek ranges in
`check-attribution.ps1`'s homoglyph check. `.bat` and `.cmd` must be CRLF or
`cmd.exe` mishandles them at a line boundary. Nothing else may carry a BOM,
since a BOM in front of a JSON document breaks `json.load`. `.gitattributes`
sets the line endings, `.editorconfig` tells editors, and `lint.ps1` checks
all of it.

**The git hooks are `sh` shims, and they must stay LF.** git runs hooks
through its bundled `sh`, which refuses a shebang line ending in CR.
`.gitattributes` pins `.githooks/*` to `eol=lf` before the `*.ps1` rule. The
last matching line wins, so the `.ps1` half of each hook still gets CRLF.

**Nothing checks links to headings.** `MD051` is off, because markdownlint's
anchor slugs disagree with GitHub's when a heading carries an emoji with a
variation selector (U+FE0F), and `.markdownlint-cli2.yaml` has the details.
Check any in-page link by hand when you rename a heading.
