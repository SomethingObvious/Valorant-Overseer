param(
    [switch]$Fix,
    [switch]$Staged
)

# The single lint entry point. The pre-commit and pre-push hooks call this and
# nothing else, so there is exactly one definition of "this is ready".
#
#   scripts\lint.ps1            everything, read-only
#   scripts\lint.ps1 -Fix       apply the safe automatic fixes first
#   scripts\lint.ps1 -Staged    only files staged for commit (used by pre-commit)
#
# Every check is fatal. There is no warning tier, and a missing tool is a
# failure rather than a silent skip. A check that quietly disappears is worse
# than no check, because the green tick still appears.
#
# There is no hosted CI. Every check here needs Windows, three of them need the
# repository's own .venv, and all of them finish in seconds on the machine that
# made the change, so they run there, before it leaves.
#
# PowerShell 5.1 compatible: install.bat and start.bat launch `powershell`, not
# `pwsh`, and a developer tool that needs a newer shell than the product does is
# a tool people stop running.

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "common.ps1")

# Script scope, so every function below reads the same two flags.
$Script:AutoFix = [bool]$Fix
$Script:StagedOnly = [bool]$Staged

$Failed = @()

function Invoke-Step([string]$Name, [scriptblock]$Body) {
    Write-Host ""
    Write-Host "> $Name" -ForegroundColor White
    $ok = $false
    try { $ok = [bool](& $Body) } catch {
        Write-Host "  x $($_.Exception.Message)" -ForegroundColor Red
        $ok = $false
    }
    if (-not $ok) { $script:Failed += $Name }
}

function Test-Tool([string]$Name) {
    if (Get-Command $Name -ErrorAction SilentlyContinue) { return $true }
    Write-Host "  x $Name is not installed. Run scripts\setup.ps1." -ForegroundColor Red
    return $false
}

# Out-Host, not the pipeline: a tool's own output belongs on the screen, and
# letting it flow back would mean Invoke-Step casting an array of stdout lines
# to a boolean instead of the exit status it asked for.
function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & $Exe @Arguments 2>&1 | Out-Host; return ($LASTEXITCODE -eq 0) }
    finally { $ErrorActionPreference = $prev }
}

# --- the file list -------------------------------------------------------------
if ($Script:StagedOnly) {
    $TrackedFiles = @(& git -C $Root diff --cached --name-only --diff-filter=ACMR)
}
else {
    $TrackedFiles = @(& git -C $Root ls-files)
}
$TrackedFiles = @($TrackedFiles | Where-Object { $_ })


# git lists paths with forward slashes. A step whose paths don't resolve checks
# nothing and still passes, so every step maps them here, and the next line
# makes sure the mapping finds this script.
function Get-FullPath([string]$Rel) { return (Join-Path $Root ($Rel -replace '/', '\')) }
if (-not (Test-Path -LiteralPath (Get-FullPath "scripts/lint.ps1"))) { throw "Get-FullPath can't find scripts/lint.ps1." }

# Reads a file's content as git will actually store it. In staged mode that is
# the index blob, not the working tree, otherwise `git add` followed by an edit
# on disk smuggles anything past every content check below.
function Get-TrackedText([string]$Rel) {
    if ($Script:StagedOnly) {
        $prev = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try { $text = (& git -C $Root show ":$Rel" 2>$null) -join "`n" } finally { $ErrorActionPreference = $prev }
        return $text
    }
    $full = Get-FullPath $Rel
    if (-not (Test-Path -LiteralPath $full)) { return "" }
    return [System.IO.File]::ReadAllText($full)
}

function Test-BinaryFile([string]$Rel) {
    $full = Get-FullPath $Rel
    if (-not (Test-Path -LiteralPath $full)) { return $true }
    $bytes = [System.IO.File]::ReadAllBytes($full)
    $probe = [Math]::Min($bytes.Length, 8192)
    for ($i = 0; $i -lt $probe; $i++) { if ($bytes[$i] -eq 0) { return $true } }
    return $false
}

# --- policy checks -------------------------------------------------------------

# Only this file, AGENTS.md and the two lint configs may name a suppression
# form, because they are what define and document the rule.
$SuppressionAllowlist = '^(scripts/lint\.ps1|AGENTS\.md|pyproject\.toml|scripts/PSScriptAnalyzerSettings\.psd1)$'
$SuppressionPattern =
'#\s*(noqa|nosec|type:\s*ignore|ruff:\s*noqa|pylint:\s*disable|fmt:\s*(off|skip))' +
'|eslint-disable' +
'|markdownlint-(disable|restore|capture|configure)' +
'|SuppressMessageAttribute' +
'|PSScriptAnalyzer\s*-\s*disable'

function Test-NoSuppressions {
    $hits = @()
    foreach ($rel in $TrackedFiles) {
        if ($rel -match $SuppressionAllowlist) { continue }
        if (Test-BinaryFile $rel) { continue }
        $n = 0
        foreach ($line in ((Get-TrackedText $rel) -split "`r?`n")) {
            $n++
            if ($line -match $SuppressionPattern) { $hits += "${rel}:${n}: $($line.Trim())" }
        }
    }
    if ($hits.Count -eq 0) { Write-Host "  + none" -ForegroundColor Green; return $true }
    Write-Host "  x inline lint suppressions are not permitted:" -ForegroundColor Red
    foreach ($h in $hits) { Write-Host "      $h" -ForegroundColor Red }
    Write-Host "    Argue the exception in pyproject.toml or scripts\PSScriptAnalyzerSettings.psd1 instead." -ForegroundColor Red
    return $false
}

# A lint config in a subdirectory is a suppression that Test-NoSuppressions
# cannot see: it silently weakens the rules for that directory and never appears
# in a diff anybody reads. The disk is checked as well as the index, because an
# untracked pyproject.toml is read by ruff exactly the same way a tracked one is.
$ConfigNames = @("ruff.toml", ".ruff.toml", "pyproject.toml", "setup.cfg", "tox.ini",
    ".flake8", ".markdownlint-cli2.yaml",
    ".markdownlint.yaml", ".markdownlint.json", ".editorconfig",
    "mypy.ini", ".mypy.ini", ".gitleaks.toml", ".typos.toml", "_typos.toml",
    ".yamllint.yml", ".yamllint.yaml")
# The few that sit with what they configure instead: the Rust ones at the
# workspace root in crates, and PowerShell's beside the scripts.
$ConfigHomes = @{
    "clippy.toml"                   = "crates"
    ".clippy.toml"                  = "crates"
    "rustfmt.toml"                  = "crates"
    ".rustfmt.toml"                 = "crates"
    "deny.toml"                     = "crates"
    "PSScriptAnalyzerSettings.psd1" = "scripts"
}

function Test-NoNestedConfig {
    $hits = @()
    foreach ($file in (Get-ChildItem -LiteralPath $Root -Recurse -File -Force)) {
        if ($file.FullName -match '\\(\.git|\.venv|node_modules|__pycache__|dist|frontend|target)\\') { continue }
        $dir = $file.DirectoryName.Substring($Root.TrimEnd('\').Length).Trim('\')
        if ($ConfigHomes.ContainsKey($file.Name)) {
            if ($dir -eq $ConfigHomes[$file.Name]) { continue }
        }
        elseif ($ConfigNames -notcontains $file.Name -or $dir -eq "") { continue }
        $hits += $file.FullName.Substring($Root.Length).TrimStart('\')
    }
    if ($hits.Count -eq 0) { Write-Host "  + configuration is only where it belongs" -ForegroundColor Green; return $true }
    Write-Host "  x lint configuration has one home each, and these are elsewhere:" -ForegroundColor Red
    foreach ($h in $hits) { Write-Host "      $h" -ForegroundColor Red }
    return $false
}

# Nothing here may cause the maintainer's inbox to fill up. No workflows, no
# Dependabot config, no CODEOWNERS, and no address to send anything to.
function Test-NoMail {
    $hits = @()
    foreach ($rel in $TrackedFiles) {
        if ($rel -match '^\.github/(workflows/|dependabot\.ya?ml$|FUNDING\.ya?ml$)' -or
            $rel -match '(^|/)CODEOWNERS$') {
            $hits += "$rel (a scheduled or automated job reports by mail)"
        }
    }
    foreach ($rel in $TrackedFiles) {
        if ($rel -match '^(scripts/lint\.ps1|AGENTS\.md)$') { continue }
        if (Test-BinaryFile $rel) { continue }
        $n = 0
        foreach ($line in ((Get-TrackedText $rel) -split "`r?`n")) {
            $n++
            if ($line -notmatch 'mailto:|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}') { continue }
            if ($line -match 'users\.noreply\.github\.com') { continue }
            # Riot's chat is XMPP, so its identifiers are shaped exactly like
            # addresses, player@eu1.pvp.net, room@muc, and offline mode is
            # built entirely out of them. No mail has ever left a JID.
            if ($line -match '@[A-Za-z0-9.-]*pvp\.net|@muc') { continue }
            # icon@2x.png is a local part, an at sign, a domain and a
            # three-letter tail. Nothing short of knowing what a filename looks
            # like can tell it from an address.
            if ($line -match '@[A-Za-z0-9.-]*\.(png|jpe?g|svg|ico|webp|gif|mp4|json|toml|ya?ml|md|py|ps1|bat|txt|zip|exe)([^A-Za-z0-9]|$)') { continue }
            $hits += "${rel}:${n}: $($line.Trim())"
        }
    }
    if ($hits.Count -eq 0) { Write-Host "  + nothing that emails anybody" -ForegroundColor Green; return $true }
    Write-Host "  x nothing here may email the maintainer:" -ForegroundColor Red
    foreach ($h in $hits) { Write-Host "      $h" -ForegroundColor Red }
    return $false
}

# This tree came from another project. It is not coupled to it, must not
# become coupled to it, and this is the check that says so.
#
# The names below are the ones that would do the coupling: the old product
# name, the old env-var prefix, the old state directory, the upstream owner.
# A rename that reintroduces any of them is how a helpful pass at 'restoring
# compatibility' quietly puts a remote, an updater or a host back.
#
# README.md, AGENTS.md and this file are exempt, because they name the origin
# only to warn about it. The build is private and never distributed, so no
# attribution is owed anywhere else.
$UpstreamNames = @(
    'valorant[-_ ]?scout', 'kryotrades', 'valorantscout',
    'VALORANT_SCOUT', 'ScoutDir', 'SCOUT_[A-Z_]+', 'scoutlog', 'scout_commands'
)

function Test-NoUpstreamCoupling {
    $hits = @()
    foreach ($rel in $TrackedFiles) {
        if ($rel -match '^(README\.md|AGENTS\.md|scripts/lint\.ps1)$') { continue }
        if (Test-BinaryFile $rel) { continue }
        $n = 0
        foreach ($line in ((Get-TrackedText $rel) -split "`r?`n")) {
            $n++
            foreach ($name in $UpstreamNames) {
                if ($line -match $name) { $hits += "${rel}:${n}: $($line.Trim())" }
            }
        }
    }

    # A remote is the other half of coupling, and it lives outside the tree.
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { $remotes = @(& git -C $Root remote -v 2>$null) } finally { $ErrorActionPreference = $prev }
    foreach ($r in $remotes) {
        foreach ($name in $UpstreamNames) {
            if ($r -match $name) { $hits += "git remote: $r" }
        }
    }

    if ($hits.Count -eq 0) {
        Write-Host "  + nothing ties this tree to where it came from" -ForegroundColor Green
        return $true
    }
    Write-Host "  x this tree must stay detached from the project it came from:" -ForegroundColor Red
    foreach ($h in $hits) { Write-Host "      $h" -ForegroundColor Red }
    Write-Host "    See the Detachment section of AGENTS.md before changing this." -ForegroundColor Red
    return $false
}

# Every host this program may talk to, and why. Anything else is a new trust
# relationship, and the whole point of this build is that there are no
# surprises in that list.
#
# There is deliberately no update server, telemetry endpoint or hosted
# dashboard in it. This build does not update itself and does not report
# anything about you anywhere. If a host appears below that you did not put
# there, that is the finding.
$AllowedHosts = @(
    # Riot's own APIs. Talking to these is what the program is for.
    'pd\.', 'glz-?', 'shared\.', '.*\.a\.pvp\.net', '.*\.riotgames\.com',
    'riot-geo\.pas\.si\.riotgames\.com', 'clientconfig\.rpg\.riotgames\.com',
    # Read-only art and metadata: agent portraits, rank icons, skin names.
    # Nothing about the player is sent, and nothing it returns is executed.
    'valorant-api\.com', 'media\.valorant-api\.com',
    # The CPython installer, pinned by SHA-256 and Authenticode in
    # runtime.json before it is ever run.
    'www\.python\.org',
    # The offline-mode chat certificate, pinned by SHA-256 in offline_launch.
    'mln\.cx',
    'localhost', '127\.0\.0\.1'
)

function Test-NoNewHosts {
    $hits = @()
    foreach ($rel in $TrackedFiles) {
        if ($rel -notmatch '\.(py|ps1|bat|cmd|json)$') { continue }
        if ($rel -match '^scripts/lint\.ps1$') { continue }
        $n = 0
        foreach ($line in ((Get-TrackedText $rel) -split "`r?`n")) {
            $n++
            # The host token, interpolation included, so that a URL built
            # at runtime can be told apart from one baked into the code.
            # `https://pd.{self.region}.a.pvp.net` names a Riot shard the
            # caller chose. It is not a fixed destination.
            foreach ($m in [regex]::Matches($line, 'https?://([^/\s"'')]+)')) {
                $found = $m.Groups[1].Value
                if ($found -match '[{}$]') { continue }
                # A port is not part of the host.
                $found = ($found -split ':')[0]
                $ok = $false
                foreach ($alw in $AllowedHosts) {
                    if ($found -match "^$alw`$") { $ok = $true; break }
                }
                if (-not $ok) { $hits += "${rel}:${n}: $found" }
            }
        }
    }
    if ($hits.Count -eq 0) {
        Write-Host "  + no host outside the allowlist" -ForegroundColor Green
        return $true
    }
    Write-Host "  x this build talks to hosts that are not on the allowlist:" -ForegroundColor Red
    foreach ($h in $hits) { Write-Host "      $h" -ForegroundColor Red }
    Write-Host "    Add it to `$AllowedHosts with its reason, or take it out." -ForegroundColor Red
    return $false
}

# Nothing may replace the code on this machine except the person at the
# keyboard. No update check, no self-download, no unpacking an archive over
# the install. A repository can be compromised, but a build that cannot update
# itself does not care.
$SelfUpdatePattern = 'Test-UpdateAvailable|Get-LatestRelease|api\.github\.com|' +
'releases/latest|Expand-Archive|ExtractToDirectory|browser_download_url|' +
'autoupdate|auto-update'

function Test-NoSelfUpdate {
    $hits = @()
    foreach ($rel in $TrackedFiles) {
        if ($rel -notmatch '\.(py|ps1|bat|cmd)$') { continue }
        # The release builder unpacks the zip it just made, to verify it. That
        # is a build step on the maintainer's machine, not a runtime path.
        if ($rel -match '^scripts/(lint|build-release|verify-release)\.ps1$') { continue }
        $n = 0
        foreach ($line in ((Get-TrackedText $rel) -split "`r?`n")) {
            $n++
            if ($line -match $SelfUpdatePattern) { $hits += "${rel}:${n}: $($line.Trim())" }
        }
    }
    if ($hits.Count -eq 0) {
        Write-Host "  + nothing here can update itself" -ForegroundColor Green
        return $true
    }
    Write-Host "  x this build must never update itself:" -ForegroundColor Red
    foreach ($h in $hits) { Write-Host "      $h" -ForegroundColor Red }
    return $false
}

# VALORANT runs on Windows and nowhere else. Riot's anti-cheat is a kernel
# driver, and there is no macOS build, no Linux build and no Proton. So a branch
# guarded on the platform is not portability. It is a second code path that
# nobody can reach, nobody tests and nobody deletes.
$PlatformPattern = 'os\.name|sys\.platform|\$IsLinux|\$IsMacOS|\$IsWindows' +
'|["'']darwin["'']|["'']posix["'']|x-terminal-emulator'

function Test-WindowsOnly {
    $hits = @()
    foreach ($rel in $TrackedFiles) {
        if ($rel -match '^(scripts/lint\.ps1|AGENTS\.md)$') { continue }
        if ($rel -notmatch '\.(py|ps1|bat|cmd)$') { continue }
        $n = 0
        foreach ($line in ((Get-TrackedText $rel) -split "`r?`n")) {
            $n++
            if ($line -match $PlatformPattern) { $hits += "${rel}:${n}: $($line.Trim())" }
        }
    }
    if ($hits.Count -eq 0) { Write-Host "  + no unreachable platform branches" -ForegroundColor Green; return $true }
    Write-Host "  x Windows is the only target, so these branches can never run:" -ForegroundColor Red
    foreach ($h in $hits) { Write-Host "      $h" -ForegroundColor Red }
    return $false
}

# PowerShell files must be UTF-8 with a BOM and CRLF. Without the BOM, Windows
# PowerShell reads them as ANSI and garbles anything outside ASCII. .bat must be
# CRLF or cmd.exe mishandles it at a line boundary. Everything else must have no
# BOM, because a BOM in front of a JSON document breaks json.load.
# verify-release checks the same thing on the artifact, and this catches it
# before the commit.
function Test-Encodings {
    $hits = @()
    foreach ($rel in $TrackedFiles) {
        $full = Get-FullPath $rel
        if (-not (Test-Path -LiteralPath $full)) { continue }
        if (Test-BinaryFile $rel) { continue }
        $bytes = [System.IO.File]::ReadAllBytes($full)
        $hasBom = ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)
        $text = [System.IO.File]::ReadAllText($full)
        $lfOnly = (($text -replace "`r`n", "") -match "`n")
        if ($rel -match '\.ps(1|d1|m1)$') {
            if (-not $hasBom) { $hits += "$rel is not UTF-8 with a BOM" }
            if ($lfOnly) { $hits += "$rel has LF-only line endings" }
        }
        elseif ($rel -match '\.(bat|cmd)$') {
            if ($lfOnly) { $hits += "$rel has LF-only line endings" }
        }
        elseif ($hasBom) {
            $hits += "$rel has a byte-order mark"
        }
    }
    if ($hits.Count -eq 0) { Write-Host "  + encodings are right" -ForegroundColor Green; return $true }
    foreach ($h in $hits) { Write-Host "  x $h" -ForegroundColor Red }
    return $false
}

# --- tools ---------------------------------------------------------------------

# Lint and format are one step because they are one tool reading one config, and
# a formatter that is merely available is a formatter half the tree has never
# been through. `check --fix` runs before `format`, which is ruff's own order: a
# fix can leave a line over the limit and the formatter is what puts it back.
function Invoke-Ruff {
    if (-not (Test-Tool "ruff")) { return $false }
    $target = @($TrackedFiles | Where-Object { $_ -match '\.py$' } | ForEach-Object { Get-FullPath $_ })
    if ($target.Count -eq 0) { Write-Host "  + no Python files in scope" -ForegroundColor Green; return $true }
    $ok = $true
    if ($Script:AutoFix) {
        $ok = (Invoke-Native "ruff" (@("check", "--fix") + $target)) -and $ok
        $ok = (Invoke-Native "ruff" (@("format") + $target)) -and $ok
    }
    $ok = (Invoke-Native "ruff" (@("check") + $target)) -and $ok
    return (Invoke-Native "ruff" (@("format", "--check") + $target)) -and $ok
}

function Invoke-ScriptAnalyzerCheck {
    if (-not (Get-Module -ListAvailable PSScriptAnalyzer)) {
        Write-Host "  x PSScriptAnalyzer is not installed. Run scripts\setup.ps1." -ForegroundColor Red
        return $false
    }
    Import-Module PSScriptAnalyzer -ErrorAction Stop
    $target = @($TrackedFiles | Where-Object { $_ -match '\.(ps1|psd1|psm1)$' } |
            ForEach-Object { Get-FullPath $_ } |
            Where-Object { Test-Path -LiteralPath $_ })
    if ($target.Count -eq 0) { Write-Host "  + no PowerShell files in scope" -ForegroundColor Green; return $true }
    $settings = Join-Path $PSScriptRoot "PSScriptAnalyzerSettings.psd1"
    # One file at a time: -Path takes a single string, and handing it an
    # array fails with a type error rather than scanning anything.
    $found = @()
    foreach ($file in $target) { $found += @(Invoke-ScriptAnalyzer -Path $file -Settings $settings) }
    if ($found.Count -eq 0) { Write-Host "  + clean" -ForegroundColor Green; return $true }
    foreach ($f in $found) {
        $name = $f.ScriptName
        Write-Host "  x ${name}:$($f.Line) $($f.RuleName) $($f.Message)" -ForegroundColor Red
    }
    return $false
}

function Invoke-Markdownlint {
    if (-not (Test-Tool "markdownlint-cli2")) { return $false }
    Push-Location $Root
    try { return (Invoke-Native "markdownlint-cli2" @()) } finally { Pop-Location }
}

# The type checker. Rust gets this from the compiler and Python has to opt in.
# pyproject.toml explains how far it is turned up and why. It reads its own config,
# so there is no file list to keep in step with this script.
function Invoke-Mypy {
    if (-not (Test-Tool "mypy")) { return $false }
    Push-Location $Root
    try { return (Invoke-Native "mypy" @()) } finally { Pop-Location }
}

# Dead code, at 80% confidence. Below 80 it starts guessing.
function Invoke-Vulture {
    if (-not (Test-Tool "vulture")) { return $false }
    Push-Location $Root
    try {
        return (Invoke-Native "vulture" @("backend", "scripts",
                "--min-confidence", "80"))
    }
    finally { Pop-Location }
}

# The hook shims are the only shell in the tree, and they are what stands
# between a bad commit and the repository. A shim is the file with a .ps1 of
# the same name next to it. .githooks/policy is git config, not shell.
function Invoke-Shellcheck {
    if (-not (Test-Tool "shellcheck")) { return $false }
    $target = @($TrackedFiles | Where-Object { $_ -match '^\.githooks/[a-z-]+$' } |
            ForEach-Object { Get-FullPath $_ } |
            Where-Object { (Test-Path -LiteralPath $_) -and (Test-Path -LiteralPath "$_.ps1") })
    if ($target.Count -eq 0) { Write-Host "  + no shell in scope" -ForegroundColor Green; return $true }
    return (Invoke-Native "shellcheck" (@("--severity=style", "--enable=all", "--shell=sh") + $target))
}

function Invoke-Yamllint {
    if (-not (Test-Tool "yamllint")) { return $false }
    $target = @($TrackedFiles | Where-Object { $_ -match '\.ya?ml$' } |
            ForEach-Object { Get-FullPath $_ })
    if ($target.Count -eq 0) { Write-Host "  + no YAML in scope" -ForegroundColor Green; return $true }
    return (Invoke-Native "yamllint" (@("-s", "-c", (Join-Path $Root ".yamllint.yml")) + $target))
}

function Invoke-Typos {
    if (-not (Test-Tool "typos")) { return $false }
    Push-Location $Root
    try { return (Invoke-Native "typos" @()) } finally { Pop-Location }
}

# Both halves. `gitleaks dir` sees a secret that is on disk but not committed
# yet. `gitleaks git` sees one that was committed and then deleted, which is
# still sitting in the pack file and is the half a working-tree scan can never
# find. scan-secrets.ps1 stays alongside it for the patterns gitleaks has no
# way to know about: this project's canary and a developer's home directory.
function Invoke-Gitleaks {
    if (-not (Test-Tool "gitleaks")) { return $false }
    $cfg = Join-Path $Root ".gitleaks.toml"
    # The build tree is excluded in .gitleaks.toml rather than here, because the
    # two scans below take different path shapes and one config covers both.
    $ok = Invoke-Native "gitleaks" @("dir", "--no-banner", "--redact", "--exit-code", "1",
        "-c", $cfg, $Root)
    return (Invoke-Native "gitleaks" @("git", "--no-banner", "--redact", "--exit-code", "1",
            "-c", $cfg, $Root)) -and $ok
}

# The dependency policy: pinned is not the same as safe. It resolves the pins
# against PyPI, so it is the second of the two checks here that needs the
# network.
function Invoke-PipAudit {
    if (-not (Test-Tool "pip-audit")) { return $false }
    return (Invoke-Native "pip-audit" @("-r", (Join-Path $Root "backend\requirements.txt"),
            "--progress-spinner", "off"))
}

# PSScriptAnalyzer ships a formatter as well as a linter, and an unformatted
# script is the same review argument ruff format exists to end.
function Test-PowerShellFormat {
    if (-not (Get-Module -ListAvailable PSScriptAnalyzer)) {
        Write-Host "  x PSScriptAnalyzer is not installed. Run scripts\setup.ps1." -ForegroundColor Red
        return $false
    }
    Import-Module PSScriptAnalyzer -ErrorAction Stop
    $bom = New-Object System.Text.UTF8Encoding($true)
    $bad = @()
    foreach ($rel in $TrackedFiles) {
        if ($rel -notmatch '\.ps1$') { continue }
        $full = Get-FullPath $rel
        if (-not (Test-Path -LiteralPath $full)) { continue }
        $text = [System.IO.File]::ReadAllText($full)
        $formatted = Invoke-Formatter -ScriptDefinition $text
        $formatted = ($formatted -replace "`r`n", "`n") -replace "`n", "`r`n"
        if ($formatted -eq $text) { continue }
        if ($Script:AutoFix) {
            [System.IO.File]::WriteAllText($full, $formatted, $bom)
            continue
        }
        $bad += $rel
    }
    if ($bad.Count -eq 0) { Write-Host "  + formatted" -ForegroundColor Green; return $true }
    Write-Host "  x not formatted (run scripts\lint.ps1 -Fix):" -ForegroundColor Red
    foreach ($b in $bad) { Write-Host "      $b" -ForegroundColor Red }
    return $false
}

# The modules with an `if __name__ == "__main__"` self-check.
$SelfChecks = @("backend\encounter_log.py", "backend\history.py",
    "backend\live_match.py", "backend\rounds.py", "backend\smurf.py", "backend\career.py",
    "backend\offline_launch.py", "backend\party_detector.py", "backend\past_games.py",
    "backend\overseerlog.py", "backend\tags.py", "backend\cards.py",
    "backend\session_tracker.py")

# Run plainly, app.py starts the WebSocket bridge and run.py the backend, so
# both take a flag. app.py's drives the bridge's request router in demo mode,
# which is the whole data surface. run.py's covers the launcher, which decides
# whether any of the rest ever runs.
$SelfCheckArgs = @{
    "backend\app.py"     = @("--self-check")
    "backend\run.py"     = @("--self-check")
    "backend\lineups.py" = @("--self-check")
}

function Invoke-SelfChecks {
    $py = $VenvPy
    if (-not (Test-Path $py)) {
        $cmd = Get-Command python -ErrorAction SilentlyContinue
        if (-not $cmd) {
            Write-Host "  x no .venv and no python on PATH. Run install.bat." -ForegroundColor Red
            return $false
        }
        $py = $cmd.Source
    }
    $ok = $true
    # A self-check must not care whether VALORANT happens to be running.
    # Without this, app.py's prints a connection-refused traceback while
    # passing, and a check that prints a traceback when it passes is a
    # check people stop reading.
    $prevSource = $env:DATA_SOURCE
    $env:DATA_SOURCE = "demo"
    try {
        foreach ($rel in ($SelfChecks + @($SelfCheckArgs.Keys))) {
            $full = Join-Path $Root $rel
            if (-not (Test-Path $full)) {
                Write-Host "  x missing: $rel" -ForegroundColor Red
                $ok = $false
                continue
            }
            $extra = @()
            if ($SelfCheckArgs.ContainsKey($rel)) { $extra = $SelfCheckArgs[$rel] }
            if (-not (Invoke-Native $py (@($full) + $extra))) {
                Write-Host "  x self-check failed: $rel" -ForegroundColor Red
                $ok = $false
            }
        }
    }
    finally { $env:DATA_SOURCE = $prevSource }
    return $ok
}

# The release build strips every comment and docstring from a staged copy of the
# tree, then byte-compiles what is left. If that fails, it fails at release
# time, with a tag already cut. Running the same two strippers over a throwaway
# copy here means a comment can never be the thing that breaks a release, which
# is what makes it safe to write comments in the first place.
function Test-StripRoundTrip {
    $py = $VenvPy
    if (-not (Test-Path $py)) {
        $cmd = Get-Command python -ErrorAction SilentlyContinue
        if (-not $cmd) { Write-Host "  x no python available" -ForegroundColor Red; return $false }
        $py = $cmd.Source
    }
    $work = Join-Path $env:TEMP ("vg-strip-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $work -Force | Out-Null
    try {
        foreach ($rel in $TrackedFiles) {
            if ($rel -notmatch '\.(py|ps1|bat|cmd)$') { continue }
            $src = Get-FullPath $rel
            if (-not (Test-Path -LiteralPath $src)) { continue }
            $dst = Join-Path $work ($rel -replace '/', '\')
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dst) | Out-Null
            Copy-Item -Force -LiteralPath $src -Destination $dst
        }
        if (-not (Invoke-Native $py @((Join-Path $PSScriptRoot "strip_comments.py"), $work))) { return $false }
        if (-not (Invoke-Native "powershell.exe" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                    (Join-Path $PSScriptRoot "strip_script_comments.ps1"), "-Root", $work))) { return $false }
        if (-not (Invoke-Native $py @("-m", "compileall", "-q", $work))) {
            Write-Host "  x stripped Python does not byte-compile" -ForegroundColor Red
            return $false
        }
        Write-Host "  + strips clean and still compiles" -ForegroundColor Green
        return $true
    }
    finally { Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue }
}

function Test-Secrets {
    return (Invoke-Native "powershell.exe" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
            (Join-Path $PSScriptRoot "scan-secrets.ps1"), "-Path", $Root))
}

function Invoke-Verifier([string]$Script) {
    return (Invoke-Native "powershell.exe" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
            (Join-Path $PSScriptRoot $Script)))
}

# --- the window ----------------------------------------------------------------
#
# One gate, so the Rust half is checked the same way and at the same time as
# everything else. Cargo is not on PATH in every shell that runs this, and a
# missing tool is a failure here rather than a skipped check: a green tick that
# skipped the linter is worse than no tick.

function Get-Cargo {
    $onPath = Get-Command cargo -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    # Not $home: that is a read-only automatic variable, and assigning to it
    # fails the whole function with a message about the variable rather than
    # about cargo.
    $installed = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (Test-Path $installed) { return $installed }
    return $null
}

# The workspace, its toolchain pin and its lint configs are all in crates.
function Invoke-Cargo([string[]]$Arguments) {
    $cargo = Get-Cargo
    if (-not $cargo) {
        Write-Host "  x cargo isn't installed. Install rustup." -ForegroundColor Red
        return $false
    }
    Push-Location (Join-Path $Root "crates")
    try { return (Invoke-Native $cargo $Arguments) } finally { Pop-Location }
}

function Invoke-CargoFormat {
    return (Invoke-Cargo @("fmt", "--all", "--", "--check"))
}

# Every lint level lives in Cargo.toml and clippy.toml, so this needs no flags
# beyond the targets: tests and benches are code too, and the restriction lints
# are exactly the ones a test is tempted to break.
function Invoke-Clippy {
    return (Invoke-Cargo @("clippy", "--workspace", "--all-targets"))
}

function Invoke-CargoTest {
    return (Invoke-Cargo @("test", "--workspace"))
}

# Rustdoc lints only fire under rustdoc, so the documentation has to be built
# to reach them.
function Invoke-CargoDoc {
    return (Invoke-Cargo @("doc", "--workspace", "--no-deps", "--quiet"))
}

function Invoke-CargoDeny {
    $deny = Join-Path $env:USERPROFILE ".cargo\bin\cargo-deny.exe"
    if (-not ((Get-Command cargo-deny -ErrorAction SilentlyContinue) -or (Test-Path $deny))) {
        Write-Host "  x cargo-deny isn't installed. Run cargo install cargo-deny." -ForegroundColor Red
        return $false
    }
    return (Invoke-Cargo @("deny", "check"))
}

# --- run -----------------------------------------------------------------------

Invoke-Step "no inline suppressions" { Test-NoSuppressions }
Invoke-Step "no nested lint config" { Test-NoNestedConfig }
Invoke-Step "nothing that emails anybody" { Test-NoMail }
Invoke-Step "no attribution in tracked files" {
    Invoke-Native "powershell.exe" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
        (Join-Path $PSScriptRoot "check-attribution.ps1"), "-Files")
}
Invoke-Step "windows is the only target" { Test-WindowsOnly }
Invoke-Step "detached from upstream" { Test-NoUpstreamCoupling }
Invoke-Step "no host outside the allowlist" { Test-NoNewHosts }
Invoke-Step "nothing can update itself" { Test-NoSelfUpdate }
Invoke-Step "encodings" { Test-Encodings }
Invoke-Step "ruff (lint + format)" { Invoke-Ruff }
Invoke-Step "mypy" { Invoke-Mypy }
Invoke-Step "PSScriptAnalyzer" { Invoke-ScriptAnalyzerCheck }
Invoke-Step "PowerShell formatting" { Test-PowerShellFormat }
Invoke-Step "markdownlint" { Invoke-Markdownlint }
Invoke-Step "yamllint" { Invoke-Yamllint }
Invoke-Step "typos" { Invoke-Typos }
Invoke-Step "shellcheck (hook shims)" { Invoke-Shellcheck }
Invoke-Step "vulture (dead code)" { Invoke-Vulture }
Invoke-Step "module self-checks" { Invoke-SelfChecks }
Invoke-Step "rust format" { Invoke-CargoFormat }
Invoke-Step "clippy" { Invoke-Clippy }
Invoke-Step "rust tests and snapshots" { Invoke-CargoTest }
Invoke-Step "rustdoc" { Invoke-CargoDoc }
Invoke-Step "cargo deny" { Invoke-CargoDeny }
Invoke-Step "comment strip round-trip" { Test-StripRoundTrip }
Invoke-Step "gitleaks (worktree + history)" { Invoke-Gitleaks }
Invoke-Step "secrets (project patterns)" { Test-Secrets }
Invoke-Step "dependency advisories" { Invoke-PipAudit }
Invoke-Step "VERSION agrees with runtime.json" { Invoke-Verifier "verify-version.ps1" }
Invoke-Step "requirements.txt is fully pinned" { Invoke-Verifier "verify-requirements-lock.ps1" }
Invoke-Step "startup path installs nothing" { Invoke-Verifier "verify-no-runtime-installs.ps1" }

Write-Host ""
if ($Failed.Count -gt 0) {
    Write-Host "FAILED: $($Failed -join ', ')" -ForegroundColor Red
    exit 1
}
Write-Host "all checks passed" -ForegroundColor Green
exit 0
